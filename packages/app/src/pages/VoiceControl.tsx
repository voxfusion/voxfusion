import { emit, listen } from "@tauri-apps/api/event";
import {
	LogicalPosition,
	LogicalSize,
	cursorPosition,
	getCurrentWindow,
	monitorFromPoint,
} from "@tauri-apps/api/window";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { Result } from "better-result";
import { Check, X } from "lucide-solid";
import { For, Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import DotMatrixSpinner from "../components/DotMatrixSpinner";
import { getFrontmostApp } from "../lib/commands/apps";
import { startRecordingWithDevice, stopRecordingWithDevice } from "../lib/commands/audio";
import {
	muteMediaForRecording as muteMediaForRecordingCommand,
	restoreMediaAfterRecording as restoreMediaAfterRecordingCommand,
} from "../lib/commands/media";
import { typeText } from "../lib/commands/text";
import { saveTranscription, transcribeAudio } from "../lib/commands/transcriptions";
import { errorFields, logDiagnostic } from "../lib/diagnostics";
import {
	DEFAULT_HOLD_TO_SPEAK_HOTKEY,
	DEFAULT_HOTKEY,
	isValidHotkey,
	registerDictationHotkeys,
	unregisterDictationHotkey,
} from "../lib/hotkeyUtils";
import {
	loadSettings,
	updateHoldToSpeakHotkey,
	updateHotkey,
	useSettings,
} from "../lib/settingsStore";

const NUM_BARS = 10;
const BAR_INDICES = Array.from({ length: NUM_BARS }, (_, i) => i);
const BAR_BASE_HEIGHT = 4;
const BAR_MAX_HEIGHT = 18;
const BAR_IDLE_AMPLITUDE = 2;
const BAR_VOICE_SCALE = 2.5;
const WAVE_STEP_MS = 110;
const WAVE_PATTERN = [
	0.25, 0.45, 0.7, 0.9, 1.0, 0.95, 0.75, 0.5, 0.3, 0.4, 0.65, 0.85, 1.0, 0.9, 0.65, 0.35,
];

const WINDOW_WIDTH_COMPACT = 100;
const WINDOW_WIDTH_HANDS_FREE = 140;
const WINDOW_HEIGHT = 28;
const BOTTOM_PADDING = 20;
const ESCAPE_KEY_CODE = 53;
const VOXFUSION_BUNDLE_ID = "io.voxfusion.app";

type KeyboardKeyPressedPayload = {
	keyCode: number;
};

let lastMonitorX: number | null = null;
let lastMonitorY: number | null = null;
let currentWindowWidth = WINDOW_WIDTH_COMPACT;

async function setWindowWidth(width: number) {
	if (currentWindowWidth === width) return;
	currentWindowWidth = width;
	await getCurrentWindow().setSize(new LogicalSize(width, WINDOW_HEIGHT));
	lastMonitorX = null;
	lastMonitorY = null;
	await repositionToCurrentMonitor();
}

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

	const x = pos.x + (size.width - currentWindowWidth) / 2;
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
	const [recordingMode, setRecordingMode] = createSignal<"toggle" | "hold" | null>(null);
	let isStopping = false;
	let isStarting = false;
	let activeAppBundleId: string | null = null;
	let activeDomain: string | null = null;

	const canUseShortcut = () => isOnboardingComplete() || isLearningActive();

	const toggleRecording = () => {
		if (!canUseShortcut()) return;
		if (isStarting || isStopping) return;

		if (isRecording()) {
			if (recordingMode() !== "toggle") return;
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
		if (recordingMode() !== "hold") return;
		stopRecording();
	};

	const cancelInterruptedHoldToSpeakRecording = () => {
		if (!canUseShortcut()) return;
		if (isStarting || isStopping || !isRecording()) return;
		if (recordingMode() !== "hold") return;
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

	const unregisterShortcuts = async () => {
		if (currentShortcut() === null) return;
		await unregisterDictationHotkey();
		setCurrentShortcut(null);
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
		setSelectedMicrophone(settings.selectedMicrophoneId);
		setMuteMediaWhileRecording(settings.muteMediaWhileRecording);
		setIsOnboardingComplete(settings.onboardingComplete);
		if (settings.onboardingComplete) {
			await registerShortcuts(hotkey, holdToSpeakHotkey);
		}

		const unlistenSettings = await listen("settings-changed", async () => {
			const newSettings = await loadSettings();
			const newHotkey = isValidHotkey(newSettings.hotkey) ? newSettings.hotkey : DEFAULT_HOTKEY;
			const newHoldToSpeakHotkey = isValidHotkey(newSettings.holdToSpeakHotkey)
				? newSettings.holdToSpeakHotkey
				: DEFAULT_HOLD_TO_SPEAK_HOTKEY;
			const wasOnboarding = !isOnboardingComplete();
			const nowComplete = newSettings.onboardingComplete;
			const shouldRegisterShortcuts = nowComplete || isLearningActive();
			const shortcutKey = `${newHotkey}|${newHoldToSpeakHotkey}`;
			if (
				shouldRegisterShortcuts &&
				(shortcutKey !== currentShortcut() || (wasOnboarding && nowComplete))
			) {
				await registerShortcuts(newHotkey, newHoldToSpeakHotkey);
			} else if (!shouldRegisterShortcuts) {
				await unregisterShortcuts();
			}
			setSelectedMicrophone(newSettings.selectedMicrophoneId);
			setMuteMediaWhileRecording(newSettings.muteMediaWhileRecording);
			setIsOnboardingComplete(newSettings.onboardingComplete);
		});

		const unlistenAudio = await listen<number>("audio-level", (event) => {
			setAudioLevel(event.payload);
		});

		const unlistenLearning = await listen<boolean>("learning-step-active", async (event) => {
			const active = event.payload;
			setIsLearningActive(active);
			const newSettings = await loadSettings();
			const newHotkey = isValidHotkey(newSettings.hotkey) ? newSettings.hotkey : DEFAULT_HOTKEY;
			const newHoldToSpeakHotkey = isValidHotkey(newSettings.holdToSpeakHotkey)
				? newSettings.holdToSpeakHotkey
				: DEFAULT_HOLD_TO_SPEAK_HOTKEY;
			if (active || isOnboardingComplete()) {
				const shortcutKey = `${newHotkey}|${newHoldToSpeakHotkey}`;
				if (shortcutKey !== currentShortcut()) {
					await registerShortcuts(newHotkey, newHoldToSpeakHotkey);
				}
			} else {
				await unregisterShortcuts();
			}
		});

		const unlistenKeyboardKeyPressed = await listen<KeyboardKeyPressedPayload>(
			"keyboard-key-pressed",
			(event) => {
				if (event.payload.keyCode === ESCAPE_KEY_CODE) return;
				if (recordingMode() !== "hold") return;
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

		logDiagnostic("info", "voice", "stop_recording_started", {
			mode: recordingMode(),
		});
		isStopping = true;
		setIsRecording(false);
		setRecordingMode(null);
		await unregisterEscapeShortcut();
		await setWindowWidth(WINDOW_WIDTH_COMPACT);

		const completed = await Result.tryPromise(async () => {
			const filePath = await stopRecordingWithDevice();
			if (Result.isError(filePath)) {
				logDiagnostic("error", "voice", "stop_recording_failed", errorFields(filePath.error));
				return;
			}
			await restoreMediaAfterRecording();

			setLoading(true);

			const bundleId = activeAppBundleId;
			const domain = activeDomain;
			activeAppBundleId = null;
			activeDomain = null;
			const result = await transcribeAudio(
				filePath.value,
				bundleId,
				domain,
				settings().defaultStyle
			);
			if (Result.isError(result)) {
				logDiagnostic("error", "voice", "transcription_failed", errorFields(result.error));
				return;
			}
			await saveTranscription(result.value);
			logDiagnostic("info", "voice", "transcription_completed", {
				wordCount: result.value.word_count,
				processingTimeMs: result.value.processing_time_ms,
				audioDurationMs: result.value.audio_duration_ms,
				hasAppContext: Boolean(bundleId),
				hasSiteContext: Boolean(domain),
				style: settings().defaultStyle,
			});

			await typeText(result.value.text ?? "");
			logDiagnostic("info", "voice", "text_typed", {
				characterCount: result.value.text?.length ?? 0,
			});

			setTimeout(() => {
				emit("transcription-created");
			}, 1000);
		});
		if (Result.isError(completed)) {
			logDiagnostic("error", "voice", "stop_recording_unhandled_failure", {
				error: errorFields(completed.error),
			});
			console.error("Failed to stop recording:", completed.error);
		}
		await restoreMediaAfterRecording();
		setLoading(false);
		isStopping = false;
		await hideVoiceControlWindow();
		logDiagnostic("info", "voice", "stop_recording_completed");
	};

	const registerEscapeShortcut = async () => {
		await Result.tryPromise(async () => {
			await register("Escape", (evt) => {
				if (evt.state !== "Pressed") return;
				cancelRecording();
			});
		});
	};

	const unregisterEscapeShortcut = async () => {
		await Result.tryPromise(() => unregister("Escape"));
	};

	const cancelRecording = async () => {
		if (isStopping) return;
		if (!isRecording()) return;

		logDiagnostic("warn", "voice", "recording_cancelled", {
			mode: recordingMode(),
		});
		isStopping = true;
		setIsRecording(false);
		setRecordingMode(null);
		activeAppBundleId = null;
		activeDomain = null;
		await unregisterEscapeShortcut();
		await setWindowWidth(WINDOW_WIDTH_COMPACT);

		await stopRecordingWithDevice();
		await restoreMediaAfterRecording();
		isStopping = false;
		await hideVoiceControlWindow();
	};

	const muteMediaForRecording = async () => {
		if (!muteMediaWhileRecording()) return;
		await muteMediaForRecordingCommand();
	};

	const restoreMediaAfterRecording = async () => {
		await restoreMediaAfterRecordingCommand();
	};

	const startRecording = async (mode: "toggle" | "hold") => {
		const startedRecording = await Result.tryPromise(async () => {
			if (isRecording()) return;
			if (isStopping || isStarting) return;
			isStarting = true;
			logDiagnostic("info", "voice", "start_recording_started", { mode });
			activeAppBundleId = null;
			activeDomain = null;
			const frontmost = await getFrontmostApp();
			if (Result.isOk(frontmost)) {
				if (frontmost.value?.bundle_id && frontmost.value.bundle_id !== VOXFUSION_BUNDLE_ID) {
					activeAppBundleId = frontmost.value.bundle_id;
					activeDomain = frontmost.value.domain ?? null;
				}
			}
			await setWindowWidth(mode === "toggle" ? WINDOW_WIDTH_HANDS_FREE : WINDOW_WIDTH_COMPACT);
			await showVoiceControlWindow();

			const deviceName = selectedMicrophone();
			const started = await startRecordingWithDevice(deviceName === "default" ? null : deviceName);
			if (Result.isError(started)) {
				logDiagnostic("error", "voice", "start_recording_failed", {
					error: errorFields(started.error),
					hasSelectedMicrophone: Boolean(deviceName && deviceName !== "default"),
				});
				await hideVoiceControlWindow();
				isStarting = false;
				return;
			}
			void muteMediaForRecording();
			setRecordingMode(mode);
			setIsRecording(true);
			isStarting = false;
			await registerEscapeShortcut();
			logDiagnostic("info", "voice", "start_recording_completed", {
				mode,
				hasAppContext: Boolean(activeAppBundleId),
				hasSiteContext: Boolean(activeDomain),
				hasSelectedMicrophone: Boolean(deviceName && deviceName !== "default"),
			});
		});
		if (Result.isError(startedRecording)) {
			logDiagnostic("error", "voice", "start_recording_unhandled_failure", {
				error: errorFields(startedRecording.error),
			});
			await hideVoiceControlWindow();
			isStopping = false;
			isStarting = false;
		}
	};

	return (
		<div
			class={`min-h-2 mx-auto w-fit bg-th-base h-full rounded-2xl border border-border-strong flex align-center justify-center py-1 ${recordingMode() === "toggle" ? "px-2" : "px-4"} ${!isRecording() && !loading() ? "opacity-0" : ""}`}
		>
			<Show when={isRecording() && recordingMode() === "toggle" && !loading()}>
				<button
					type="button"
					aria-label="Cancel transcription"
					onClick={() => cancelRecording()}
					class="w-4 h-4 mr-1 self-center flex items-center justify-center rounded-full bg-th-elevated text-txt-secondary hover:text-txt-primary hover:bg-th-hover transition-colors cursor-pointer"
				>
					<X size={10} />
				</button>
			</Show>
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
			<Show when={isRecording() && recordingMode() === "toggle" && !loading()}>
				<button
					type="button"
					aria-label="End transcription"
					onClick={() => stopRecording()}
					class="w-4 h-4 ml-1 self-center flex items-center justify-center rounded-full bg-ac text-ac-on hover:bg-ac-hover transition-colors cursor-pointer"
				>
					<Check size={10} />
				</button>
			</Show>
			<div
				class="overflow-hidden transition-[max-width] duration-300 ease-out flex items-center"
				style={{ "max-width": loading() ? "24px" : "0px" }}
			>
				<DotMatrixSpinner class="text-ac ml-2" size={16} dotSize={3} />
			</div>
		</div>
	);
}
