import { invoke } from "@tauri-apps/api/core";

export async function muteMediaForRecording(): Promise<void> {
	await invoke("mute_media_for_recording");
}

export async function restoreMediaAfterRecording(): Promise<void> {
	await invoke("restore_media_after_recording");
}
