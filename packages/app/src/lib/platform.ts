import { invoke } from "@tauri-apps/api/core";

export const isLinux = typeof navigator !== "undefined" && /Linux/.test(navigator.platform);
export const isMacOS = typeof navigator !== "undefined" && /Mac/.test(navigator.platform);
export interface PlatformInfo {
	os: string;
	wayland: boolean;
	hyprland: boolean;
	textInsertionAvailable: boolean;
}
let info: PlatformInfo | undefined;
export async function getPlatformInfo(): Promise<PlatformInfo> {
	info ??= await invoke<PlatformInfo>("platform_info");
	return info;
}
export function isWayland(): boolean {
	return info?.wayland ?? false;
}
