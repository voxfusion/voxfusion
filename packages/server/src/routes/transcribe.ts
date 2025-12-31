import { Elysia, t } from "elysia";
import { groq } from "../providers/groq";

export const transcribeRoutes = new Elysia({ prefix: "/transcribe" }).post("/", async ({ body }) => {
    console.log("Transcribing...");
    const file = body.file;
    console.log(file);

    try {
        const fileBuffer = await file.arrayBuffer();

        const transcription = await groq.audio.transcriptions.create({
            model: "whisper-large-v3-turbo",
            file: new File([fileBuffer], "recording.webm", { type: "audio/webm" }),
        });

        const fileName = `${crypto.randomUUID()}.webm`;
        const s3Path = `uploads/recordings/${fileName}`;
        await Bun.s3.write(s3Path, new Blob([fileBuffer], { type: "audio/webm" }));

        return transcription;
    } catch (error) {
        console.error("Error in /transcribe:", error);
        return new Response(
            JSON.stringify({ error: "Transcription failed", details: error instanceof Error ? error.message : error }),
            { status: 500, headers: { "Content-Type": "application/json" } }
        );
    }
}, {
    body: t.Object({
        file: t.File()
    })
})