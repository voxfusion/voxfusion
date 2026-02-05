import { createSignal, createEffect, For, Show, onCleanup } from "solid-js";
import { ChevronDown, RefreshCw } from "lucide-solid";
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
				class="flex items-center justify-between w-full px-4 py-2.5 bg-[#111] border border-[#333] text-[#e0e0e0] hover:border-[#ff3e00] focus:outline-none focus:border-[#ff3e00] transition-colors font-mono text-sm"
			>
				<span class="truncate">{selectedLabel()}</span>
				<ChevronDown
					class={`w-4 h-4 ml-2 text-[#666] transition-transform ${isOpen() ? "rotate-180" : ""}`}
				/>
			</button>

			<Show when={isOpen()}>
				<div class="absolute z-50 w-full mt-1 bg-[#111] border border-[#333] max-h-60 overflow-auto">
					<For each={props.options}>
						{(option) => (
							<button
								type="button"
								onClick={() => {
									props.onChange(option.value);
									setIsOpen(false);
								}}
								class={`flex items-center justify-between w-full px-4 py-2.5 text-left hover:bg-[#1a1a1a] transition-colors font-mono text-sm ${
									option.value === props.value
										? "text-[#ff3e00] bg-[#1a1a1a]"
										: "text-[#e0e0e0]"
								}`}
							>
								<span class="truncate">{option.label}</span>
								<Show when={option.value === props.value}>
									<span class="text-[#ff3e00]">[*]</span>
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

	createEffect(() => {
		if (props.isOpen) {
			fetchAudioDevices();
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
	];

	return (
		<Show when={props.isOpen}>
			<div
				class="fixed inset-0 bg-black/70 flex items-center justify-center z-50"
				onClick={handleOverlayClick}
			>
				<div class="bg-[#0a0a0a] border border-[#222] w-[800px] h-[600px] flex overflow-hidden">
					{/* Sidebar */}
					<div class="w-56 bg-[#0a0a0a] border-r border-[#222] flex flex-col">
						{/* Header */}
						<div class="px-4 py-4 border-b border-[#222]">
							<h2 class="font-mono text-[#ff3e00] text-sm tracking-wider">
								[VOXFUSION] &gt; SETTINGS
							</h2>
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
												? "text-[#ff3e00] border-l-2 border-[#ff3e00] bg-[#111]"
												: "text-[#666] hover:text-[#888] hover:bg-[#111] border-l-2 border-transparent"
										}`}
									>
										<span class="text-[#444]">{item.num}</span>
										<span>{item.label}</span>
									</button>
								)}
							</For>
						</nav>
					</div>

					{/* Content */}
					<div class="flex-1 flex flex-col">
						{/* Content Header */}
						<div class="flex items-center justify-between px-6 py-4 border-b border-[#222]">
							<h3 class="font-mono text-[#e0e0e0] text-sm tracking-wider uppercase">
								{activeSection() === "audio" && "// AUDIO_CONFIG"}
								{activeSection() === "hotkey" && "// HOTKEY_CONFIG"}
								{activeSection() === "appearance" && "// APPEARANCE_CONFIG"}
								{activeSection() === "language" && "// LANGUAGE_CONFIG"}
							</h3>
							<button
								type="button"
								onClick={props.onClose}
								class="font-mono text-[#666] hover:text-[#ff3e00] transition-colors text-sm"
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
											<label class="font-mono text-[#666] text-xs uppercase tracking-wider">
												INPUT_DEVICE
											</label>
											<button
												type="button"
												onClick={fetchAudioDevices}
												disabled={isLoadingDevices()}
												class="p-1.5 text-[#666] hover:text-[#ff3e00] transition-colors disabled:opacity-50"
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
										<p class="mt-3 font-mono text-xs text-[#444]">
											{t("settings.microphoneDescription")}
										</p>
									</div>
								</div>
							</Show>

							{/* Hotkey Section */}
							<Show when={activeSection() === "hotkey"}>
								<div class="space-y-6">
									<div>
										<label class="font-mono text-[#666] text-xs uppercase tracking-wider block mb-3">
											RECORDING_TRIGGER
										</label>
										<div class="flex items-center gap-3">
											<div
												class={`flex-1 px-4 py-3 border font-mono text-center text-sm ${
													isRecordingHotkey()
														? "border-[#ff3e00] bg-[#1a0a00] text-[#ff3e00]"
														: "border-[#333] bg-[#111] text-[#e0e0e0]"
												}`}
											>
												{isRecordingHotkey()
													? pendingHotkey() || "_ WAITING FOR INPUT _"
													: settings().hotkey}
											</div>
											<button
												type="button"
												onClick={() => {
													setIsRecordingHotkey(!isRecordingHotkey());
													setPendingHotkey("");
												}}
												class={`px-4 py-3 font-mono text-xs uppercase tracking-wider transition-colors ${
													isRecordingHotkey()
														? "bg-[#222] text-[#888] hover:bg-[#333]"
														: "bg-[#ff3e00] text-[#0a0a0a] hover:bg-[#ff5500]"
												}`}
											>
												{isRecordingHotkey() ? "[CANCEL]" : "[CHANGE]"}
											</button>
										</div>
										<p class="mt-3 font-mono text-xs text-[#444]">
											{t("settings.hotkeyDescription")}
										</p>
									</div>
								</div>
							</Show>

							{/* Appearance Section */}
							<Show when={activeSection() === "appearance"}>
								<div class="space-y-6">
									<div>
										<label class="font-mono text-[#666] text-xs uppercase tracking-wider block mb-4">
											THEME_MODE
										</label>
										<div class="grid grid-cols-3 gap-4">
											<ThemeOption
												value="light"
												label="LIGHT"
												isSelected={settings().theme === "light"}
												onClick={() => updateTheme("light")}
											/>
											<ThemeOption
												value="dark"
												label="DARK"
												isSelected={settings().theme === "dark"}
												onClick={() => updateTheme("dark")}
											/>
											<ThemeOption
												value="system"
												label="SYSTEM"
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
										<label class="font-mono text-[#666] text-xs uppercase tracking-wider block mb-3">
											INTERFACE_LANGUAGE
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

						{/* Footer */}
						<div class="px-6 py-3 border-t border-[#222]">
							<p class="font-mono text-[10px] text-[#333]">
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
					? "border-[#ff3e00] bg-[#1a0a00]"
					: "border-[#333] bg-[#111] hover:border-[#444]"
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
				class={`font-mono text-xs uppercase tracking-wider ${props.isSelected ? "text-[#ff3e00]" : "text-[#888]"}`}
			>
				{props.label}
			</span>
			<Show when={props.isSelected}>
				<div class="absolute top-2 right-2 font-mono text-[#ff3e00] text-xs">
					[*]
				</div>
			</Show>
		</button>
	);
}
