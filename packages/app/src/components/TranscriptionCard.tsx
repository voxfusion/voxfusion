import { createSignal, Show } from "solid-js";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Copy, Check } from "lucide-solid";
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
};

export default function TranscriptionCard(props: Props) {
	const [t, { locale }] = useI18n();
	const [copied, setCopied] = createSignal(false);
	const [isHovered, setIsHovered] = createSignal(false);

	const handleCopy = async () => {
		await writeText(props.transcription.text);
		setCopied(true);
		setTimeout(() => setCopied(false), 2000);
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

	const formatTime = (date: Date) => {
		return date.toLocaleTimeString(locale(), {
			hour: "2-digit",
			minute: "2-digit",
		});
	};

	const truncateText = (text: string, maxLength = 150) => {
		if (text.length <= maxLength) return text;
		return `${text.slice(0, maxLength)}...`;
	};

	return (
		<div
			class="group relative p-3 hover:bg-slate-50 transition-colors"
			onMouseEnter={() => setIsHovered(true)}
			onMouseLeave={() => setIsHovered(false)}
		>
			<p class="text-slate-700 text-sm leading-relaxed mb-2">
				{truncateText(props.transcription.text)}
			</p>

			<div class="flex items-center gap-3 text-xs text-slate-400">
				<span>{formatTime(props.transcription.createdAt)}</span>
				<span>{formatDuration(props.transcription.audioDurationMs)}</span>
				<span>{formatProcessingTime(props.transcription.processingTimeMs)}</span>
			</div>

			<Show when={isHovered() || copied()}>
				<button
					type="button"
					onClick={handleCopy}
					class="absolute top-2 right-2 p-1.5 rounded-md bg-white border border-slate-200 text-slate-500 hover:text-slate-700 hover:bg-slate-100 transition-colors shadow-sm"
					title={copied() ? t("transcription.copied") : t("transcription.copy")}
				>
					<Show when={copied()} fallback={<Copy class="w-3.5 h-3.5" />}>
						<Check class="w-3.5 h-3.5 text-green-600" />
					</Show>
				</button>
			</Show>
		</div>
	);
}
