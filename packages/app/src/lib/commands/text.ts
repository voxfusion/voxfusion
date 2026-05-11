import { invoke } from "@tauri-apps/api/core";

export async function typeText(text: string): Promise<void> {
	await invoke("type_text", { text });
}
