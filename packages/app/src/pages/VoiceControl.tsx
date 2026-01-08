import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { Loader } from "lucide-solid";
import { createSignal, For, onMount, onCleanup, Show } from "solid-js";
import { startRecording as nativeStartRecording, stopRecording as nativeStopRecording } from "tauri-plugin-mic-recorder-api";
import eden from "../lib/eden";

export default function VoiceControl() {
	const [isRecording, setIsRecording] = createSignal(false);
	const [loading, setLoading] = createSignal(false);

	const shortcut = "Command+;";
	let isStopping = false;
	let isStarting = false;

	onMount(async () => {
		try {
			await unregister(shortcut);
		} catch {}

		await register(shortcut, (evt) => {
			if (evt.state !== "Pressed") return;
			console.log("shortcut pressed");

			if (isStarting || isStopping) return;
			if (isRecording()) {
				stopRecording();
			} else {
				startRecording();
			}
		});
	});

	onCleanup(async () => {
		if (isRecording()) {
			await stopRecording();
		}
		await unregister(shortcut);
	});

	const stopRecording = async () => {
		if (isStopping) return;
		if (!isRecording()) return;

		isStopping = true;
		setIsRecording(false);

		try {
			// Stop native recording and get the file path
			const filePath = await nativeStopRecording();
			console.log("Recording saved at:", filePath);

			// Read the recorded file and send to transcription API
			setLoading(true);

			// Read the file using Rust command
			const audioBytes = await invoke<number[]>("read_audio_file", { path: filePath });
			const audioBlob = new Blob([new Uint8Array(audioBytes)], { type: "audio/wav" });
			const audioFile = new File([audioBlob], "recording.wav", { type: "audio/wav" });

			const res = await eden.api.transcribe.post({ file: audioFile });

			// Handle both parsed data and raw Response
			let body: { text?: string };
			if (res.data instanceof Response) {
				body = await res.data.json();
			} else {
				body = res.data as { text?: string };
			}
			await invoke("type_text", { text: body?.text ?? "" });

			// Notify other windows that a new transcription was created
			// Small delay to allow afterResponse hook to save to database
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

			// Start native recording
			await nativeStartRecording();
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
								class="w-[5px] rounded bg-slate-500 animate-voice-wave"
								style={{ "animation-delay": `${i * 0.1}s` }}
							/>
						)}
					</For>
				</div>
			</Show>
		</div>
	);
}
