import type { AppStyle } from "./apps";
import type { CommandResult } from "./invokeResult";
import { invokeResult } from "./invokeResult";

export interface Transcription {
	id: string;
	text: string;
	word_count: number;
	processing_time_ms: number;
	audio_duration_ms: number | null;
	created_at: string;
}

export interface TranscriptionPage {
	transcriptions: Transcription[];
	has_more: boolean;
}

export interface TranscriptionResult {
	text: string;
	word_count: number;
	processing_time_ms: number;
	audio_duration_ms: number | null;
}

export async function listTranscriptions(
	limit: number,
	cursor: string | null
): Promise<CommandResult<TranscriptionPage>> {
	return invokeResult<TranscriptionPage>("list_transcriptions", { limit, cursor });
}

export async function transcribeAudio(
	audioPath: string,
	bundleId: string | null,
	fallbackStyle: AppStyle
): Promise<CommandResult<TranscriptionResult>> {
	return invokeResult<TranscriptionResult>("transcribe_audio", {
		audioPath,
		bundleId,
		fallbackStyle,
	});
}

export async function saveTranscription(
	result: TranscriptionResult
): Promise<CommandResult<Transcription>> {
	return invokeResult<Transcription>("save_transcription", {
		text: result.text,
		wordCount: result.word_count,
		processingTimeMs: result.processing_time_ms,
		audioDurationMs: result.audio_duration_ms,
	});
}
