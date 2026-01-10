import { createSignal, Show } from "solid-js";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Copy, ThumbsUp, ThumbsDown, Check } from "lucide-solid";
import eden from "../lib/eden";
import { useI18n } from "../i18n";

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
	const [t, { locale }] = useI18n();
	const [copied, setCopied] = createSignal(false);
	const [rating, setRating] = createSignal<"up" | "down" | null>(
		props.transcription.rating as "up" | "down" | null,
	);

	const handleCopy = async () => {
		await writeText(props.transcription.text);
		setCopied(true);
		setTimeout(() => setCopied(false), 2000);
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
		if (!ms) return t("transcription.notAvailable");
		const seconds = Math.floor(ms / 1000);
		const minutes = Math.floor(seconds / 60);
		const remainingSeconds = seconds % 60;
		return minutes > 0 ? `${minutes}m ${remainingSeconds}s` : `${remainingSeconds}s`;
	};

	const formatProcessingTime = (ms: number) => {
		return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
	};

	const formatDate = (date: Date) => {
		return date.toLocaleDateString(locale(), {
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
		<div class="bg-white dark:bg-midnight-800 rounded-xl border border-slate-200 dark:border-midnight-700 p-4 hover:shadow-md dark:hover:shadow-midnight-900/50 transition-all">
			<p class="text-slate-700 dark:text-slate-200 text-sm leading-relaxed mb-3">
				{truncateText(props.transcription.text)}
			</p>

			<div class="flex items-center gap-4 text-xs text-slate-500 dark:text-slate-400 mb-3">
				<span title="Processing time">
					{t("transcription.processing")} {formatProcessingTime(props.transcription.processingTimeMs)}
				</span>
				<span title="Audio duration">
					{t("transcription.duration")} {formatDuration(props.transcription.audioDurationMs)}
				</span>
				<span class="ml-auto select-text">{formatDate(props.transcription.createdAt)}</span>
			</div>

			<div class="flex items-center gap-2 pt-3 border-t border-slate-100 dark:border-midnight-700">
				<button
					type="button"
					onClick={handleCopy}
					class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-slate-600 dark:text-slate-400 hover:text-slate-800 dark:hover:text-white hover:bg-slate-100 dark:hover:bg-midnight-700 rounded-lg transition-colors"
				>
					<Show when={copied()} fallback={<Copy class="w-4 h-4" />}>
						<Check class="w-4 h-4 text-green-600 dark:text-green-400" />
					</Show>
					<span>{copied() ? t("transcription.copied") : t("transcription.copy")}</span>
				</button>

				<div class="flex items-center gap-1 ml-auto">
					<button
						type="button"
						onClick={() => handleRate("up")}
						class={`p-1.5 rounded-lg transition-colors ${
							rating() === "up"
								? "text-green-600 dark:text-green-400 bg-green-50 dark:bg-green-900/20"
								: "text-slate-400 dark:text-slate-500 hover:text-green-600 dark:hover:text-green-400 hover:bg-green-50 dark:hover:bg-green-900/20"
						}`}
						title={t("transcription.goodTranscription")}
					>
						<ThumbsUp class="w-4 h-4" />
					</button>
					<button
						type="button"
						onClick={() => handleRate("down")}
						class={`p-1.5 rounded-lg transition-colors ${
							rating() === "down"
								? "text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-900/20"
								: "text-slate-400 dark:text-slate-500 hover:text-red-600 dark:hover:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20"
						}`}
						title={t("transcription.poorTranscription")}
					>
						<ThumbsDown class="w-4 h-4" />
					</button>
				</div>
			</div>
		</div>
	);
}
