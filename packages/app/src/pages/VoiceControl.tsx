import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import {
	LogicalPosition,
	cursorPosition,
	getCurrentWindow,
	monitorFromPoint,
} from "@tauri-apps/api/window";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { Loader } from "lucide-solid";
import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import eden from "../lib/eden";
import { loadSettings, updateHotkey } from "../lib/settingsStore";
import { tokenManager } from "../lib/tokenManager";

const DEFAULT_HOTKEY = "Command+;";

/**
 * Check if a hotkey string is a valid combo shortcut (modifier + non-modifier key).
 * Modifier-only shortcuts (e.g., "RightCommand", "Control+Shift") are no longer supported.
 */
function isValidComboHotkey(hotkey: string): boolean {
	if (!hotkey.includes("+")) return false;
	const parts = hotkey.split("+");
	const modifierNames = new Set(["Command", "Control", "Alt", "Shift", "CapsLock", "Fn"]);
	// At least one part must NOT be a modifier
	return parts.some((part) => !modifierNames.has(part));
}

const BAR_MULTIPLIERS = [0.5, 0.8, 0.4, 0.9, 0.6, 1.0, 0.7, 0.95, 0.5, 0.85, 0.6, 0.45];

const WINDOW_WIDTH = 100;
const WINDOW_HEIGHT = 20;
const BOTTOM_PADDING = 20;

let lastMonitorX = 0;
let lastMonitorY = 0;

async function repositionToCurrentMonitor() {
	const cursor = await cursorPosition();
	const monitor = await monitorFromPoint(cursor.x, cursor.y);
	if (!monitor) return;

	const pos = monitor.position.toLogical(monitor.scaleFactor);
	const size = monitor.size.toLogical(monitor.scaleFactor);

	if (pos.x === lastMonitorX && pos.y === lastMonitorY) return;
	lastMonitorX = pos.x;
	lastMonitorY = pos.y;

	const x = pos.x + (size.width - WINDOW_WIDTH) / 2;
	const y = pos.y + size.height - WINDOW_HEIGHT - BOTTOM_PADDING;

	await getCurrentWindow().setPosition(new LogicalPosition(x, y));
}

export default function VoiceControl() {
	const [isRecording, setIsRecording] = createSignal(false);
	const [loading, setLoading] = createSignal(false);
	const [currentShortcut, setCurrentShortcut] = createSignal<string | null>(null);
	const [selectedMicrophone, setSelectedMicrophone] = createSignal<string | null>(null);
	const [audioQuality, setAudioQuality] = createSignal<string>("high");
	const [audioLevel, setAudioLevel] = createSignal(0);
	const [isAuthenticated, setIsAuthenticated] = createSignal(false);
	const [isOnboardingComplete, setIsOnboardingComplete] = createSignal(false);
	const [isLearningActive, setIsLearningActive] = createSignal(false);
	let isStopping = false;
	let isStarting = false;

	const toggleRecording = () => {
		if (!isAuthenticated() || (!isOnboardingComplete() && !isLearningActive())) {
			return;
		}
		if (isStarting || isStopping) return;

		if (isRecording()) {
			stopRecording();
		} else {
			startRecording();
		}
	};

	const handleShortcut = (evt: { state: string }) => {
		if (evt.state !== "Pressed") return;
		toggleRecording();
	};

	/**
	 * Unregister the currently active shortcut.
	 */
	const unregisterCurrentShortcut = async () => {
		const current = currentShortcut();
		if (!current) return;

		try {
			await unregister(current);
		} catch {}

		setCurrentShortcut(null);
	};

	/**
	 * Register a global shortcut.
	 */
	const registerShortcut = async (shortcut: string) => {
		// Unregister any existing shortcut first
		await unregisterCurrentShortcut();

		try {
			await unregister(shortcut);
		} catch {}
		await register(shortcut, handleShortcut);
		setCurrentShortcut(shortcut);
	};

	onMount(async () => {
		await repositionToCurrentMonitor();
		const repositionInterval = setInterval(repositionToCurrentMonitor, 1000);
		onCleanup(() => clearInterval(repositionInterval));

		const settings = await loadSettings();
		// Migrate: if a modifier-only hotkey was stored from a previous version, reset to default
		let hotkey = settings.hotkey;
		if (!isValidComboHotkey(hotkey)) {
			hotkey = DEFAULT_HOTKEY;
			await updateHotkey(hotkey);
		}
		await registerShortcut(hotkey);
		setSelectedMicrophone(settings.selectedMicrophoneId);
		setAudioQuality(settings.audioQuality);
		setIsOnboardingComplete(settings.onboardingComplete);

		const unlistenAuth = await listen("auth-changed", async () => {
			await tokenManager.init();
			const token = await tokenManager.getToken();
			setIsAuthenticated(!!token);
		});

		// Request current auth state from the main window. This avoids
		// calling Stronghold.load() concurrently with the main window,
		// which replaces the Rust-side vault instance and causes token
		// reads to fail.
		await emit("auth-request");

		const unlistenSettings = await listen("settings-changed", async () => {
			const newSettings = await loadSettings();
			const newHotkey = isValidComboHotkey(newSettings.hotkey)
				? newSettings.hotkey
				: DEFAULT_HOTKEY;
			const wasOnboarding = !isOnboardingComplete();
			const nowComplete = newSettings.onboardingComplete;
			if (newHotkey !== currentShortcut() || (wasOnboarding && nowComplete)) {
				await registerShortcut(newHotkey);
			}
			setSelectedMicrophone(newSettings.selectedMicrophoneId);
			setAudioQuality(newSettings.audioQuality);
			setIsOnboardingComplete(newSettings.onboardingComplete);
		});

		const unlistenAudio = await listen<number>("audio-level", (event) => {
			setAudioLevel(event.payload);
		});

		const unlistenLearning = await listen<boolean>("learning-step-active", (event) => {
			setIsLearningActive(event.payload);
		});

		onCleanup(() => {
			unlistenSettings();
			unlistenAudio();
			unlistenAuth();
			unlistenLearning();
		});
	});

	onCleanup(async () => {
		if (isRecording()) {
			await cancelRecording();
		}
		await unregisterEscapeShortcut();
		await unregisterCurrentShortcut();
	});

	const stopRecording = async () => {
		if (isStopping) return;
		if (!isRecording()) return;

		isStopping = true;
		setIsRecording(false);
		await unregisterEscapeShortcut();

		try {
			const filePath = await invoke<string>("stop_recording_with_device");

			setLoading(true);

			const audioBytes = await invoke<number[]>("process_audio_file", {
				path: filePath,
				quality: audioQuality(),
			});
			const audioBlob = new Blob([new Uint8Array(audioBytes)], { type: "audio/wav" });
			const audioFile = new File([audioBlob], "recording.wav", { type: "audio/wav" });

			const res = await eden.api.transcribe.post({ file: audioFile });

			if (res.status === 403) {
				emit("transcription-created");
				return;
			}

			let body: { text?: string };
			if (res.data instanceof Response) {
				body = await res.data.json();
			} else {
				body = res.data as { text?: string };
			}
			await invoke("type_text", { text: body?.text ?? "" });

			setTimeout(() => {
				emit("transcription-created");
			}, 1000);
		} catch {
			// Transcription failed
		} finally {
			setLoading(false);
			isStopping = false;
		}
	};

	const registerEscapeShortcut = async () => {
		try {
			await register("Escape", (evt) => {
				if (evt.state !== "Pressed") return;
				cancelRecording();
			});
		} catch {
			// Escape shortcut registration failed
		}
	};

	const unregisterEscapeShortcut = async () => {
		try {
			await unregister("Escape");
		} catch {}
	};

	const cancelRecording = async () => {
		if (isStopping) return;
		if (!isRecording()) return;

		isStopping = true;
		setIsRecording(false);
		await unregisterEscapeShortcut();

		try {
			await invoke<string>("stop_recording_with_device");
		} catch {
			// Cancel recording failed
		} finally {
			isStopping = false;
		}
	};

	const startRecording = async () => {
		try {
			if (isRecording()) return;
			if (isStopping || isStarting) return;
			isStarting = true;

			const deviceName = selectedMicrophone();
			await invoke("start_recording_with_device", {
				deviceName: deviceName === "default" ? null : deviceName,
			});
			setIsRecording(true);
			isStarting = false;
			await registerEscapeShortcut();
		} catch {
			isStopping = false;
			isStarting = false;
		}
	};

	return (
		<div
			class={`min-h-2 mx-auto bg-th-base h-full rounded-xl flex align-center justify-center ${!isRecording() && !loading() ? "opacity-0" : ""}`}
		>
			<Show when={loading()}>
				<Loader class="w-4 h-4 animate-spin text-ac m-auto" />
			</Show>
			<Show when={isRecording() && !loading()}>
				<div class="flex items-center justify-center gap-[3px]">
					<For each={BAR_MULTIPLIERS}>
						{(multiplier) => (
							<div
								class="w-[2px] rounded bg-ac transition-all duration-75"
								style={{ height: `${Math.max(4, audioLevel() * multiplier * 3)}px` }}
							/>
						)}
					</For>
				</div>
			</Show>
		</div>
	);
}
