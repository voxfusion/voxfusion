import { getVersion } from "@tauri-apps/api/app";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Check, ChevronDown, Copy, RefreshCw } from "lucide-solid";
import { For, Show, createEffect, createSignal, onCleanup } from "solid-js";
import { useHotkeyRecorder } from "../hooks/useHotkeyRecorder";
import { type Locale, useI18n } from "../i18n";
import { hotkeyDisplayName } from "../lib/hotkeyUtils";
import { capture } from "../lib/posthog";
import {
	type AudioDevice,
	type AudioQuality,
	type Theme,
	getAudioInputDevices,
	updateAudioQuality,
	updateLanguage,
	updateMicrophone,
	updateTheme,
	useSettings,
} from "../lib/settingsStore";

interface SettingsModalProps {
	isOpen: boolean;
	onClose: () => void;
}

type SettingsSection = "audio" | "hotkey" | "appearance" | "language";

interface SelectOption {
	value: string;
	label: string;
}

interface SelectProps {
	value: string;
	options: SelectOption[];
	onChange: (value: string) => void;
	placeholder?: string;
}

function Select(props: SelectProps) {
	const [isOpen, setIsOpen] = createSignal(false);
	let containerRef: HTMLDivElement | undefined;

	const selectedLabel = () => {
		const option = props.options.find((o) => o.value === props.value);
		return option?.label ?? props.placeholder ?? "";
	};

	const handleClickOutside = (e: MouseEvent) => {
		if (containerRef && !containerRef.contains(e.target as Node)) {
			setIsOpen(false);
		}
	};

	createEffect(() => {
		if (isOpen()) {
			document.addEventListener("click", handleClickOutside);
		} else {
			document.removeEventListener("click", handleClickOutside);
		}
	});

	onCleanup(() => {
		document.removeEventListener("click", handleClickOutside);
	});

	return (
		<div ref={containerRef} class="relative">
			<button
				type="button"
				onClick={() => setIsOpen(!isOpen())}
				class="flex items-center justify-between w-full px-4 py-2.5 bg-th-surface border border-border-strong text-txt-primary hover:border-ac focus:outline-none focus:border-ac transition-colors font-mono text-sm"
			>
				<span class="truncate">{selectedLabel()}</span>
				<ChevronDown
					class={`w-4 h-4 ml-2 text-txt-muted transition-transform ${isOpen() ? "rotate-180" : ""}`}
				/>
			</button>

			<Show when={isOpen()}>
				<div class="absolute z-50 w-full mt-1 bg-th-surface border border-border-strong max-h-60 overflow-auto">
					<For each={props.options}>
						{(option) => (
							<button
								type="button"
								onClick={() => {
									props.onChange(option.value);
									setIsOpen(false);
								}}
								class={`flex items-center justify-between w-full px-4 py-2.5 text-left hover:bg-th-hover transition-colors font-mono text-sm ${
									option.value === props.value ? "text-ac bg-th-hover" : "text-txt-primary"
								}`}
							>
								<span class="truncate">{option.label}</span>
								<Show when={option.value === props.value}>
									<span class="text-ac">[*]</span>
								</Show>
							</button>
						)}
					</For>
				</div>
			</Show>
		</div>
	);
}

export default function SettingsModal(props: SettingsModalProps) {
	const [t, { locale, setLocale }] = useI18n();
	const settings = useSettings();
	const [activeSection, setActiveSection] = createSignal<SettingsSection>("audio");
	const [audioDevices, setAudioDevices] = createSignal<AudioDevice[]>([]);
	const {
		isRecording: isRecordingHotkey,
		pendingHotkey,
		toggleRecording: toggleHotkeyRecording,
	} = useHotkeyRecorder();
	const [isLoadingDevices, setIsLoadingDevices] = createSignal(false);
	const [appVersion, setAppVersion] = createSignal<string>("");
	const [versionCopied, setVersionCopied] = createSignal(false);

	const handleCopyVersion = async () => {
		if (versionCopied()) return;
		await writeText(appVersion());
		setVersionCopied(true);
		setTimeout(() => setVersionCopied(false), 1500);
	};

	const fetchAudioDevices = async () => {
		setIsLoadingDevices(true);
		try {
			const devices = await getAudioInputDevices();
			setAudioDevices(devices);
		} catch {
			// Failed to fetch audio devices
		} finally {
			setIsLoadingDevices(false);
		}
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
			if (e.key === "Escape" && !isRecordingHotkey()) {
				props.onClose();
			}
		};

		window.addEventListener("keydown", handleEscape);
		return () => window.removeEventListener("keydown", handleEscape);
	});

	const sidebarItems = [
		{ id: "audio" as const, num: "01", label: "AUDIO" },
		{ id: "hotkey" as const, num: "02", label: "HOTKEY" },
		{ id: "appearance" as const, num: "03", label: "APPEARANCE" },
		{ id: "language" as const, num: "04", label: "LANGUAGE" },
	];

	const handleOverlayClick = (e: MouseEvent) => {
		if (e.target === e.currentTarget) {
			props.onClose();
		}
	};

	const microphoneOptions = (): SelectOption[] => {
		const devices = audioDevices();
		const defaultDevice = devices.find((d) => d.isDefault);
		const otherDevices = devices.filter((d) => !d.isDefault);

		return [
			{
				value: "default",
				label: defaultDevice
					? `${t("settings.defaultMicrophone")} (${defaultDevice.name})`
					: t("settings.defaultMicrophone"),
			},
			...otherDevices.map((device) => ({
				value: device.name,
				label: device.name,
			})),
		];
	};

	const languageOptions: SelectOption[] = [
		{ value: "en", label: "ENGLISH" },
		{ value: "ru", label: "RUSSIAN" },
		{ value: "es", label: "ESPAÑOL" },
		{ value: "zh", label: "中文" },
		{ value: "ja", label: "日本語" },
		{ value: "ko", label: "한국어" },
		{ value: "de", label: "DEUTSCH" },
		{ value: "fr", label: "FRANÇAIS" },
		{ value: "it", label: "ITALIANO" },
		{ value: "sv", label: "SVENSKA" },
		{ value: "hi", label: "हिन्दी" },
		{ value: "uk", label: "UKRAINIAN" },
	];

	return (
		<Show when={props.isOpen}>
			<div
				class="fixed inset-0 bg-th-overlay flex items-center justify-center z-50"
				onClick={handleOverlayClick}
			>
				<div class="bg-th-base border border-border w-[800px] h-[600px] flex overflow-hidden">
					{/* Sidebar */}
					<div class="w-56 bg-th-base border-r border-border flex flex-col">
						{/* Header */}
						<div class="px-4 py-4 border-b border-border">
							<h2 class="font-mono text-ac text-sm tracking-wider">[VOXFUSION] &gt; SETTINGS</h2>
						</div>
						{/* Navigation */}
						<nav class="flex-1 py-2">
							<For each={sidebarItems}>
								{(item) => (
									<button
										type="button"
										onClick={() => setActiveSection(item.id)}
										class={`flex items-center gap-3 w-full px-4 py-3 font-mono text-xs tracking-wider transition-colors ${
											activeSection() === item.id
												? "text-ac border-l-2 border-ac bg-th-surface"
												: "text-txt-muted hover:text-txt-secondary hover:bg-th-surface border-l-2 border-transparent"
										}`}
									>
										<span class="text-txt-faint">{item.num}</span>
										<span>{item.label}</span>
									</button>
								)}
							</For>
						</nav>
						<Show when={appVersion()}>
							<div class="px-4 py-3 border-t border-border">
								<button
									type="button"
									onClick={handleCopyVersion}
									class="group flex items-center gap-1.5 font-mono text-[10px] text-txt-faint hover:text-txt-muted transition-all active:scale-95 cursor-pointer"
								>
									<span>v{appVersion()}</span>
									<Show
										when={versionCopied()}
										fallback={
											<Copy class="w-3 h-3 opacity-0 group-hover:opacity-100 transition-opacity" />
										}
									>
										<Check class="w-3 h-3 text-ac" />
									</Show>
								</button>
							</div>
						</Show>
					</div>

					{/* Content */}
					<div class="flex-1 flex flex-col">
						{/* Content Header */}
						<div class="flex items-center justify-between px-6 py-4 border-b border-border">
							<h3 class="font-mono text-txt-primary text-sm tracking-wider uppercase">
								{activeSection() === "audio" && "// AUDIO_CONFIG"}
								{activeSection() === "hotkey" && "// HOTKEY_CONFIG"}
								{activeSection() === "appearance" && "// APPEARANCE_CONFIG"}
								{activeSection() === "language" && "// LANGUAGE_CONFIG"}
							</h3>
							<button
								type="button"
								onClick={props.onClose}
								class="font-mono text-txt-muted hover:text-ac transition-colors text-sm"
							>
								[X]
							</button>
						</div>

						{/* Content Body */}
						<div class="flex-1 overflow-auto p-6">
							{/* Audio Section */}
							<Show when={activeSection() === "audio"}>
								<div class="space-y-6">
									<div>
										<div class="flex items-center justify-between mb-3">
											<label class="font-mono text-txt-muted text-xs uppercase tracking-wider">
												INPUT_DEVICE
											</label>
											<button
												type="button"
												onClick={fetchAudioDevices}
												disabled={isLoadingDevices()}
												class="p-1.5 text-txt-muted hover:text-ac transition-colors disabled:opacity-50"
												title="Refresh devices"
											>
												<RefreshCw class={`w-4 h-4 ${isLoadingDevices() ? "animate-spin" : ""}`} />
											</button>
										</div>
										<Select
											value={settings().selectedMicrophoneId ?? "default"}
											options={microphoneOptions()}
											onChange={(value) => {
												capture("settings_microphone_changed");
												updateMicrophone(value === "default" ? null : value);
											}}
										/>
										<p class="mt-3 font-mono text-xs text-txt-faint">
											{t("settings.microphoneDescription")}
										</p>
									</div>

									{/* Audio Quality Preset */}
									<div>
										<label class="font-mono text-txt-muted text-xs uppercase tracking-wider block mb-4">
											AUDIO_QUALITY
										</label>
										<div class="grid grid-cols-3 gap-4">
											<QualityOption
												value="high"
												label={t("settings.audioQualityHigh")}
												description={t("settings.audioQualityHighDescription")}
												isSelected={settings().audioQuality === "high"}
												onClick={() => {
													capture("settings_audio_quality_changed", { quality: "high" });
													updateAudioQuality("high");
												}}
											/>
											<QualityOption
												value="medium"
												label={t("settings.audioQualityMedium")}
												description={t("settings.audioQualityMediumDescription")}
												isSelected={settings().audioQuality === "medium"}
												onClick={() => {
													capture("settings_audio_quality_changed", { quality: "medium" });
													updateAudioQuality("medium");
												}}
											/>
											<QualityOption
												value="low"
												label={t("settings.audioQualityLow")}
												description={t("settings.audioQualityLowDescription")}
												isSelected={settings().audioQuality === "low"}
												onClick={() => {
													capture("settings_audio_quality_changed", { quality: "low" });
													updateAudioQuality("low");
												}}
											/>
										</div>
										<p class="mt-3 font-mono text-xs text-txt-faint">
											{t("settings.audioQualityDescription")}
										</p>
									</div>
								</div>
							</Show>

							{/* Hotkey Section */}
							<Show when={activeSection() === "hotkey"}>
								<div class="space-y-6">
									<div>
										<label class="font-mono text-txt-muted text-xs uppercase tracking-wider block mb-3">
											RECORDING_TRIGGER
										</label>
										<div class="flex items-center gap-3">
											<div
												class={`flex-1 px-4 py-3 border font-mono text-center text-sm ${
													isRecordingHotkey()
														? "border-ac bg-ac-bg text-ac"
														: "border-border-strong bg-th-surface text-txt-primary"
												}`}
											>
												{isRecordingHotkey()
													? pendingHotkey() || "_ WAITING FOR INPUT _"
													: hotkeyDisplayName(settings().hotkey)}
											</div>
											<button
												type="button"
												onClick={() => toggleHotkeyRecording()}
												class={`px-4 py-3 font-mono text-xs uppercase tracking-wider transition-colors ${
													isRecordingHotkey()
														? "bg-border text-txt-secondary hover:bg-border-strong"
														: "bg-ac text-ac-on hover:bg-ac-hover"
												}`}
											>
												{isRecordingHotkey() ? "[CANCEL]" : "[CHANGE]"}
											</button>
										</div>
										<p class="mt-3 font-mono text-xs text-txt-faint">
											{t("settings.hotkeyDescription")}
										</p>
									</div>
								</div>
							</Show>

							{/* Appearance Section */}
							<Show when={activeSection() === "appearance"}>
								<div class="space-y-6">
									<div>
										<label class="font-mono text-txt-muted text-xs uppercase tracking-wider block mb-4">
											THEME_MODE
										</label>
										<div class="grid grid-cols-3 gap-4">
											<ThemeOption
												value="light"
												label="LIGHT"
												isSelected={settings().theme === "light"}
												onClick={() => {
													capture("settings_theme_changed", { theme: "light" });
													updateTheme("light");
												}}
											/>
											<ThemeOption
												value="dark"
												label="DARK"
												isSelected={settings().theme === "dark"}
												onClick={() => {
													capture("settings_theme_changed", { theme: "dark" });
													updateTheme("dark");
												}}
											/>
											<ThemeOption
												value="system"
												label="SYSTEM"
												isSelected={settings().theme === "system"}
												onClick={() => {
													capture("settings_theme_changed", { theme: "system" });
													updateTheme("system");
												}}
											/>
										</div>
									</div>
								</div>
							</Show>

							{/* Language Section */}
							<Show when={activeSection() === "language"}>
								<div class="space-y-6">
									<div>
										<label class="font-mono text-txt-muted text-xs uppercase tracking-wider block mb-3">
											INTERFACE_LANGUAGE
										</label>
										<Select
											value={locale()}
											options={languageOptions}
											onChange={(value) => {
												capture("settings_language_changed", { language: value });
												updateLanguage(value as Locale, setLocale);
											}}
										/>
									</div>
								</div>
							</Show>
						</div>

						{/* Footer */}
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

interface ThemeOptionProps {
	value: Theme;
	label: string;
	isSelected: boolean;
	onClick: () => void;
}

function ThemeOption(props: ThemeOptionProps) {
	return (
		<button
			type="button"
			onClick={props.onClick}
			class={`relative p-4 border transition-all ${
				props.isSelected
					? "border-ac bg-ac-bg"
					: "border-border-strong bg-th-surface hover:border-txt-faint"
			}`}
		>
			<div class="mb-3">
				<Show when={props.value === "light"}>
					<div class="w-full h-12 bg-[#e0e0e0] border border-[#ccc] flex items-center justify-center">
						<div class="w-8 h-1.5 bg-[#999]" />
					</div>
				</Show>
				<Show when={props.value === "dark"}>
					<div class="w-full h-12 bg-[#1a1a1a] border border-[#333] flex items-center justify-center">
						<div class="w-8 h-1.5 bg-[#444]" />
					</div>
				</Show>
				<Show when={props.value === "system"}>
					<div class="w-full h-12 overflow-hidden flex">
						<div class="w-1/2 bg-[#e0e0e0] border-l border-t border-b border-[#ccc] flex items-center justify-center">
							<div class="w-4 h-1.5 bg-[#999]" />
						</div>
						<div class="w-1/2 bg-[#1a1a1a] border-r border-t border-b border-[#333] flex items-center justify-center">
							<div class="w-4 h-1.5 bg-[#444]" />
						</div>
					</div>
				</Show>
			</div>
			<span
				class={`font-mono text-xs uppercase tracking-wider ${props.isSelected ? "text-ac" : "text-txt-secondary"}`}
			>
				{props.label}
			</span>
			<Show when={props.isSelected}>
				<div class="absolute top-2 right-2 font-mono text-ac text-xs">[*]</div>
			</Show>
		</button>
	);
}

interface QualityOptionProps {
	value: AudioQuality;
	label: string;
	description: string;
	isSelected: boolean;
	onClick: () => void;
}

function QualityOption(props: QualityOptionProps) {
	return (
		<button
			type="button"
			onClick={props.onClick}
			class={`relative p-4 border transition-all text-left ${
				props.isSelected
					? "border-ac bg-ac-bg"
					: "border-border-strong bg-th-surface hover:border-txt-faint"
			}`}
		>
			<div class="mb-2">
				<div
					class={`w-full h-8 border flex items-end justify-center gap-[2px] px-2 pb-1 ${
						props.isSelected ? "border-ac/30 bg-th-surface" : "border-border-strong bg-th-base"
					}`}
				>
					<Show when={props.value === "high"}>
						<div class={`w-[3px] h-2 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
						<div class={`w-[3px] h-4 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
						<div class={`w-[3px] h-6 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
						<div class={`w-[3px] h-3 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
						<div class={`w-[3px] h-5 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
					</Show>
					<Show when={props.value === "medium"}>
						<div class={`w-[3px] h-1.5 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
						<div class={`w-[3px] h-3 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
						<div class={`w-[3px] h-4 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
						<div class={`w-[3px] h-2 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
						<div class={`w-[3px] h-3 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
					</Show>
					<Show when={props.value === "low"}>
						<div class={`w-[3px] h-1 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
						<div class={`w-[3px] h-2 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
						<div class={`w-[3px] h-1.5 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
						<div class={`w-[3px] h-1 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
						<div class={`w-[3px] h-2 ${props.isSelected ? "bg-ac" : "bg-txt-faint"}`} />
					</Show>
				</div>
			</div>
			<span
				class={`font-mono text-xs uppercase tracking-wider block ${props.isSelected ? "text-ac" : "text-txt-secondary"}`}
			>
				{props.label}
			</span>
			<span class="font-mono text-[10px] text-txt-muted block mt-1">{props.description}</span>
			<Show when={props.isSelected}>
				<div class="absolute top-2 right-2 font-mono text-ac text-xs">[*]</div>
			</Show>
		</button>
	);
}
