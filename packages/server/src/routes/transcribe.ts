import { Elysia, status, t } from "elysia";
import { groq } from "../providers/groq";
import { auth } from "../auth";

type TranscriptionResult = {
	text: string;
};

export const transcribeRoutes = new Elysia({ prefix: "/transcribe" }).post(
	"/",
	async ({ body, request }) => {
		console.log("request", request.headers);
		const session = await auth.api.getSession({
			headers: request.headers,
		});
		console.log("session", session);

		if (!session || !session.user) {
			return status(401, { error: "Unauthorized" });
		}

		const file = body.file;
		try {
			const fileBuffer = await file.arrayBuffer();

			const transcription = await groq.audio.transcriptions.create({
				model: "whisper-large-v3",
				file: new File([fileBuffer], "recording.webm", { type: "audio/webm" }),
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
