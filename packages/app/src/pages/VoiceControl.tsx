import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { invoke } from "@tauri-apps/api/core";
import { createSignal, For, onMount, onCleanup } from "solid-js";
import eden from "../lib/eden";

export default function VoiceControl() {
	const [isRecording, setIsRecording] = createSignal(false);
	const [audioLevels, setAudioLevels] = createSignal<number[]>([4, 4, 4, 4, 4, 4, 4, 4, 4, 4]);

	let mediaRecorder: MediaRecorder | null = null;
	let audioChunks: Blob[] = [];

	let audioContext: AudioContext | null = null;
	let analyser: AnalyserNode | null = null;
	let animationId: number | null = null;
	let recordingMimeType = "";
	const shortcut = "Command+;";
	let stream: MediaStream | null = null;
	let isStopping = false;
	let isStarting = false;

	const getMicStream = async () => {
		const hasLiveTracks = stream?.getTracks().some((track) => track.readyState === "live") ?? false;
		if (stream && hasLiveTracks) return stream;

		for (const track of stream?.getTracks() ?? []) {
			track.stop();
		}
		stream = await navigator.mediaDevices.getUserMedia({
			audio: {
				echoCancellation: true,
				noiseSuppression: true,
				sampleRate: 44100,
			},
		});
		return stream;
	};

	onMount(async () => {
		try {
			await unregister(shortcut);
		} catch {}

		await register(shortcut, (evt) => {
			if (evt.state !== "Pressed") return;

			console.log("Shortcut pressed");
			if (isStarting || isStopping) return;
			if (isRecording()) {
				stopRecording();
			} else {
				startRecording();
			}
		});

		void getMicStream().catch((err) => console.warn("Failed to warm mic stream:", err));
	});

	onCleanup(async () => {
		stopRecording();
		for (const track of stream?.getTracks() ?? []) {
			track.stop();
		}
		stream = null;
		await unregister(shortcut);
	});

	const stopRecording = () => {
		if (isStopping) return;
		if (!mediaRecorder) return;
		if (mediaRecorder.state === "inactive") return;

		isStopping = true;
		mediaRecorder.stop();
		setIsRecording(false);
		if (animationId) {
			cancelAnimationFrame(animationId);
			animationId = null;
		}
	};

	const startRecording = async () => {
		try {
			if (isRecording()) return;
			audioChunks = [];
			if (isStopping || isStarting) return;
			isStarting = true;

			const micStream = await getMicStream();

			recordingMimeType = MediaRecorder.isTypeSupported("audio/webm;codecs=opus")
				? "audio/webm;codecs=opus"
				: MediaRecorder.isTypeSupported("audio/mp4")
					? "audio/mp4"
					: "audio/wav";

			mediaRecorder = new MediaRecorder(
				micStream,
				recordingMimeType ? { mimeType: recordingMimeType } : undefined
			);

			audioContext = new AudioContext();
			analyser = audioContext.createAnalyser();
			analyser.fftSize = 256;
			const source = audioContext.createMediaStreamSource(micStream);
			source.connect(analyser);

			mediaRecorder.ondataavailable = async (event) => {
				if (event.data.size > 0) {
					audioChunks.push(event.data);
				}
			};

			mediaRecorder.onstop = async () => {
				const chunks = audioChunks;
				const mimeType = recordingMimeType;
				audioChunks = [];

				if (animationId) {
					cancelAnimationFrame(animationId);
					animationId = null;
				}
				if (audioContext) {
					audioContext.close();
					audioContext = null;
				}
				analyser = null;
				setAudioLevels([4, 4, 4, 4, 4, 4, 4, 4, 4, 4]);

				mediaRecorder = null;
				isStopping = false;

				void (async () => {
					try {
						const audioBlob = new Blob(chunks, { type: mimeType });
						const audioFile = new File([audioBlob], "recording.webm", { type: mimeType });
						const response = await eden.api.transcribe.post({ file: audioFile });

						await writeText(response.data?.text ?? "");
						await invoke("type_text", { text: response.data?.text ?? "" });
					} catch (err) {
						console.error("Transcription failed:", err);
					}
				})();
			};

			const updateAudioLevels = () => {
				if (!analyser) return;

				const dataArray = new Uint8Array(analyser.frequencyBinCount);
				analyser.getByteFrequencyData(dataArray);

				const levels = [];
				for (let i = 0; i < 10; i++) {
					const start = i * 5;
					const end = start + 5;
					const slice = dataArray.slice(start, end);
					const avg = slice.reduce((a, b) => a + b, 0) / slice.length;
					levels.push(Math.max(4, Math.floor((avg / 255) * 20)));
				}
				setAudioLevels(levels);

				animationId = requestAnimationFrame(updateAudioLevels);
			};
			updateAudioLevels();

			mediaRecorder?.start(100);
			setIsRecording(true);
			isStarting = false;
		} catch (err) {
			console.error("Failed to start recording:", err);
			isStopping = false;
			isStarting = false;
		}
	};

	return (
		<div class="h-[100vh] bg-black">
			<div class="flex items-center justify-center gap-[2px] ">
				<For each={audioLevels()}>
					{(level) => (
						<div class="w-[6px] h-10 rounded bg-slate-500" style={{ height: `${level}px` }} />
					)}
				</For>
			</div>
		</div>
	);
}
