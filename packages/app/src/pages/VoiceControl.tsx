import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { Loader } from "lucide-solid";
import { createSignal, For, onMount, onCleanup, Show } from "solid-js";
import eden from "../lib/eden";
import { loadSettings } from "../lib/settingsStore";

export default function VoiceControl() {
	const [isRecording, setIsRecording] = createSignal(false);
	const [loading, setLoading] = createSignal(false);
	const [currentShortcut, setCurrentShortcut] = createSignal<string | null>(null);
	const [selectedMicrophone, setSelectedMicrophone] = createSignal<string | null>(null);
	let isStopping = false;
	let isStarting = false;

	const handleShortcut = (evt: { state: string }) => {
		if (evt.state !== "Pressed") return;
		console.log("shortcut pressed");

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
		// Load settings and register the initial shortcut
		const settings = await loadSettings();
		await registerShortcut(settings.hotkey);
		setSelectedMicrophone(settings.selectedMicrophoneId);

		// Listen for settings changes from the main window
		const unlisten = await listen("settings-changed", async () => {
			const newSettings = await loadSettings();
			if (newSettings.hotkey !== currentShortcut()) {
				await registerShortcut(newSettings.hotkey);
			}
			setSelectedMicrophone(newSettings.selectedMicrophoneId);
		});

		onCleanup(() => {
			unlisten();
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
		<div class="min-h-2 bg-black rounded-xl flex align-center justify-center">
			<Show when={loading()}>
				<Loader class="w-4 h-4 animate-spin text-white" />
			</Show>
			<Show when={!loading() && isRecording()}>
				<div class="flex items-center justify-center gap-[3px]">
					<For each={[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]}>
						{(i) => (
							<div
								class="w-[5px] rounded bg-primary-500 animate-voice-wave"
								style={{ "animation-delay": `${i * 0.1}s` }}
							/>
						)}
					</For>
				</div>
			</Show>
		</div>
	);
}
