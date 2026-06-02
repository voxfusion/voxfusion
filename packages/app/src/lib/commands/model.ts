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
	model_id: string;
	progress: number;
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

export async function checkModelDownloaded(modelId: string): Promise<CommandResult<boolean>> {
	return invokeResult<boolean>("check_model_downloaded", { modelId });
}
