import { getVersion } from "@tauri-apps/api/app";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Show, createEffect, createSignal } from "solid-js";
import { useHotkeyRecorder } from "../hooks/useHotkeyRecorder";
import { useI18n } from "../i18n";
import { validateHandsFreeHotkey, validateHoldToSpeakHotkey } from "../lib/hotkeyUtils";
import {
	type AudioDevice,
	getAudioInputDevices,
	updateHoldToSpeakHotkey,
	useSettings,
} from "../lib/settingsStore";
import AppearanceSettings from "./settings/AppearanceSettings";
import AudioSettings from "./settings/AudioSettings";
import HotkeySettings from "./settings/HotkeySettings";
import LanguageSettings from "./settings/LanguageSettings";
import SettingsSidebar from "./settings/SettingsSidebar";
import type { SettingsSection } from "./settings/types";

interface SettingsModalProps {
	isOpen: boolean;
	onClose: () => void;
}

const sectionTitles: Record<SettingsSection, string> = {
	audio: "// AUDIO_CONFIG",
	hotkey: "// HOTKEY_CONFIG",
	appearance: "// APPEARANCE_CONFIG",
	language: "// LANGUAGE_CONFIG",
};

export default function SettingsModal(props: SettingsModalProps) {
	const [t, { locale, setLocale }] = useI18n();
	const settings = useSettings();
	const [activeSection, setActiveSection] = createSignal<SettingsSection>("audio");
	const [audioDevices, setAudioDevices] = createSignal<AudioDevice[]>([]);
	const [appVersion, setAppVersion] = createSignal("");
	const [versionCopied, setVersionCopied] = createSignal(false);

	const {
		isRecording: isRecordingHotkey,
		pendingHotkey,
		error: hotkeyError,
		toggleRecording: toggleHotkeyRecording,
	} = useHotkeyRecorder({
		validator: validateHandsFreeHotkey,
		recorderId: "settings-handsfree",
	});
	const {
		isRecording: isRecordingHoldToSpeakHotkey,
		pendingHotkey: pendingHoldToSpeakHotkey,
		error: holdToSpeakHotkeyError,
		toggleRecording: toggleHoldToSpeakHotkeyRecording,
	} = useHotkeyRecorder({
		onHotkeyRecorded: updateHoldToSpeakHotkey,
		validator: validateHoldToSpeakHotkey,
		recorderId: "settings-holdtospeak",
	});

	const handleCopyVersion = async () => {
		if (versionCopied()) return;
		await writeText(appVersion());
		setVersionCopied(true);
		setTimeout(() => setVersionCopied(false), 1500);
	};

	const fetchAudioDevices = async () => {
		const devices = await getAudioInputDevices();
		setAudioDevices(devices);
	};

	createEffect(() => {
		if (props.isOpen) {
			fetchAudioDevices();
			getVersion().then(setAppVersion);
		}
	});

	createEffect(() => {
		if (!props.isOpen) return;

		const handleEscape = (e: KeyboardEvent) => {
			if (e.key === "Escape" && !isRecordingHotkey() && !isRecordingHoldToSpeakHotkey()) {
				props.onClose();
			}
		};

		window.addEventListener("keydown", handleEscape);
		return () => window.removeEventListener("keydown", handleEscape);
	});

	const handleOverlayClick = (e: MouseEvent) => {
		if (e.target === e.currentTarget) {
			props.onClose();
		}
	};

	return (
		<Show when={props.isOpen}>
			{/* biome-ignore lint/a11y/useKeyWithClickEvents: the overlay click mirrors Escape handling. */}
			<div
				class="fixed inset-0 bg-th-overlay flex items-center justify-center z-50"
				onClick={handleOverlayClick}
			>
				<div class="bg-th-base border border-border w-[800px] h-[600px] flex overflow-hidden">
					<SettingsSidebar
						activeSection={activeSection()}
						appVersion={appVersion()}
						versionCopied={versionCopied()}
						onSectionChange={setActiveSection}
						onCopyVersion={handleCopyVersion}
					/>

					<div class="flex-1 flex flex-col">
						<div class="flex items-center justify-between px-6 py-4 border-b border-border">
							<h3 class="font-mono text-txt-primary text-sm tracking-wider uppercase">
								{sectionTitles[activeSection()]}
							</h3>
							<button
								type="button"
								onClick={props.onClose}
								class="font-mono text-txt-muted hover:text-ac transition-colors text-sm"
							>
								[X]
							</button>
						</div>

						<div class="flex-1 overflow-auto p-6">
							<Show when={activeSection() === "audio"}>
								<AudioSettings
									t={t}
									settings={settings}
									audioDevices={audioDevices()}
									onRefreshDevices={fetchAudioDevices}
								/>
							</Show>
							<Show when={activeSection() === "hotkey"}>
								<HotkeySettings
									t={t}
									settings={settings}
									isRecordingHotkey={isRecordingHotkey()}
									pendingHotkey={pendingHotkey()}
									hotkeyError={hotkeyError()}
									onToggleHotkeyRecording={toggleHotkeyRecording}
									isRecordingHoldToSpeakHotkey={isRecordingHoldToSpeakHotkey()}
									pendingHoldToSpeakHotkey={pendingHoldToSpeakHotkey()}
									holdToSpeakHotkeyError={holdToSpeakHotkeyError()}
									onToggleHoldToSpeakHotkeyRecording={toggleHoldToSpeakHotkeyRecording}
								/>
							</Show>
							<Show when={activeSection() === "appearance"}>
								<AppearanceSettings settings={settings} />
							</Show>
							<Show when={activeSection() === "language"}>
								<LanguageSettings locale={locale()} setLocale={setLocale} />
							</Show>
						</div>

						<div class="px-6 py-3 border-t border-border">
							<p class="font-mono text-[10px] text-txt-faint">
								ESC TO CLOSE | CHANGES SAVED AUTOMATICALLY
							</p>
						</div>
					</div>
				</div>
			</div>
		</Show>
	);
}
