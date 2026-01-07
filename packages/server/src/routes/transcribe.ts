import { Elysia, status, t } from "elysia";
import { and, desc, eq, lt } from "drizzle-orm";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { groq } from "../providers/groq";
import { db } from "../providers/db";
import { transcriptions } from "../providers/db/schema";
import { auth } from "../auth";

async function getAudioDuration(buffer: ArrayBuffer): Promise<number | null> {
	const tempFile = join(tmpdir(), `audio-${crypto.randomUUID()}.webm`);
	try {
		// Write buffer to temp file
		await Bun.write(tempFile, buffer);

		// Use ffprobe to get duration
		const proc = Bun.spawn(
			["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", tempFile],
			{ stdout: "pipe", stderr: "pipe" }
		);
		const output = await new Response(proc.stdout).text();
		await proc.exited;

		const duration = Number.parseFloat(output.trim());
		if (!Number.isNaN(duration)) {
			return Math.round(duration * 1000);
		}
		return null;
	} catch (err) {
		console.error("Failed to get audio duration:", err);
		return null;
	} finally {
		// Clean up temp file
		try {
			(await Bun.file(tempFile).exists()) && (await Bun.$`rm ${tempFile}`);
		} catch {}
	}
}

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
	fileBuffer: ArrayBuffer;
};

export const transcribeRoutes = new Elysia({ prefix: "/transcribe" })
	.derive(async ({ request }) => {
		const session = await auth.api.getSession({
			headers: request.headers,
		});
		if (!session?.user) {
			return status(401, { error: "Unauthorized" });
		}
		return { session };
	})
	.state("transcriptionMetadata", null as TranscriptionMetadata | null)
	.post(
		"/",
		async (ctx) => {
			const file = ctx.body.file;
			try {
				const fileBuffer = await file.arrayBuffer();

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

				ctx.store.transcriptionMetadata = {
					text: transcription.text.trim(),
					processingTimeMs,
					fileBuffer,
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
				const session = (ctx as any).session as Session | undefined;
				const metadata = ctx.store.transcriptionMetadata;

				if (!session || !metadata) {
					return;
				}

				try {
					// Extract audio duration (non-blocking for user)
					const audioDurationMs = await getAudioDuration(metadata.fileBuffer);

					const fileName = `${crypto.randomUUID()}.webm`;
					const s3Path = `uploads/recordings/${fileName}`;
					await Bun.s3.write(s3Path, new Blob([metadata.fileBuffer], { type: "audio/webm" }), {
						acl: "public-read",
					});

					const fileUrl = `${process.env.S3_ENDPOINT}/${process.env.S3_BUCKET}/${s3Path}`;

					const transcriptionId = crypto.randomUUID();
					await db.insert(transcriptions).values({
						id: transcriptionId,
						userId: session.user.id,
						fileUrl,
						text: metadata.text,
						processingTimeMs: metadata.processingTimeMs,
						audioDurationMs,
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
			const session = (ctx as any).session as Session | undefined;

			if (!session?.user) {
				return {
					transcriptions: [],
					nextCursor: null,
					hasMore: false,
				};
			}

			const { cursor, limit } = ctx.query;
			const pageSize = Math.min(limit ?? 20, 50);

			const whereCondition = cursor
				? and(
						eq(transcriptions.userId, session.user.id),
						lt(transcriptions.createdAt, new Date(cursor))
					)
				: eq(transcriptions.userId, session.user.id);

			const results = await db
				.select()
				.from(transcriptions)
				.where(whereCondition)
				.orderBy(desc(transcriptions.createdAt))
				.limit(pageSize + 1);

			const hasMore = results.length > pageSize;
			const items = hasMore ? results.slice(0, -1) : results;
			const nextCursor =
				hasMore && items.length > 0 ? items[items.length - 1]!.createdAt.toISOString() : null;

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
		}
	)
	.get(
		"/:id",
		async (ctx) => {
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
			if (transcription.userId !== session.user.id) {
				return status(403, { error: "Forbidden" });
			}

			return transcription;
		},
		{
			params: t.Object({
				id: t.String(),
			}),
		}
	)
	.patch(
		"/:id/rating",
		async (ctx) => {
			const session = (ctx as any).session as Session;
			const { id } = ctx.params;
			const { rating } = ctx.body;

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

			await db.update(transcriptions).set({ rating }).where(eq(transcriptions.id, id));

			return { success: true, rating };
		},
		{
			params: t.Object({
				id: t.String(),
			}),
			body: t.Object({
				rating: t.Union([t.Literal("up"), t.Literal("down"), t.Null()]),
			}),
		}
	);
