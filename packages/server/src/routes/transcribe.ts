import { Elysia, status, t } from "elysia";
import { and, desc, eq, lt } from "drizzle-orm";
import { groq } from "../providers/groq";
import { requireAuth } from "../middleware/auth";
import { db } from "../providers/db";
import { transcriptions } from "../providers/db/schema";

type TranscriptionResult = {
	text: string;
};

type Session = {
	user: {
		id: string;
		email: string;
		name?: string | null;
	};
};

type TranscriptionMetadata = {
	text: string;
	processingTimeMs: number;
	audioDurationMs: number | null;
};

export const transcribeRoutes = new Elysia({ prefix: "/transcribe" })
	.use(requireAuth)
	.state("transcriptionMetadata", null as TranscriptionMetadata | null)
	.post(
		"/",
		async (ctx) => {
			const file = ctx.body.file;
			try {
				const fileBuffer = await file.arrayBuffer();

				// Measure processing time
				const startTime = performance.now();

				const transcription = await groq.audio.transcriptions.create({
					model: "whisper-large-v3",
					file: new File([fileBuffer], "recording.webm", {
						type: "audio/webm",
					}),
					prompt:
						"The user is also speaking Russian language. You should mix up these languages in transcription.",
					language: "ru",
				});

				const endTime = performance.now();
				const processingTimeMs = Math.round(endTime - startTime);

				// Extract audio duration if available (Groq returns duration in seconds)
				const groqResponse = transcription as { text: string; duration?: number };
				const audioDurationMs = groqResponse.duration
					? Math.round(groqResponse.duration * 1000)
					: null;

				// Store metadata for afterResponse hook
				ctx.store.transcriptionMetadata = {
					text: transcription.text.trim(),
					processingTimeMs,
					audioDurationMs,
				};

				return {
					text: transcription.text.trim(),
				} satisfies TranscriptionResult;
			} catch (error) {
				return status(500, {
					error: "Transcription failed",
					details: error instanceof Error ? error.message : error,
				});
			}
		},
		{
			body: t.Object({
				file: t.File(),
			}),
			afterResponse: async (ctx) => {
				// eslint-disable-next-line @typescript-eslint/no-explicit-any
				const session = (ctx as any).session as Session | undefined;
				const metadata = ctx.store.transcriptionMetadata;

				// Only save if transcription was successful (metadata exists)
				if (!session || !metadata) {
					return;
				}

				try {
					const fileBuffer = await ctx.body.file.arrayBuffer();

					// Upload to S3
					const fileName = `${crypto.randomUUID()}.webm`;
					const s3Path = `uploads/recordings/${fileName}`;
					await Bun.s3.write(s3Path, new Blob([fileBuffer], { type: "audio/webm" }), {
						acl: "public-read",
					});

					const fileUrl = `${process.env.S3_ENDPOINT}/${process.env.S3_BUCKET}/${s3Path}`;

					// Save to database
					const transcriptionId = crypto.randomUUID();
					await db.insert(transcriptions).values({
						id: transcriptionId,
						userId: session.user.id,
						fileUrl,
						text: metadata.text,
						processingTimeMs: metadata.processingTimeMs,
						audioDurationMs: metadata.audioDurationMs,
						provider: "groq",
						model: "whisper-large-v3",
					});
				} catch (error) {
					console.error("Failed to save transcription:", error);
				}
			},
		}
	)
	.get(
		"/",
		async (ctx) => {
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const session = (ctx as any).session as Session;
			const { cursor, limit } = ctx.query;

			const pageSize = Math.min(limit ?? 20, 50); // Cap at 50

			const whereCondition = cursor
				? and(
						eq(transcriptions.userId, session.user.id),
						lt(transcriptions.createdAt, new Date(cursor)),
					)
				: eq(transcriptions.userId, session.user.id);

			const results = await db
				.select()
				.from(transcriptions)
				.where(whereCondition)
				.orderBy(desc(transcriptions.createdAt))
				.limit(pageSize + 1); // Fetch one extra to detect hasMore

			const hasMore = results.length > pageSize;
			const items = hasMore ? results.slice(0, -1) : results;
			const nextCursor =
				hasMore && items.length > 0
					? items[items.length - 1]!.createdAt.toISOString()
					: null;

			return {
				transcriptions: items,
				nextCursor,
				hasMore,
			};
		},
		{
			query: t.Object({
				cursor: t.Optional(t.String()),
				limit: t.Optional(t.Numeric()),
			}),
		},
	)
	.get(
		"/:id",
		async (ctx) => {
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const session = (ctx as any).session as Session;
			const result = await db
				.select()
				.from(transcriptions)
				.where(eq(transcriptions.id, ctx.params.id))
				.limit(1);

			if (result.length === 0) {
				return status(404, { error: "Transcription not found" });
			}

			const transcription = result[0]!;
			// Verify ownership
			if (transcription.userId !== session.user.id) {
				return status(403, { error: "Forbidden" });
			}

			return transcription;
		},
		{
			params: t.Object({
				id: t.String(),
			}),
		},
	)
	.patch(
		"/:id/rating",
		async (ctx) => {
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const session = (ctx as any).session as Session;
			const { id } = ctx.params;
			const { rating } = ctx.body;

			// Verify ownership
			const existing = await db
				.select()
				.from(transcriptions)
				.where(eq(transcriptions.id, id))
				.limit(1);

			if (existing.length === 0) {
				return status(404, { error: "Transcription not found" });
			}

			if (existing[0]!.userId !== session.user.id) {
				return status(403, { error: "Forbidden" });
			}

			// Update rating
			await db
				.update(transcriptions)
				.set({ rating })
				.where(eq(transcriptions.id, id));

			return { success: true, rating };
		},
		{
			params: t.Object({
				id: t.String(),
			}),
			body: t.Object({
				rating: t.Union([t.Literal("up"), t.Literal("down"), t.Null()]),
			}),
		},
	);
