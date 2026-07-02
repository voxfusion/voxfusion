import type { CommandResult } from "./invokeResult";
import { invokeResult } from "./invokeResult";

/** Id of the default Whisper model downloaded during onboarding. */
export const DEFAULT_MODEL_ID = "whisper-large-v3-turbo";

export type ModelEngine = "whisper" | "parakeet";

/** A transcription model as reported by the backend registry (`list_models`). */
export interface ModelInfo {
	id: string;
	name: string;
	engine: ModelEngine;
	size_label: string;
	languages: string;
	experimental: boolean;
	recommended: boolean;
	downloaded: boolean;
	active: boolean;
}

/** Payload of the `model-download-progress` event. */
export interface ModelDownloadProgress {
	modelId: string;
	/** Whole percentage, 0-100. */
	progress: number;
	downloadedBytes: number;
	totalBytes: number;
}

export async function checkModelStatus(): Promise<CommandResult<boolean>> {
	return invokeResult<boolean>("check_model_status");
}

export async function downloadWhisperModel(): Promise<CommandResult<void>> {
	return invokeResult<void>("download_whisper_model");
}

export async function listModels(): Promise<CommandResult<ModelInfo[]>> {
	return invokeResult<ModelInfo[]>("list_models");
}

export async function getActiveModel(): Promise<CommandResult<string>> {
	return invokeResult<string>("get_active_model");
}

export async function setActiveModel(modelId: string): Promise<CommandResult<void>> {
	return invokeResult<void>("set_active_model", { modelId });
}

export async function downloadModel(modelId: string): Promise<CommandResult<void>> {
	return invokeResult<void>("download_model", { modelId });
}

/**
 * Requests cancellation of an in-flight download. The partial file is kept,
 * so a later `downloadModel` call resumes where it left off.
 */
export async function cancelModelDownload(modelId: string): Promise<CommandResult<void>> {
	return invokeResult<void>("cancel_model_download", { modelId });
}

export async function checkModelDownloaded(modelId: string): Promise<CommandResult<boolean>> {
	return invokeResult<boolean>("check_model_downloaded", { modelId });
}
