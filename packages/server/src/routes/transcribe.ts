import { Elysia, status, t } from "elysia";
import { desc, eq } from "drizzle-orm";
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

export const transcribeRoutes = new Elysia({ prefix: "/transcribe" })
	.use(requireAuth)
	.post(
		"/",
		async (ctx) => {
			const file = ctx.body.file;
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const session = (ctx as any).session as Session;
			try {
				const fileBuffer = await file.arrayBuffer();

				// Upload to S3 first (moved from afterResponse so URL is available)
				const fileName = `${crypto.randomUUID()}.webm`;
				const s3Path = `uploads/recordings/${fileName}`;
				await Bun.s3.write(s3Path, new Blob([fileBuffer], { type: "audio/webm" }), {
					acl: "public-read",
				});

				const fileUrl = `${process.env.S3_ENDPOINT}/${process.env.S3_BUCKET}/${s3Path}`;

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

				// Save to database
				const transcriptionId = crypto.randomUUID();
				await db.insert(transcriptions).values({
					id: transcriptionId,
					userId: session.user.id,
					fileUrl,
					text: transcription.text.trim(),
					processingTimeMs,
					audioDurationMs,
					provider: "groq",
					model: "whisper-large-v3",
				});

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
		}
	)
	.get("/", async (ctx) => {
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		const session = (ctx as any).session as Session;
		const userTranscriptions = await db
			.select()
			.from(transcriptions)
			.where(eq(transcriptions.userId, session.user.id))
			.orderBy(desc(transcriptions.createdAt));

		return { transcriptions: userTranscriptions };
	})
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
		}
	);
