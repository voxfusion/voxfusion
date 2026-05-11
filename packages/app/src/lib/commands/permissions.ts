import { invoke } from "@tauri-apps/api/core";

export async function checkAccessibilityProbe(): Promise<boolean> {
	return invoke<boolean>("check_accessibility_probe");
}
