import { listen } from "@tauri-apps/api/event";
import { AlertCircle, Check, Download, Loader } from "lucide-solid";
import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { useI18n } from "../../../i18n";
import { checkModelStatus, downloadWhisperModel } from "../../../lib/commands/model";

interface ModelDownloadStepProps {
	onDownloadComplete: () => void;
}

export default function ModelDownloadStep(props: ModelDownloadStepProps) {
	const [t] = useI18n();
	const [downloadProgress, setDownloadProgress] = createSignal(0);
	const [isDownloading, setIsDownloading] = createSignal(false);
	const [isDownloaded, setIsDownloaded] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const downloadSize = "~1.5 GB";

	onMount(async () => {
		try {
			const downloaded = await checkModelStatus();
			if (downloaded) {
				setIsDownloaded(true);
				setDownloadProgress(100);
				props.onDownloadComplete();
			}
		} catch {}

		const unlisten = await listen<number>("model-download-progress", (event) => {
			setDownloadProgress(event.payload);
			if (event.payload >= 100) {
				setIsDownloaded(true);
				props.onDownloadComplete();
			}
		});
		onCleanup(() => unlisten());
	});

	const handleDownload = async () => {
		setIsDownloading(true);
		setError(null);

		try {
			await downloadWhisperModel();
			setIsDownloaded(true);
			props.onDownloadComplete();
		} catch (err) {
			setError(err instanceof Error ? err.message : "Download failed");
			setIsDownloading(false);
		}
	};

	return (
		<div class="text-center max-w-md mx-auto">
			<div class="font-mono text-ac text-sm mb-8 tracking-wider">[STEP_05] &gt; DOWNLOAD_MODEL</div>

			<div class="border border-border bg-th-surface p-8">
				<div class="w-16 h-16 border border-border-strong flex items-center justify-center mx-auto mb-6">
					<Show when={isDownloaded()} fallback={<Download class="w-8 h-8 text-ac" />}>
						<Check class="w-8 h-8 text-success" />
					</Show>
				</div>

				<h2 class="font-mono text-xl uppercase tracking-wider text-txt-primary mb-3">
					{t("onboarding.modelDownloadTitle")}
				</h2>

				<p class="font-mono text-sm text-txt-secondary mb-8">
					{t("onboarding.modelDownloadDescription")}
				</p>

				<div class="mb-6">
					<Show when={isDownloaded()}>
						<div class="flex items-center justify-center gap-2 font-mono text-sm text-success">
							<Check class="w-5 h-5" />
							<span>{t("onboarding.modelDownloadComplete")}</span>
						</div>
					</Show>

					<Show when={isDownloading() && !isDownloaded()}>
						<div class="space-y-3">
							<div class="w-full h-2 bg-border overflow-hidden">
								<div
									class="h-full bg-ac transition-all duration-300"
									style={{ width: `${downloadProgress()}%` }}
								/>
							</div>
							<div class="flex items-center justify-center gap-2 font-mono text-sm text-txt-secondary">
								<Loader class="w-4 h-4 animate-spin" />
								<span>
									{downloadProgress()}% — {t("onboarding.modelDownloading")}
								</span>
							</div>
						</div>
					</Show>

					<Show when={!isDownloading() && !isDownloaded()}>
						<div class="font-mono text-xs text-txt-muted mb-4">
							{t("onboarding.modelSize")}: {downloadSize}
						</div>
						<button
							type="button"
							onClick={handleDownload}
							class="px-6 py-3 bg-ac text-ac-on font-mono font-bold uppercase tracking-wider text-sm hover:bg-ac-hover transition-colors"
						>
							{t("onboarding.downloadModel")}
						</button>
					</Show>

					<Show when={error()}>
						<div class="flex items-center justify-center gap-2 font-mono text-sm text-ac mt-3">
							<AlertCircle class="w-4 h-4" />
							<span>{error()}</span>
						</div>
						<button
							type="button"
							onClick={handleDownload}
							class="mt-3 px-4 py-2 border border-ac text-ac font-mono text-xs uppercase tracking-wider hover:bg-ac hover:text-ac-on transition-colors"
						>
							{t("onboarding.retryDownload")}
						</button>
					</Show>
				</div>

				<div class="mt-6 pt-4 border-t border-border">
					<p class="font-mono text-xs text-txt-muted">{t("onboarding.modelDownloadNote")}</p>
				</div>
			</div>
		</div>
	);
}
