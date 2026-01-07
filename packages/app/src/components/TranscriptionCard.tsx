import { createSignal, Show } from "solid-js";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Copy, Download, ThumbsUp, ThumbsDown, Check } from "lucide-solid";
import eden from "../lib/eden";

type Transcription = {
	id: string;
	text: string;
	fileUrl: string;
	processingTimeMs: number;
	audioDurationMs: number | null;
	rating: string | null;
	createdAt: Date;
};

type Props = {
	transcription: Transcription;
	onRatingChange?: (id: string, rating: "up" | "down" | null) => void;
};

export default function TranscriptionCard(props: Props) {
	const [copied, setCopied] = createSignal(false);
	const [rating, setRating] = createSignal<"up" | "down" | null>(
		props.transcription.rating as "up" | "down" | null,
	);

	const handleCopy = async () => {
		await writeText(props.transcription.text);
		setCopied(true);
		setTimeout(() => setCopied(false), 2000);
	};

	const handleDownload = () => {
		window.open(props.transcription.fileUrl, "_blank");
	};

	const handleRate = async (newRating: "up" | "down") => {
		const currentRating = rating();
		const ratingToSet = currentRating === newRating ? null : newRating;

		setRating(ratingToSet);

		try {
			await eden.api.transcribe({ id: props.transcription.id }).rating.patch({
				rating: ratingToSet,
			});
			props.onRatingChange?.(props.transcription.id, ratingToSet);
		} catch (error) {
			setRating(currentRating);
			console.error("Failed to update rating:", error);
		}
	};

	const formatDuration = (ms: number | null) => {
		if (!ms) return "N/A";
		const seconds = Math.floor(ms / 1000);
		const minutes = Math.floor(seconds / 60);
		const remainingSeconds = seconds % 60;
		return minutes > 0 ? `${minutes}m ${remainingSeconds}s` : `${remainingSeconds}s`;
	};

	const formatProcessingTime = (ms: number) => {
		return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
	};

	const formatDate = (date: Date) => {
		return date.toLocaleDateString(undefined, {
			month: "short",
			day: "numeric",
			hour: "2-digit",
			minute: "2-digit",
		});
	};

	const truncateText = (text: string, maxLength = 150) => {
		if (text.length <= maxLength) return text;
		return `${text.slice(0, maxLength)}...`;
	};

	return (
		<div class="bg-white rounded-xl border border-slate-200 p-4 hover:shadow-md transition-shadow">
			<p class="text-slate-700 text-sm leading-relaxed mb-3">
				{truncateText(props.transcription.text)}
			</p>

			<div class="flex items-center gap-4 text-xs text-slate-500 mb-3">
				<span title="Processing time">
					Processing: {formatProcessingTime(props.transcription.processingTimeMs)}
				</span>
				<span title="Audio duration">
					Duration: {formatDuration(props.transcription.audioDurationMs)}
				</span>
				<span class="ml-auto">{formatDate(props.transcription.createdAt)}</span>
			</div>

			<div class="flex items-center gap-2 pt-3 border-t border-slate-100">
				<button
					type="button"
					onClick={handleCopy}
					class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-slate-600 hover:text-slate-800 hover:bg-slate-100 rounded-lg transition-colors"
				>
					<Show when={copied()} fallback={<Copy class="w-4 h-4" />}>
						<Check class="w-4 h-4 text-green-600" />
					</Show>
					<span>{copied() ? "Copied!" : "Copy"}</span>
				</button>

				<button
					type="button"
					onClick={handleDownload}
					class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-slate-600 hover:text-slate-800 hover:bg-slate-100 rounded-lg transition-colors"
				>
					<Download class="w-4 h-4" />
					<span>Download</span>
				</button>

				<div class="flex items-center gap-1 ml-auto">
					<button
						type="button"
						onClick={() => handleRate("up")}
						class={`p-1.5 rounded-lg transition-colors ${
							rating() === "up"
								? "text-green-600 bg-green-50"
								: "text-slate-400 hover:text-green-600 hover:bg-green-50"
						}`}
						title="Good transcription"
					>
						<ThumbsUp class="w-4 h-4" />
					</button>
					<button
						type="button"
						onClick={() => handleRate("down")}
						class={`p-1.5 rounded-lg transition-colors ${
							rating() === "down"
								? "text-red-600 bg-red-50"
								: "text-slate-400 hover:text-red-600 hover:bg-red-50"
						}`}
						title="Poor transcription"
					>
						<ThumbsDown class="w-4 h-4" />
					</button>
				</div>
			</div>
		</div>
	);
}
