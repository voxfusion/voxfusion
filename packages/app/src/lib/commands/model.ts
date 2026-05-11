import { invoke } from "@tauri-apps/api/core";

export async function checkModelStatus(): Promise<boolean> {
	return invoke<boolean>("check_model_status");
}

export async function downloadWhisperModel(): Promise<void> {
	await invoke("download_whisper_model");
}
