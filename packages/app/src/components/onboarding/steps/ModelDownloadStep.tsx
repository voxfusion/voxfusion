import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { Result } from "better-result";
import { AlertCircle, Check, Download, Loader } from "lucide-solid";
import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { useI18n } from "../../../i18n";
import {
	DEFAULT_MODEL_ID,
	type ModelDownloadProgress,
	cancelModelDownload,
	checkModelStatus,
	downloadWhisperModel,
} from "../../../lib/commands/model";

interface ModelDownloadStepProps {
	onDownloadComplete: () => void;
}

const BYTES_PER_MB = 1024 * 1024;
/** Backend error for a duplicate `download_model` call. */
const DOWNLOAD_IN_PROGRESS_ERROR = "download already in progress";
/** Backend error a cancelled `download_model` call resolves with. */
const DOWNLOAD_CANCELLED_ERROR = "Download cancelled";

const formatMb = (bytes: number): string => (bytes / BYTES_PER_MB).toFixed(1);

const formatEta = (etaSeconds: number): string => {
	const total = Math.max(0, Math.round(etaSeconds));
	const hours = Math.floor(total / 3600);
	const minutes = Math.floor((total % 3600) / 60);
	const seconds = total % 60;
	if (hours > 0) {
		return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
	}
	return `${minutes}:${String(seconds).padStart(2, "0")}`;
};

export default function ModelDownloadStep(props: ModelDownloadStepProps) {
	const [t] = useI18n();
	const [downloadProgress, setDownloadProgress] = createSignal(0);
	const [downloadedBytes, setDownloadedBytes] = createSignal<number | null>(null);
	const [totalBytes, setTotalBytes] = createSignal<number | null>(null);
	const [bytesPerSecond, setBytesPerSecond] = createSignal<number | null>(null);
	const [etaSeconds, setEtaSeconds] = createSignal<number | null>(null);
	const [isDownloading, setIsDownloading] = createSignal(false);
	const [isDownloaded, setIsDownloaded] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);
	const downloadSize = "~1.5 GB";
	let lastSample: { at: number; bytes: number } | null = null;
	let cancelRequested = false;

	const handleProgress = (payload: ModelDownloadProgress) => {
		if (payload.modelId !== DEFAULT_MODEL_ID) return;

		const now = Date.now();
		if (lastSample && now > lastSample.at && payload.downloadedBytes >= lastSample.bytes) {
			const instant = ((payload.downloadedBytes - lastSample.bytes) * 1000) / (now - lastSample.at);
			// Smooth the instantaneous rate so the readout doesn't flicker.
			setBytesPerSecond((previous) =>
				previous === null ? instant : previous * 0.7 + instant * 0.3
			);
		}
		lastSample = { at: now, bytes: payload.downloadedBytes };

		const speed = bytesPerSecond();
		const remainingBytes = payload.totalBytes - payload.downloadedBytes;
		setEtaSeconds(
			speed !== null && speed > 0 && remainingBytes > 0 ? remainingBytes / speed : null
		);
		setDownloadProgress(payload.progress);
		setDownloadedBytes(payload.downloadedBytes);
		setTotalBytes(payload.totalBytes);

		if (payload.progress >= 100) {
			setIsDownloaded(true);
			props.onDownloadComplete();
		} else {
			// A download may already be streaming (e.g. started from Settings
			// before this step mounted) — reflect it.
			setIsDownloading(true);
		}
	};

	onMount(() => {
		let unlisten: UnlistenFn | undefined;
		let disposed = false;

		void checkModelStatus().then((downloaded) => {
			if (disposed) return;
			if (Result.isOk(downloaded) && downloaded.value) {
				setIsDownloaded(true);
				setDownloadProgress(100);
				props.onDownloadComplete();
			}
		});

		void listen<ModelDownloadProgress>("model-download-progress", (event) => {
			handleProgress(event.payload);
		}).then((dispose) => {
			if (disposed) {
				dispose();
			} else {
				unlisten = dispose;
			}
		});

		onCleanup(() => {
			disposed = true;
			unlisten?.();
		});
	});

	const handleDownload = async () => {
		setIsDownloading(true);
		setError(null);
		cancelRequested = false;
		// Progress is intentionally not reset: the backend resumes a partial
		// download, so the bar jumps forward with the first event, not to 0.

		const result = await downloadWhisperModel();
		if (Result.isOk(result)) {
			setIsDownloaded(true);
			props.onDownloadComplete();
			return;
		}
		const message = result.error.message ?? "";
		if (message.includes(DOWNLOAD_IN_PROGRESS_ERROR)) {
			// The same download is already streaming; keep showing its progress.
			return;
		}
		setIsDownloading(false);
		if (!cancelRequested && !message.includes(DOWNLOAD_CANCELLED_ERROR)) {
			setError(message);
		}
		cancelRequested = false;
	};

	const handleCancel = async () => {
		cancelRequested = true;
		const result = await cancelModelDownload(DEFAULT_MODEL_ID);
		if (Result.isError(result)) {
			cancelRequested = false;
			setError(result.error.message);
		}
	};

	return (
		<div class="text-center max-w-md mx-auto">
			<div class="font-mono text-ac text-sm mb-8 tracking-wider">[STEP_06] &gt; DOWNLOAD_MODEL</div>

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
							<Show when={downloadedBytes() !== null && totalBytes() !== null}>
								<div class="font-mono text-xs text-txt-muted tabular-nums">
									<span>
										{t("settings.modelDownloadedOfTotal", {
											downloaded: formatMb(downloadedBytes() ?? 0),
											total: formatMb(totalBytes() ?? 0),
										})}
									</span>
									<Show when={bytesPerSecond() !== null}>
										<span>
											{" · "}
											{t("settings.modelDownloadSpeed", {
												speed: formatMb(bytesPerSecond() ?? 0),
											})}
										</span>
									</Show>
									<Show when={etaSeconds() !== null}>
										<span>
											{" · "}
											{t("settings.modelEta", { eta: formatEta(etaSeconds() ?? 0) })}
										</span>
									</Show>
								</div>
							</Show>
							<button
								type="button"
								onClick={handleCancel}
								class="px-4 py-2 border border-border-strong text-txt-secondary font-mono text-xs uppercase tracking-wider hover:border-ac hover:text-ac transition-colors"
							>
								{t("settings.cancel")}
							</button>
						</div>
					</Show>

					<Show when={!isDownloading() && !isDownloaded() && error() === null}>
						<div class="font-mono text-xs text-txt-muted mb-4">
							{t("onboarding.modelSize")}: {downloadSize}
						</div>
						<Show when={downloadProgress() > 0}>
							<p class="font-mono text-xs text-txt-muted mb-4">{t("settings.modelResumeNote")}</p>
						</Show>
						<button
							type="button"
							onClick={handleDownload}
							class="px-6 py-3 bg-ac text-ac-on font-mono font-bold uppercase tracking-wider text-sm hover:bg-ac-hover transition-colors"
						>
							{t("onboarding.downloadModel")}
						</button>
					</Show>

					<Show when={error() !== null && !isDownloaded()}>
						<div class="flex items-center justify-center gap-2 font-mono text-sm text-ac mt-3">
							<AlertCircle class="w-4 h-4" />
							<span>{t("settings.modelDownloadFailed")}</span>
						</div>
						<Show when={error()}>
							<p class="mt-2 font-mono text-xs text-txt-faint break-all">{error()}</p>
						</Show>
						<Show when={downloadProgress() > 0}>
							<p class="mt-2 font-mono text-xs text-txt-muted">{t("settings.modelResumeNote")}</p>
						</Show>
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
