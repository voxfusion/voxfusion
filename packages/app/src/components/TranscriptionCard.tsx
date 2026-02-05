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
			class="group relative p-3 hover:bg-[#1a1a1a] transition-colors"
			onMouseEnter={() => setIsHovered(true)}
			onMouseLeave={() => setIsHovered(false)}
		>
			<p class="text-[#e0e0e0] font-mono text-sm leading-relaxed mb-2">
				{truncateText(props.transcription.text)}
			</p>

			<div class="flex items-center gap-3 text-[#666] font-mono text-xs">
				<span>{formatTime(props.transcription.createdAt)}</span>
				<span>{formatProcessingTime(props.transcription.processingTimeMs)}</span>
			</div>

			<Show when={isHovered() || copied()}>
				<button
					type="button"
					onClick={handleCopy}
					class="absolute top-2 right-2 p-1.5 bg-[#0a0a0a] border border-[#333] hover:border-[#ff3e00] text-[#888] transition-colors"
					title={copied() ? t("transcription.copied") : t("transcription.copy")}
				>
					<Show when={copied()} fallback={<Copy class="w-3.5 h-3.5" />}>
						<Check class="w-3.5 h-3.5 text-[#00ff88]" />
					</Show>
				</button>
			</Show>
		</div>
	);
}
