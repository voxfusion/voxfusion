import { Elysia, status, t } from "elysia";
import { groq } from "../providers/groq";
import { requireAuth } from "../middleware/auth";
// import type { Uploadable } from "groq-sdk/uploads.mjs";

type TranscriptionResult = {
	text: string;
};

export const transcribeRoutes = new Elysia({ prefix: "/transcribe" }).use(requireAuth).post(
	"/",
	async ({ body }) => {
		const file = body.file;
		try {
			const fileBuffer = await file.arrayBuffer();

			const transcription = await groq.audio.transcriptions.create({
				model: "whisper-large-v3",
				file: new File([fileBuffer], "recording.webm", {
					type: "audio/webm",
				}),
				prompt:
					"The user is also speaking Russian language. You should mix up these languages in transcription.",
				language: "ru",
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
		afterResponse: async ({ body }) => {
			const fileBuffer = await body.file.arrayBuffer();

			const fileName = `${crypto.randomUUID()}.webm`;
			const s3Path = `uploads/recordings/${fileName}`;
			await Bun.s3.write(s3Path, new Blob([fileBuffer], { type: "audio/webm" }), {
				acl: "public-read",
			});
		},
	}
);
