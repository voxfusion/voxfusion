import { Elysia } from "elysia";
import { groq } from "../providers/groq";

export const transcribeRoutes = new Elysia({ prefix: "/transcribe" }).post("/", async ctx => {
    const formData = await ctx.request.formData();
    const file = formData.get("file") as File;

    const fileBuffer = await file.arrayBuffer();

    const transcription = await groq.audio.transcriptions.create({
        model: "whisper-large-v3-turbo",
        file: new File([fileBuffer], "recording.webm", { type: "audio/webm" }),
    })

    const fileName = `${crypto.randomUUID()}.webm`;
    const s3Path = `uploads/recordings/${fileName}`;
    Bun.s3.write(s3Path, new Blob([fileBuffer], { type: "audio/webm" }));

    return transcription
})