import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { Loader } from "lucide-solid";
import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import eden from "../lib/eden";
import { loadSettings } from "../lib/settingsStore";
import { tokenManager } from "../lib/tokenManager";

const NUM_BARS = 12;
const BAR_MULTIPLIERS = [0.5, 0.8, 0.4, 0.9, 0.6, 1.0, 0.7, 0.95, 0.5, 0.85, 0.6, 0.45];

export default function VoiceControl() {
	const [isRecording, setIsRecording] = createSignal(false);
	const [loading, setLoading] = createSignal(false);
	const [currentShortcut, setCurrentShortcut] = createSignal<string | null>(null);
	const [selectedMicrophone, setSelectedMicrophone] = createSignal<string | null>(null);
	const [audioLevel, setAudioLevel] = createSignal(0);
	const [isAuthenticated, setIsAuthenticated] = createSignal(false);
	const [isOnboardingComplete, setIsOnboardingComplete] = createSignal(false);
	let isStopping = false;
	let isStarting = false;

	const handleShortcut = (evt: { state: string }) => {
		if (evt.state !== "Pressed") return;
		console.log("shortcut pressed");

		if (!isAuthenticated() || !isOnboardingComplete()) {
			return;
		}

		if (isStarting || isStopping) return;
		if (isRecording()) {
			stopRecording();
		} else {
			startRecording();
		}
	};

	const registerShortcut = async (shortcut: string) => {
		const current = currentShortcut();
		if (current) {
			try {
				await unregister(current);
			} catch {}
		}

		try {
			await unregister(shortcut);
		} catch {}

		await register(shortcut, handleShortcut);
		setCurrentShortcut(shortcut);
	};

	onMount(async () => {
		const settings = await loadSettings();
		await registerShortcut(settings.hotkey);
		setSelectedMicrophone(settings.selectedMicrophoneId);
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
			if (newSettings.hotkey !== currentShortcut()) {
				await registerShortcut(newSettings.hotkey);
			}
			setSelectedMicrophone(newSettings.selectedMicrophoneId);
			setIsOnboardingComplete(newSettings.onboardingComplete);
		});

		const unlistenAudio = await listen<number>("audio-level", (event) => {
			setAudioLevel(event.payload);
		});

		onCleanup(() => {
			unlistenSettings();
			unlistenAudio();
			unlistenAuth();
		});
	});

	onCleanup(async () => {
		if (isRecording()) {
			await stopRecording();
		}
		const current = currentShortcut();
		if (current) {
			await unregister(current);
		}
	});

	const stopRecording = async () => {
		if (isStopping) return;
		if (!isRecording()) return;

		isStopping = true;
		setIsRecording(false);

		try {
			const filePath = await invoke<string>("stop_recording_with_device");

			setLoading(true);

			const audioBytes = await invoke<number[]>("read_audio_file", { path: filePath });
			const audioBlob = new Blob([new Uint8Array(audioBytes)], { type: "audio/wav" });
			const audioFile = new File([audioBlob], "recording.wav", { type: "audio/wav" });

			const res = await eden.api.transcribe.post({ file: audioFile });

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
		} catch (err) {
			console.error("Transcription failed:", err);
		} finally {
			setLoading(false);
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
		} catch (err) {
			console.error("Failed to start recording:", err);
			isStopping = false;
			isStarting = false;
		}
	};

	return (
		<div
			class={`min-h-2 mx-auto bg-black h-full rounded-xl flex align-center justify-center ${!isRecording() && !loading() ? "opacity-0" : ""}`}
		>
			<Show when={loading()}>
				<Loader class="w-4 h-4 animate-spin text-white m-auto" />
			</Show>
			<Show when={isRecording() && !loading()}>
				<div class="flex items-center justify-center gap-[3px]">
					<For each={BAR_MULTIPLIERS}>
						{(multiplier) => (
							<div
								class="w-[2px] rounded bg-gray-300 transition-all duration-75"
								style={{ height: `${Math.max(4, audioLevel() * multiplier * 3)}px` }}
							/>
						)}
					</For>
				</div>
			</Show>
		</div>
	);
}
