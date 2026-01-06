import { Elysia, status, t } from "elysia";
import { groq } from "../providers/groq";
import { requireAuth } from "../middleware/auth";
import type { Uploadable } from "groq-sdk/uploads";

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
				}) as unknown as Uploadable,
			});

			const fileName = `${crypto.randomUUID()}.webm`;
			const s3Path = `uploads/recordings/${fileName}`;
			Bun.s3.write(s3Path, new Blob([fileBuffer], { type: "audio/webm" }));

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
);
