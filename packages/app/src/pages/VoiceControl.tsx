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
import {
	isValidHotkey,
	registerDictationHotkeys,
	unregisterDictationHotkey,
} from "../lib/hotkeyUtils";
import { loadSettings, updateHoldToSpeakHotkey, updateHotkey } from "../lib/settingsStore";

const DEFAULT_HOTKEY = "LeftControl+LeftOption";
const DEFAULT_HOLD_TO_SPEAK_HOTKEY = "RightCommand";

const BAR_MULTIPLIERS = [0.5, 0.8, 0.4, 0.9, 0.6, 1.0, 0.7, 0.95, 0.5, 0.85, 0.6, 0.45];

const WINDOW_WIDTH = 100;
const WINDOW_HEIGHT = 20;
const BOTTOM_PADDING = 20;
const ESCAPE_KEY_CODE = 53;

type KeyboardKeyPressedPayload = {
	keyCode: number;
};

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
	const [muteMediaWhileRecording, setMuteMediaWhileRecording] = createSignal(false);
	const [audioLevel, setAudioLevel] = createSignal(0);
	const [isOnboardingComplete, setIsOnboardingComplete] = createSignal(false);
	const [isLearningActive, setIsLearningActive] = createSignal(false);
	let isStopping = false;
	let isStarting = false;
	let activeRecordingMode: "toggle" | "hold" | null = null;

	const canUseShortcut = () => isOnboardingComplete() || isLearningActive();

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

	const cancelInterruptedHoldToSpeakRecording = () => {
		if (!canUseShortcut()) return;
		if (isStarting || isStopping || !isRecording()) return;
		if (activeRecordingMode !== "hold") return;
		cancelRecording();
	};

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
		setMuteMediaWhileRecording(settings.muteMediaWhileRecording);
		setIsOnboardingComplete(settings.onboardingComplete);

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
			setMuteMediaWhileRecording(newSettings.muteMediaWhileRecording);
			setIsOnboardingComplete(newSettings.onboardingComplete);
		});

		const unlistenAudio = await listen<number>("audio-level", (event) => {
			setAudioLevel(event.payload);
		});

		const unlistenLearning = await listen<boolean>("learning-step-active", (event) => {
			setIsLearningActive(event.payload);
		});

		const unlistenKeyboardKeyPressed = await listen<KeyboardKeyPressedPayload>(
			"keyboard-key-pressed",
			(event) => {
				if (event.payload.keyCode === ESCAPE_KEY_CODE) return;
				if (activeRecordingMode !== "hold") return;
				cancelInterruptedHoldToSpeakRecording();
			}
		);

		onCleanup(() => {
			unlistenSettings();
			unlistenAudio();
			unlistenLearning();
			unlistenKeyboardKeyPressed();
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

			const prompt = await invoke<string | null>("get_dictionary_prompt");

			const result = await invoke<{
				text: string;
				word_count: number;
				processing_time_ms: number;
				audio_duration_ms: number | null;
			}>("transcribe_audio", { audioPath: filePath, prompt });

			await invoke("save_transcription", {
				text: result.text,
				wordCount: result.word_count,
				processingTimeMs: result.processing_time_ms,
				audioDurationMs: result.audio_duration_ms,
			});

			await invoke("type_text", { text: result.text ?? "" });

			setTimeout(() => {
				emit("transcription-created");
			}, 1000);
		} catch {
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
		} catch {}
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
		} finally {
			await restoreMediaAfterRecording();
			isStopping = false;
		}
	};

	const muteMediaForRecording = async () => {
		if (!muteMediaWhileRecording()) return;
		try {
			await invoke("mute_media_for_recording");
		} catch {}
	};

	const restoreMediaAfterRecording = async () => {
		try {
			await invoke("restore_media_after_recording");
		} catch {}
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
			void muteMediaForRecording();
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
