import { emit, listen } from "@tauri-apps/api/event";
import {
	LogicalPosition,
	cursorPosition,
	getCurrentWindow,
	monitorFromPoint,
} from "@tauri-apps/api/window";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { For, Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import DotMatrixSpinner from "../components/DotMatrixSpinner";
import {
	isValidHotkey,
	registerDictationHotkeys,
	unregisterDictationHotkey,
} from "../lib/hotkeyUtils";
import { getFrontmostApp } from "../lib/commands/apps";
import { startRecordingWithDevice, stopRecordingWithDevice } from "../lib/commands/audio";
import {
	muteMediaForRecording as muteMediaForRecordingCommand,
	restoreMediaAfterRecording as restoreMediaAfterRecordingCommand,
} from "../lib/commands/media";
import { typeText } from "../lib/commands/text";
import { saveTranscription, transcribeAudio } from "../lib/commands/transcriptions";
import {
	loadSettings,
	updateHoldToSpeakHotkey,
	updateHotkey,
	useSettings,
} from "../lib/settingsStore";

const DEFAULT_HOTKEY = "LeftControl+LeftOption";
const DEFAULT_HOLD_TO_SPEAK_HOTKEY = "RightCommand";

const NUM_BARS = 10;
const BAR_INDICES = Array.from({ length: NUM_BARS }, (_, i) => i);
const BAR_BASE_HEIGHT = 4;
const BAR_MAX_HEIGHT = 18;
const BAR_IDLE_AMPLITUDE = 2;
const BAR_VOICE_SCALE = 2.5;
const WAVE_STEP_MS = 110;
const WAVE_PATTERN = [
	0.25, 0.45, 0.7, 0.9, 1.0, 0.95, 0.75, 0.5,
	0.3, 0.4, 0.65, 0.85, 1.0, 0.9, 0.65, 0.35,
];

const WINDOW_WIDTH = 100;
const WINDOW_HEIGHT = 28;
const BOTTOM_PADDING = 20;
const ESCAPE_KEY_CODE = 53;
const VOXFUSION_BUNDLE_ID = "io.voxfusion.app";

type KeyboardKeyPressedPayload = {
	keyCode: number;
};

let lastMonitorX = 0;
let lastMonitorY = 0;

async function showVoiceControlWindow() {
	await repositionToCurrentMonitor();
	await getCurrentWindow().show();
}

async function hideVoiceControlWindow() {
	await getCurrentWindow().hide();
}

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
	const settings = useSettings();
	const [isRecording, setIsRecording] = createSignal(false);
	const [loading, setLoading] = createSignal(false);
	const [currentShortcut, setCurrentShortcut] = createSignal<string | null>(null);
	const [selectedMicrophone, setSelectedMicrophone] = createSignal<string | null>(null);
	const [muteMediaWhileRecording, setMuteMediaWhileRecording] = createSignal(false);
	const [audioLevel, setAudioLevel] = createSignal(0);
	const [waveOffset, setWaveOffset] = createSignal(0);
	const [isOnboardingComplete, setIsOnboardingComplete] = createSignal(false);
	const [isLearningActive, setIsLearningActive] = createSignal(false);

	createEffect(() => {
		if (!isRecording()) return;
		const intervalId = setInterval(() => {
			setWaveOffset((o) => (o + 1) % WAVE_PATTERN.length);
		}, WAVE_STEP_MS);
		onCleanup(() => clearInterval(intervalId));
	});

	const barHeight = (index: number) => {
		if (loading()) return BAR_BASE_HEIGHT;
		const factor = WAVE_PATTERN[(waveOffset() + index) % WAVE_PATTERN.length]!;
		const amplitude = BAR_IDLE_AMPLITUDE + audioLevel() * BAR_VOICE_SCALE;
		return Math.min(BAR_MAX_HEIGHT, BAR_BASE_HEIGHT + factor * amplitude);
	};
	let isStopping = false;
	let isStarting = false;
	let activeRecordingMode: "toggle" | "hold" | null = null;
	let activeAppBundleId: string | null = null;

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
			const filePath = await stopRecordingWithDevice();
			await restoreMediaAfterRecording();

			setLoading(true);

			const bundleId = activeAppBundleId;
			activeAppBundleId = null;
			const result = await transcribeAudio(filePath, bundleId, settings().defaultStyle);
			await saveTranscription(result);

			await typeText(result.text ?? "");

			setTimeout(() => {
				emit("transcription-created");
			}, 1000);
		} catch {
		} finally {
			await restoreMediaAfterRecording();
			setLoading(false);
			isStopping = false;
			await hideVoiceControlWindow();
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
		activeAppBundleId = null;
		await unregisterEscapeShortcut();

		try {
			await stopRecordingWithDevice();
		} catch {
		} finally {
			await restoreMediaAfterRecording();
			isStopping = false;
			await hideVoiceControlWindow();
		}
	};

	const muteMediaForRecording = async () => {
		if (!muteMediaWhileRecording()) return;
		try {
			await muteMediaForRecordingCommand();
		} catch {}
	};

	const restoreMediaAfterRecording = async () => {
		try {
			await restoreMediaAfterRecordingCommand();
		} catch {}
	};

	const startRecording = async (mode: "toggle" | "hold") => {
		try {
			if (isRecording()) return;
			if (isStopping || isStarting) return;
			isStarting = true;
			activeAppBundleId = null;
			try {
				const frontmost = await getFrontmostApp();
				if (frontmost?.bundle_id && frontmost.bundle_id !== VOXFUSION_BUNDLE_ID) {
					activeAppBundleId = frontmost.bundle_id;
				}
			} catch {}
			await showVoiceControlWindow();

			const deviceName = selectedMicrophone();
			await startRecordingWithDevice(deviceName === "default" ? null : deviceName);
			void muteMediaForRecording();
			activeRecordingMode = mode;
			setIsRecording(true);
			isStarting = false;
			await registerEscapeShortcut();
		} catch {
			await hideVoiceControlWindow();
			isStopping = false;
			isStarting = false;
		}
	};

	return (
		<div
			class={`min-h-2 mx-auto w-fit bg-th-base h-full rounded-2xl border border-border-strong flex align-center justify-center px-4 py-1 ${!isRecording() && !loading() ? "opacity-0" : ""}`}
		>
			<Show when={isRecording() || loading()}>
				<div class="flex items-center justify-center gap-[2px]">
					<For each={BAR_INDICES}>
						{(index) => (
							<div
								class="w-[2px] rounded-full bg-ac transition-[height] duration-200 ease-out"
								style={{ height: `${barHeight(index)}px` }}
							/>
						)}
					</For>
				</div>
			</Show>
			<div
				class="overflow-hidden transition-[max-width] duration-300 ease-out flex items-center"
				style={{ "max-width": loading() ? "26px" : "0px" }}
			>
				<DotMatrixSpinner class="text-ac ml-2" size={18} dotSize={3} />
			</div>
		</div>
	);
}
