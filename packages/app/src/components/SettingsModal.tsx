import { createSignal, createEffect, For, Show, onCleanup } from "solid-js";
import { X, Mic, Keyboard, Palette, ChevronDown, Check, Globe, RefreshCw } from "lucide-solid";
import { useI18n, type Locale } from "../i18n";
import {
	useSettings,
	updateTheme,
	updateHotkey,
	updateMicrophone,
	updateLanguage,
	getAudioInputDevices,
	type Theme,
	type AudioDevice,
} from "../lib/settingsStore";

interface SettingsModalProps {
	isOpen: boolean;
	onClose: () => void;
}

type SettingsSection = "audio" | "hotkey" | "appearance" | "language";

// Custom Select Component with ShadCN-like styling
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
				class="flex items-center justify-between w-full px-4 py-2.5 bg-white dark:bg-midnight-800 border border-slate-200 dark:border-midnight-600 rounded-lg text-slate-900 dark:text-slate-100 hover:bg-slate-50 dark:hover:bg-midnight-700 focus:outline-none focus:ring-2 focus:ring-primary-500 focus:border-primary-500 transition-colors"
			>
				<span class="truncate">{selectedLabel()}</span>
				<ChevronDown
					class={`w-4 h-4 ml-2 text-slate-500 dark:text-slate-400 transition-transform ${isOpen() ? "rotate-180" : ""}`}
				/>
			</button>

			<Show when={isOpen()}>
				<div class="absolute z-50 w-full mt-1 bg-white dark:bg-midnight-800 border border-slate-200 dark:border-midnight-600 rounded-lg shadow-lg max-h-60 overflow-auto">
					<For each={props.options}>
						{(option) => (
							<button
								type="button"
								onClick={() => {
									props.onChange(option.value);
									setIsOpen(false);
								}}
								class={`flex items-center justify-between w-full px-4 py-2.5 text-left hover:bg-slate-100 dark:hover:bg-midnight-700 transition-colors ${
									option.value === props.value
										? "bg-primary-50 dark:bg-primary-900/20 text-primary-700 dark:text-primary-400"
										: "text-slate-900 dark:text-slate-100"
								}`}
							>
								<span class="truncate">{option.label}</span>
								<Show when={option.value === props.value}>
									<Check class="w-4 h-4 text-primary-600 dark:text-primary-400" />
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
	const [isRecordingHotkey, setIsRecordingHotkey] = createSignal(false);
	const [pendingHotkey, setPendingHotkey] = createSignal<string>("");
	const [isLoadingDevices, setIsLoadingDevices] = createSignal(false);

	const fetchAudioDevices = async () => {
		setIsLoadingDevices(true);
		try {
			const devices = await getAudioInputDevices();
			setAudioDevices(devices);
		} catch (error) {
			console.error("Failed to fetch audio devices:", error);
		} finally {
			setIsLoadingDevices(false);
		}
	};

	// Fetch devices when modal opens
	createEffect(() => {
		if (props.isOpen) {
			fetchAudioDevices();
		}
	});

	// Close modal on Escape key
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

	const handleKeyDown = (e: KeyboardEvent) => {
		if (!isRecordingHotkey()) return;

		e.preventDefault();
		e.stopPropagation();

		const modifiers: string[] = [];
		if (e.metaKey) modifiers.push("Command");
		if (e.ctrlKey) modifiers.push("Control");
		if (e.altKey) modifiers.push("Alt");
		if (e.shiftKey) modifiers.push("Shift");

		const key = e.key;
		// Ignore pure modifier keys
		if (["Meta", "Control", "Alt", "Shift"].includes(key)) {
			return;
		}

		const hotkeyString = [...modifiers, key.length === 1 ? key.toUpperCase() : key].join("+");
		setPendingHotkey(hotkeyString);
	};

	const handleKeyUp = async (_e: KeyboardEvent) => {
		if (!isRecordingHotkey()) return;

		const pending = pendingHotkey();
		if (pending && pending.includes("+")) {
			await updateHotkey(pending);
			setIsRecordingHotkey(false);
			setPendingHotkey("");
		}
	};

	createEffect(() => {
		if (isRecordingHotkey()) {
			window.addEventListener("keydown", handleKeyDown);
			window.addEventListener("keyup", handleKeyUp);
		} else {
			window.removeEventListener("keydown", handleKeyDown);
			window.removeEventListener("keyup", handleKeyUp);
		}
	});

	onCleanup(() => {
		window.removeEventListener("keydown", handleKeyDown);
		window.removeEventListener("keyup", handleKeyUp);
	});

	const sidebarItems = [
		{ id: "audio" as const, icon: Mic, label: t("settings.audio") },
		{ id: "hotkey" as const, icon: Keyboard, label: t("settings.hotkey") },
		{ id: "appearance" as const, icon: Palette, label: t("settings.appearance") },
		{ id: "language" as const, icon: Globe, label: t("settings.language") },
	];

	const handleOverlayClick = (e: MouseEvent) => {
		if (e.target === e.currentTarget) {
			props.onClose();
		}
	};

	const microphoneOptions = (): SelectOption[] => {
		const devices = audioDevices();
		// Find the default device to show it first with a special label
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
		{ value: "en", label: "English" },
		{ value: "ru", label: "Русский" },
	];

	return (
		<Show when={props.isOpen}>
			<div
				class="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
				onClick={handleOverlayClick}
			>
				<div class="bg-white dark:bg-midnight-900 rounded-xl shadow-2xl w-[800px] h-[600px] flex overflow-hidden border border-slate-200 dark:border-midnight-700">
					{/* Sidebar */}
					<div class="w-56 bg-slate-50 dark:bg-midnight-800 border-r border-slate-200 dark:border-midnight-700 p-4">
						<div class="flex items-center justify-between mb-6 px-2">
							<h2 class="text-lg font-semibold text-slate-900 dark:text-white">
								{t("settings.title")}
							</h2>
						</div>
						<nav class="space-y-1">
							<For each={sidebarItems}>
								{(item) => (
									<button
										type="button"
										onClick={() => setActiveSection(item.id)}
										class={`flex items-center gap-3 w-full px-3 py-2.5 rounded-lg text-sm font-medium transition-colors ${
											activeSection() === item.id
												? "bg-primary-100 dark:bg-primary-900/30 text-primary-700 dark:text-primary-400"
												: "text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-midnight-700 hover:text-slate-900 dark:hover:text-white"
										}`}
									>
										<item.icon class="w-5 h-5" />
										<span class="truncate">{item.label}</span>
									</button>
								)}
							</For>
						</nav>
					</div>

					{/* Main content area */}
					<div class="flex-1 flex flex-col">
						{/* Header */}
						<div class="flex items-center justify-between px-6 py-4 border-b border-slate-200 dark:border-midnight-700">
							<h3 class="text-lg font-semibold text-slate-900 dark:text-white">
								{activeSection() === "audio" && t("settings.audio")}
								{activeSection() === "hotkey" && t("settings.hotkey")}
								{activeSection() === "appearance" && t("settings.appearance")}
								{activeSection() === "language" && t("settings.language")}
							</h3>
							<button
								type="button"
								onClick={props.onClose}
								class="p-2 rounded-lg text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 hover:bg-slate-100 dark:hover:bg-midnight-700 transition-colors"
							>
								<X class="w-5 h-5" />
							</button>
						</div>

						{/* Content */}
						<div class="flex-1 overflow-auto p-6">
							{/* Audio Section */}
							<Show when={activeSection() === "audio"}>
								<div class="space-y-6">
									<div>
										<div class="flex items-center justify-between mb-2">
											<label class="block text-sm font-medium text-slate-700 dark:text-slate-300">
												{t("settings.microphone")}
											</label>
											<button
												type="button"
												onClick={fetchAudioDevices}
												disabled={isLoadingDevices()}
												class="p-1.5 text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-midnight-700 rounded-lg transition-colors disabled:opacity-50"
												title="Refresh devices"
											>
												<RefreshCw class={`w-4 h-4 ${isLoadingDevices() ? "animate-spin" : ""}`} />
											</button>
										</div>
										<Select
											value={settings().selectedMicrophoneId ?? "default"}
											options={microphoneOptions()}
											onChange={(value) => {
												updateMicrophone(value === "default" ? null : value);
											}}
										/>
										<p class="mt-2 text-sm text-slate-500 dark:text-slate-400">
											{t("settings.microphoneDescription")}
										</p>
									</div>
								</div>
							</Show>

							{/* Hotkey Section */}
							<Show when={activeSection() === "hotkey"}>
								<div class="space-y-6">
									<div>
										<label class="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
											{t("settings.recordingHotkey")}
										</label>
										<div class="flex items-center gap-3">
											<div
												class={`flex-1 px-4 py-3 border rounded-lg font-mono text-center ${
													isRecordingHotkey()
														? "border-primary-500 bg-primary-50 dark:bg-primary-900/20 text-primary-700 dark:text-primary-400"
														: "border-slate-300 dark:border-midnight-600 bg-slate-50 dark:bg-midnight-800 text-slate-700 dark:text-slate-300"
												}`}
											>
												{isRecordingHotkey()
													? pendingHotkey() || t("settings.pressHotkey")
													: settings().hotkey}
											</div>
											<button
												type="button"
												onClick={() => {
													setIsRecordingHotkey(!isRecordingHotkey());
													setPendingHotkey("");
												}}
												class={`px-4 py-2.5 rounded-lg font-medium transition-colors ${
													isRecordingHotkey()
														? "bg-slate-200 dark:bg-midnight-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-midnight-600"
														: "bg-primary-500 text-white hover:bg-primary-600"
												}`}
											>
												{isRecordingHotkey() ? t("settings.cancel") : t("settings.change")}
											</button>
										</div>
										<p class="mt-2 text-sm text-slate-500 dark:text-slate-400">
											{t("settings.hotkeyDescription")}
										</p>
									</div>
								</div>
							</Show>

							{/* Appearance Section */}
							<Show when={activeSection() === "appearance"}>
								<div class="space-y-6">
									<div>
										<label class="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-3">
											{t("settings.theme")}
										</label>
										<div class="grid grid-cols-3 gap-4">
											<ThemeOption
												value="light"
												label={t("settings.light")}
												isSelected={settings().theme === "light"}
												onClick={() => updateTheme("light")}
											/>
											<ThemeOption
												value="dark"
												label={t("settings.dark")}
												isSelected={settings().theme === "dark"}
												onClick={() => updateTheme("dark")}
											/>
											<ThemeOption
												value="system"
												label={t("settings.system")}
												isSelected={settings().theme === "system"}
												onClick={() => updateTheme("system")}
											/>
										</div>
									</div>
								</div>
							</Show>

							{/* Language Section */}
							<Show when={activeSection() === "language"}>
								<div class="space-y-6">
									<div>
										<label class="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
											{t("settings.language")}
										</label>
										<Select
											value={locale()}
											options={languageOptions}
											onChange={(value) => {
												updateLanguage(value as Locale, setLocale);
											}}
										/>
									</div>
								</div>
							</Show>
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
			class={`relative p-4 rounded-xl border-2 transition-all ${
				props.isSelected
					? "border-primary-500 bg-primary-50 dark:bg-primary-900/20"
					: "border-slate-200 dark:border-midnight-600 hover:border-slate-300 dark:hover:border-midnight-500 bg-white dark:bg-midnight-800"
			}`}
		>
			<div class="mb-3">
				<Show when={props.value === "light"}>
					<div class="w-full h-16 rounded-lg bg-white border border-slate-200 flex items-center justify-center">
						<div class="w-8 h-2 bg-slate-200 rounded" />
					</div>
				</Show>
				<Show when={props.value === "dark"}>
					<div class="w-full h-16 rounded-lg bg-slate-800 border border-slate-700 flex items-center justify-center">
						<div class="w-8 h-2 bg-slate-600 rounded" />
					</div>
				</Show>
				<Show when={props.value === "system"}>
					<div class="w-full h-16 rounded-lg overflow-hidden flex">
						<div class="w-1/2 bg-white border-l border-t border-b border-slate-200 flex items-center justify-center">
							<div class="w-4 h-2 bg-slate-200 rounded" />
						</div>
						<div class="w-1/2 bg-slate-800 border-r border-t border-b border-slate-700 flex items-center justify-center">
							<div class="w-4 h-2 bg-slate-600 rounded" />
						</div>
					</div>
				</Show>
			</div>
			<span
				class={`text-sm font-medium ${props.isSelected ? "text-primary-700 dark:text-primary-400" : "text-slate-700 dark:text-slate-300"}`}
			>
				{props.label}
			</span>
			<Show when={props.isSelected}>
				<div class="absolute top-2 right-2 w-5 h-5 bg-primary-500 rounded-full flex items-center justify-center">
					<svg class="w-3 h-3 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
					</svg>
				</div>
			</Show>
		</button>
	);
}
