import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Check, Copy } from "lucide-solid";
import { Show, createSignal } from "solid-js";
import { useI18n } from "../i18n";
import { capture } from "../lib/posthog";
import type { Transcription } from "../types";

type Props = {
	transcription: Transcription;
};

export default function TranscriptionCard(props: Props) {
	const [t, { locale }] = useI18n();
	const [copied, setCopied] = createSignal(false);
	const [isHovered, setIsHovered] = createSignal(false);

	const handleCopy = async () => {
		await writeText(props.transcription.text);
		capture("transcription_copied", {
			transcription_id: props.transcription.id,
			text_length: props.transcription.text.length,
		});
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
			class="group relative p-3 hover:bg-th-hover transition-colors"
			onMouseEnter={() => setIsHovered(true)}
			onMouseLeave={() => setIsHovered(false)}
		>
			<p class="text-txt-primary font-mono text-sm leading-relaxed mb-2">
				{truncateText(props.transcription.text)}
			</p>

			<div class="flex items-center gap-3 text-txt-muted font-mono text-xs">
				<span>{formatTime(props.transcription.createdAt)}</span>
				<span>{formatProcessingTime(props.transcription.processingTimeMs)}</span>
			</div>

			<Show when={isHovered() || copied()}>
				<button
					type="button"
					onClick={handleCopy}
					class="absolute top-2 right-2 p-1.5 bg-th-base border border-border-strong hover:border-ac text-txt-secondary transition-colors"
					title={copied() ? t("transcription.copied") : t("transcription.copy")}
				>
					<Show when={copied()} fallback={<Copy class="w-3.5 h-3.5" />}>
						<Check class="w-3.5 h-3.5 text-success" />
					</Show>
				</button>
			</Show>
		</div>
	);
}
