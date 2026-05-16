import { invoke } from "@tauri-apps/api/core";

export type AppStyle = "professional" | "casual" | "agents" | "default";

export const STYLE_LIST: readonly AppStyle[] = [
	"professional",
	"casual",
	"agents",
	"default",
];

export const DEFAULT_STYLE: AppStyle = "default";

export interface InstalledApp {
	name: string;
	bundle_id: string;
	path: string;
	icon_data_url: string | null;
}

export interface FrontmostApp {
	name: string;
	bundle_id: string;
}

export interface AppInstruction {
	id: string;
	bundle_id: string;
	app_name: string;
	style: AppStyle;
	created_at: string;
	updated_at: string;
}

export async function listInstalledApps(): Promise<InstalledApp[]> {
	return invoke<InstalledApp[]>("list_installed_apps");
}

export async function getFrontmostApp(): Promise<FrontmostApp | null> {
	return invoke<FrontmostApp | null>("get_frontmost_app");
}

export async function listAppInstructions(): Promise<AppInstruction[]> {
	return invoke<AppInstruction[]>("list_app_instructions");
}

export async function setAppInstruction(
	bundleId: string,
	appName: string,
	style: AppStyle
): Promise<AppInstruction> {
	return invoke<AppInstruction>("set_app_instruction", {
		bundleId,
		appName,
		style,
	});
}

export async function deleteAppInstruction(id: string): Promise<void> {
	await invoke("delete_app_instruction", { id });
}

