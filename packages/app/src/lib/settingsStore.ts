import { emit } from "@tauri-apps/api/event";
import { load } from "@tauri-apps/plugin-store";
import { createSignal } from "solid-js";
import type { Locale } from "../i18n";

export type Theme = "dark" | "light" | "system";
export type AudioQuality = "high" | "medium" | "low";

export interface Settings {
	theme: Theme;
	hotkey: string;
	selectedMicrophoneId: string | null;
	language: Locale;
	audioQuality: AudioQuality;
	onboardingComplete: boolean;
	onboardingStep: number;
}

const DEFAULT_SETTINGS: Settings = {
	theme: "system",
	hotkey: "Command+;",
	selectedMicrophoneId: null,
	language: "en",
	audioQuality: "high",
	onboardingComplete: false,
	onboardingStep: 1,
};

const STORE_NAME = "settings.json";

let storeInstance: Awaited<ReturnType<typeof load>> | null = null;

async function getStore() {
	if (!storeInstance) {
		storeInstance = await load(STORE_NAME, {
			defaults: {
				theme: DEFAULT_SETTINGS.theme,
				hotkey: DEFAULT_SETTINGS.hotkey,
				selectedMicrophoneId: DEFAULT_SETTINGS.selectedMicrophoneId,
				language: DEFAULT_SETTINGS.language,
				audioQuality: DEFAULT_SETTINGS.audioQuality,
				onboardingComplete: DEFAULT_SETTINGS.onboardingComplete,
				onboardingStep: DEFAULT_SETTINGS.onboardingStep,
			},
			autoSave: 500,
		});
	}
	return storeInstance;
}

export async function loadSettings(): Promise<Settings> {
	const store = await getStore();
	const theme = await store.get<Theme>("theme");
	const hotkey = await store.get<string>("hotkey");
	const selectedMicrophoneId = await store.get<string | null>("selectedMicrophoneId");
	const language = await store.get<Locale>("language");
	const audioQuality = await store.get<AudioQuality>("audioQuality");
	const onboardingComplete = await store.get<boolean>("onboardingComplete");
	const onboardingStep = await store.get<number>("onboardingStep");

	return {
		theme: theme ?? DEFAULT_SETTINGS.theme,
		hotkey: hotkey ?? DEFAULT_SETTINGS.hotkey,
		selectedMicrophoneId: selectedMicrophoneId ?? DEFAULT_SETTINGS.selectedMicrophoneId,
		language: language ?? DEFAULT_SETTINGS.language,
		audioQuality: audioQuality ?? DEFAULT_SETTINGS.audioQuality,
		onboardingComplete: onboardingComplete ?? DEFAULT_SETTINGS.onboardingComplete,
		onboardingStep: onboardingStep ?? DEFAULT_SETTINGS.onboardingStep,
	};
}

export async function saveTheme(theme: Theme): Promise<void> {
	const store = await getStore();
	await store.set("theme", theme);
	try {
		localStorage.setItem("voxfusion-theme", theme);
	} catch (e) {}
}

export async function saveHotkey(hotkey: string): Promise<void> {
	const store = await getStore();
	await store.set("hotkey", hotkey);
}

export async function saveMicrophone(microphoneId: string | null): Promise<void> {
	const store = await getStore();
	await store.set("selectedMicrophoneId", microphoneId);
}

export async function saveLanguage(language: Locale): Promise<void> {
	const store = await getStore();
	await store.set("language", language);
}

export async function saveAudioQuality(quality: AudioQuality): Promise<void> {
	const store = await getStore();
	await store.set("audioQuality", quality);
}

export interface AudioDevice {
	name: string;
	isDefault: boolean;
}

export async function getAudioInputDevices(): Promise<AudioDevice[]> {
	try {
		const { invoke } = await import("@tauri-apps/api/core");
		const devices = await invoke<{ name: string; is_default: boolean }[]>("list_audio_devices");
		return devices.map((device) => ({
			name: device.name,
			isDefault: device.is_default,
		}));
	} catch (error) {
		console.error("Failed to enumerate audio devices:", error);
		return [];
	}
}

const [settings, setSettingsInternal] = createSignal<Settings>(DEFAULT_SETTINGS);

export function useSettings() {
	return settings;
}

export async function initSettings(): Promise<void> {
	const loaded = await loadSettings();
	setSettingsInternal(loaded);
	applyTheme(loaded.theme);
	try {
		localStorage.setItem("voxfusion-theme", loaded.theme);
	} catch (e) {}
}

export async function updateTheme(theme: Theme): Promise<void> {
	await saveTheme(theme);
	setSettingsInternal((prev) => ({ ...prev, theme }));
	applyTheme(theme);
}

export async function updateHotkey(hotkey: string): Promise<void> {
	await saveHotkey(hotkey);
	setSettingsInternal((prev) => ({ ...prev, hotkey }));
	await emit("settings-changed");
}

export async function updateMicrophone(microphoneId: string | null): Promise<void> {
	await saveMicrophone(microphoneId);
	setSettingsInternal((prev) => ({ ...prev, selectedMicrophoneId: microphoneId }));
	await emit("settings-changed");
}

export async function updateLanguage(
	language: Locale,
	setLocale: (locale: Locale) => void
): Promise<void> {
	await saveLanguage(language);
	setSettingsInternal((prev) => ({ ...prev, language }));
	setLocale(language);
}

export async function updateAudioQuality(quality: AudioQuality): Promise<void> {
	await saveAudioQuality(quality);
	setSettingsInternal((prev) => ({ ...prev, audioQuality: quality }));
	await emit("settings-changed");
}

function applyTheme(theme: Theme): void {
	const root = document.documentElement;

	if (theme === "system") {
		const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
		root.classList.toggle("dark", prefersDark);
	} else {
		root.classList.toggle("dark", theme === "dark");
	}
}

if (typeof window !== "undefined") {
	window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
		const currentSettings = settings();
		if (currentSettings.theme === "system") {
			applyTheme("system");
		}
	});
}

export async function updateOnboardingStep(step: number): Promise<void> {
	const store = await getStore();
	await store.set("onboardingStep", step);
	await store.save();
	setSettingsInternal((prev) => ({ ...prev, onboardingStep: step }));
}

export async function markOnboardingComplete(): Promise<void> {
	const store = await getStore();
	await store.set("onboardingComplete", true);
	await store.set("onboardingStep", 1);
	await store.save();
	setSettingsInternal((prev) => ({ ...prev, onboardingComplete: true, onboardingStep: 1 }));
	await emit("settings-changed");
}

export async function resetOnboarding(): Promise<void> {
	const store = await getStore();
	await store.set("onboardingComplete", false);
	setSettingsInternal((prev) => ({ ...prev, onboardingComplete: false }));
}
