import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { invoke } from "@tauri-apps/api/core";
import { createSignal, For, onMount, onCleanup } from "solid-js";
import eden from "../lib/eden";

export default function VoiceControl() {
	const [isRecording, setIsRecording] = createSignal(false);
	const [error, setError] = createSignal("");
	const [audioLevels, setAudioLevels] = createSignal<number[]>([4, 4, 4, 4, 4]);

	let mediaRecorder: MediaRecorder | null = null;
	let audioChunks: Blob[] = [];
	let audioContext: AudioContext | null = null;
	let analyser: AnalyserNode | null = null;
	let animationId: number | null = null;
	let recordingMimeType = "";
	const shortcut = "Command+;";

	onMount(async () => {
		try {
			await unregister(shortcut);
		} catch {}

		await register(shortcut, (evt) => {
			if (evt.state !== "Pressed") return;

			if (isRecording()) {
				stopRecording();
			} else {
				startRecording();
			}
		});
	});

	onCleanup(async () => {
		await unregister(shortcut);
	});

	const stopRecording = () => {
		mediaRecorder?.stop();
		setIsRecording(false);
	};

	const startRecording = async () => {
		try {
			const stream = await navigator.mediaDevices.getUserMedia({
				audio: {
					echoCancellation: true,
					noiseSuppression: true,
					sampleRate: 44100,
				},
			});

			audioContext = new AudioContext();
			analyser = audioContext.createAnalyser();
			analyser.fftSize = 256;
			const source = audioContext.createMediaStreamSource(stream);
			source.connect(analyser);

			recordingMimeType = MediaRecorder.isTypeSupported("audio/webm;codecs=opus")
				? "audio/webm;codecs=opus"
				: MediaRecorder.isTypeSupported("audio/mp4")
					? "audio/mp4"
					: "audio/wav";
			mediaRecorder = new MediaRecorder(
				stream,
				recordingMimeType ? { mimeType: recordingMimeType } : undefined
			);

			audioChunks = [];

			mediaRecorder.ondataavailable = async (event) => {
				if (event.data.size > 0) {
					audioChunks.push(event.data);
				}
			};

			mediaRecorder.onstop = async () => {
				// Stop all tracks
				for (const track of stream.getTracks()) {
					track.stop();
				}

				// Stop visualization
				if (animationId) {
					cancelAnimationFrame(animationId);
					animationId = null;
				}
				if (audioContext) {
					audioContext.close();
					audioContext = null;
				}
				setAudioLevels([4, 4, 4, 4, 4]);

				// Create audio blob and send to server
				const audioBlob = new Blob(audioChunks, { type: recordingMimeType });
				const audioFile = new File([audioBlob], "recording.webm", { type: recordingMimeType });
				const response = await eden.api.transcribe.post({ file: audioFile });

				await writeText(response.data?.text as string);
				await invoke("trigger_paste");
			};
			// Start audio level visualization
			const updateAudioLevels = () => {
				if (!analyser) return;

				const dataArray = new Uint8Array(analyser.frequencyBinCount);
				analyser.getByteFrequencyData(dataArray);

				// Only use first 25 bins (~0-4300 Hz) where voice energy exists
				// Split into 5 bands of 5 bins each
				const levels = [];
				for (let i = 0; i < 5; i++) {
					const start = i * 5;
					const end = start + 5;
					const slice = dataArray.slice(start, end);
					const avg = slice.reduce((a, b) => a + b, 0) / slice.length;
					// Normalize to 4-40 range
					levels.push(Math.max(4, Math.floor((avg / 255) * 40)));
				}
				setAudioLevels(levels);

				animationId = requestAnimationFrame(updateAudioLevels);
			};
			updateAudioLevels();

			mediaRecorder.start(100); // Collect data every 100ms
			setIsRecording(true);
		} catch (err) {
			console.error("Failed to start recording:", err);
			setError("Microphone access denied");
		}
	};

	return (
		<>
			<div class="flex items-center justify-center">
				<For each={audioLevels()}>
					{(level, index) => <div class="w-2 h-20 bg-blue-500" style={{ height: `${level}px` }} />}
				</For>
			</div>
		</>
	);
}
