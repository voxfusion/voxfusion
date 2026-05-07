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
import {
	isValidHotkey,
	registerDictationHotkeys,
	unregisterDictationHotkey,
} from "../lib/hotkeyUtils";
import { loadSettings, updateHoldToSpeakHotkey, updateHotkey } from "../lib/settingsStore";
import { tokenManager } from "../lib/tokenManager";

const DEFAULT_HOTKEY = "LeftControl+LeftOption";
const DEFAULT_HOLD_TO_SPEAK_HOTKEY = "RightCommand";

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
	const [muteMediaWhileRecording, setMuteMediaWhileRecording] = createSignal(false);
	const [audioLevel, setAudioLevel] = createSignal(0);
	const [isAuthenticated, setIsAuthenticated] = createSignal(false);
	const [isOnboardingComplete, setIsOnboardingComplete] = createSignal(false);
	const [isLearningActive, setIsLearningActive] = createSignal(false);
	let isStopping = false;
	let isStarting = false;
	let activeRecordingMode: "toggle" | "hold" | null = null;

	const canUseShortcut = () => isAuthenticated() && (isOnboardingComplete() || isLearningActive());

	const toggleRecording = () => {
		if (!canUseShortcut()) return;
		if (isStarting || isStopping) return;

		if (isRecording()) {
			if (activeRecordingMode !== "toggle") return;
			stopRecording();
		} else {
			startRecording("toggle");
		}
	};

	const startHoldToSpeakRecording = () => {
		if (!canUseShortcut()) return;
		if (isStarting || isStopping || isRecording()) return;
		startRecording("hold");
	};

	const stopHoldToSpeakRecording = () => {
		if (!canUseShortcut()) return;
		if (isStarting || isStopping || !isRecording()) return;
		if (activeRecordingMode !== "hold") return;
		stopRecording();
	};

	/**
	 * Register a dictation shortcut through the matching backend.
	 */
	const registerShortcuts = async (toggleShortcut: string, holdShortcut: string) => {
		const toggleHotkey = { hotkey: toggleShortcut, onPressed: toggleRecording };
		const holdHotkey = {
			hotkey: holdShortcut,
			onPressed: startHoldToSpeakRecording,
			onReleased: stopHoldToSpeakRecording,
		};
		const hotkeys = toggleShortcut === holdShortcut ? [toggleHotkey] : [toggleHotkey, holdHotkey];

		await registerDictationHotkeys(hotkeys);
		setCurrentShortcut(`${toggleShortcut}|${holdShortcut}`);
	};

	onMount(async () => {
		await repositionToCurrentMonitor();
		const repositionInterval = setInterval(repositionToCurrentMonitor, 1000);
		onCleanup(() => clearInterval(repositionInterval));

		const settings = await loadSettings();
		let hotkey = settings.hotkey;
		if (!isValidHotkey(hotkey)) {
			hotkey = DEFAULT_HOTKEY;
			await updateHotkey(hotkey);
		}
		let holdToSpeakHotkey = settings.holdToSpeakHotkey;
		if (!isValidHotkey(holdToSpeakHotkey)) {
			holdToSpeakHotkey = DEFAULT_HOLD_TO_SPEAK_HOTKEY;
			await updateHoldToSpeakHotkey(holdToSpeakHotkey);
		}
		await registerShortcuts(hotkey, holdToSpeakHotkey);
		setSelectedMicrophone(settings.selectedMicrophoneId);
		setAudioQuality(settings.audioQuality);
		setMuteMediaWhileRecording(settings.muteMediaWhileRecording);
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
			const newHotkey = isValidHotkey(newSettings.hotkey) ? newSettings.hotkey : DEFAULT_HOTKEY;
			const newHoldToSpeakHotkey = isValidHotkey(newSettings.holdToSpeakHotkey)
				? newSettings.holdToSpeakHotkey
				: DEFAULT_HOLD_TO_SPEAK_HOTKEY;
			const wasOnboarding = !isOnboardingComplete();
			const nowComplete = newSettings.onboardingComplete;
			const shortcutKey = `${newHotkey}|${newHoldToSpeakHotkey}`;
			if (shortcutKey !== currentShortcut() || (wasOnboarding && nowComplete)) {
				await registerShortcuts(newHotkey, newHoldToSpeakHotkey);
			}
			setSelectedMicrophone(newSettings.selectedMicrophoneId);
			setAudioQuality(newSettings.audioQuality);
			setMuteMediaWhileRecording(newSettings.muteMediaWhileRecording);
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
		await unregisterDictationHotkey();
		setCurrentShortcut(null);
	});

	const stopRecording = async () => {
		if (isStopping) return;
		if (!isRecording()) return;

		isStopping = true;
		setIsRecording(false);
		activeRecordingMode = null;
		await unregisterEscapeShortcut();

		try {
			const filePath = await invoke<string>("stop_recording_with_device");
			await restoreMediaAfterRecording();

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
			await restoreMediaAfterRecording();
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
		activeRecordingMode = null;
		await unregisterEscapeShortcut();

		try {
			await invoke<string>("stop_recording_with_device");
		} catch {
			// Cancel recording failed
		} finally {
			await restoreMediaAfterRecording();
			isStopping = false;
		}
	};

	const muteMediaForRecording = async () => {
		if (!muteMediaWhileRecording()) return;
		try {
			await invoke("mute_media_for_recording");
		} catch {
			// Media muting is best-effort and should not block recording.
		}
	};

	const restoreMediaAfterRecording = async () => {
		try {
			await invoke("restore_media_after_recording");
		} catch {
			// Media restore failed
		}
	};

	const startRecording = async (mode: "toggle" | "hold") => {
		try {
			if (isRecording()) return;
			if (isStopping || isStarting) return;
			isStarting = true;

			const deviceName = selectedMicrophone();
			await invoke("start_recording_with_device", {
				deviceName: deviceName === "default" ? null : deviceName,
			});
			await muteMediaForRecording();
			activeRecordingMode = mode;
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
