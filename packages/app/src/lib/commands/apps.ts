import { Result } from "better-result";
import type { CommandResult } from "./invokeResult";
import { invokeResult } from "./invokeResult";

export type AppStyle = "professional" | "casual" | "agents" | "default";

export const STYLE_LIST: readonly AppStyle[] = ["professional", "casual", "agents", "default"];

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
	url?: string | null;
	domain?: string | null;
}

export interface AppInstruction {
	id: string;
	bundle_id: string;
	app_name: string;
	style: AppStyle;
	created_at: string;
	updated_at: string;
}

export interface SiteStyle {
	id: string;
	domain: string;
	style: AppStyle;
	created_at: string;
	updated_at: string;
}

let installedAppsCache: InstalledApp[] | null = null;
let installedAppsInflight: Promise<CommandResult<InstalledApp[]>> | null = null;

export function getCachedInstalledApps(): InstalledApp[] | null {
	return installedAppsCache;
}

export async function listInstalledApps(): Promise<CommandResult<InstalledApp[]>> {
	if (installedAppsCache) return Result.ok(installedAppsCache);
	if (installedAppsInflight) return installedAppsInflight;
	installedAppsInflight = invokeResult<InstalledApp[]>("list_installed_apps").then((result) => {
		if (Result.isOk(result)) installedAppsCache = result.value;
		installedAppsInflight = null;
		return result;
	});
	return installedAppsInflight;
}

export async function getFrontmostApp(): Promise<CommandResult<FrontmostApp | null>> {
	return invokeResult<FrontmostApp | null>("get_frontmost_app");
}

export async function listAppInstructions(): Promise<CommandResult<AppInstruction[]>> {
	return invokeResult<AppInstruction[]>("list_app_instructions");
}

export async function setAppInstruction(
	bundleId: string,
	appName: string,
	style: AppStyle
): Promise<CommandResult<AppInstruction>> {
	return invokeResult<AppInstruction>("set_app_instruction", {
		bundleId,
		appName,
		style,
	});
}

export async function deleteAppInstruction(id: string): Promise<CommandResult<void>> {
	return invokeResult<void>("delete_app_instruction", { id });
}

export async function listSiteStyles(): Promise<CommandResult<SiteStyle[]>> {
	return invokeResult<SiteStyle[]>("list_site_styles");
}

export async function setSiteStyle(
	domain: string,
	style: AppStyle
): Promise<CommandResult<SiteStyle>> {
	return invokeResult<SiteStyle>("set_site_style", { domain, style });
}

export async function deleteSiteStyle(id: string): Promise<CommandResult<void>> {
	return invokeResult<void>("delete_site_style", { id });
}
