import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { Result } from "better-result";
import { AlertCircle, Check, Download, Loader } from "lucide-solid";
import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import type { I18nContextType } from "../../i18n";
import {
	type ModelDownloadProgress,
	type ModelInfo,
	cancelModelDownload,
	downloadModel,
	listModels,
	setActiveModel,
} from "../../lib/commands/model";
import { capture } from "../../lib/posthog";

interface ModelSettingsProps {
	t: I18nContextType[0];
}

interface DownloadStats {
	progress: number;
	downloadedBytes: number;
	totalBytes: number;
	bytesPerSecond: number | null;
	etaSeconds: number | null;
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

export default function ModelSettings(props: ModelSettingsProps) {
	const [models, setModels] = createSignal<ModelInfo[]>([]);
	const [downloadStats, setDownloadStats] = createSignal<Record<string, DownloadStats>>({});
	const [downloadingId, setDownloadingId] = createSignal<string | null>(null);
	const [busyId, setBusyId] = createSignal<string | null>(null);
	const [error, setError] = createSignal<string | null>(null);
	const lastSamples: Record<string, { at: number; bytes: number }> = {};
	const cancelRequested = new Set<string>();

	const refresh = async () => {
		const result = await listModels();
		if (Result.isOk(result)) {
			setModels(result.value);
		} else {
			setError(result.error.message);
		}
	};

	const handleProgress = (payload: ModelDownloadProgress) => {
		const now = Date.now();
		const sample = lastSamples[payload.modelId];
		const previousSpeed = downloadStats()[payload.modelId]?.bytesPerSecond ?? null;
		let bytesPerSecond = previousSpeed;
		if (sample && now > sample.at && payload.downloadedBytes >= sample.bytes) {
			const instant = ((payload.downloadedBytes - sample.bytes) * 1000) / (now - sample.at);
			// Smooth the instantaneous rate so the readout doesn't flicker.
			bytesPerSecond = previousSpeed === null ? instant : previousSpeed * 0.7 + instant * 0.3;
		}
		lastSamples[payload.modelId] = { at: now, bytes: payload.downloadedBytes };

		const remainingBytes = payload.totalBytes - payload.downloadedBytes;
		const etaSeconds =
			bytesPerSecond !== null && bytesPerSecond > 0 && remainingBytes > 0
				? remainingBytes / bytesPerSecond
				: null;

		setDownloadStats((prev) => ({
			...prev,
			[payload.modelId]: {
				progress: payload.progress,
				downloadedBytes: payload.downloadedBytes,
				totalBytes: payload.totalBytes,
				bytesPerSecond,
				etaSeconds,
			},
		}));

		if (payload.progress >= 100) {
			setDownloadingId((current) => (current === payload.modelId ? null : current));
			void refresh();
		} else {
			// A download may already be streaming (e.g. started before this
			// section was opened) — reflect it.
			setDownloadingId((current) => current ?? payload.modelId);
		}
	};

	onMount(() => {
		void refresh();

		let unlisten: UnlistenFn | undefined;
		let disposed = false;
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

	const handleDownload = async (model: ModelInfo) => {
		setError(null);
		cancelRequested.delete(model.id);
		// Keep any previous partial progress on screen: the backend resumes the
		// download, so the bar jumps forward with the first event, not back to 0.
		setDownloadingId(model.id);
		capture("settings_model_download_started", { model: model.id });

		const result = await downloadModel(model.id);
		const wasCancelled = cancelRequested.delete(model.id);
		if (Result.isError(result)) {
			const message = result.error.message ?? "";
			if (message.includes(DOWNLOAD_IN_PROGRESS_ERROR)) {
				// The same download is already streaming; keep showing its progress.
				return;
			}
			setDownloadingId((current) => (current === model.id ? null : current));
			if (!wasCancelled && !message.includes(DOWNLOAD_CANCELLED_ERROR)) {
				setError(message);
			}
		} else {
			setDownloadingId((current) => (current === model.id ? null : current));
		}
		await refresh();
	};

	const handleCancel = async (model: ModelInfo) => {
		cancelRequested.add(model.id);
		const result = await cancelModelDownload(model.id);
		if (Result.isError(result)) {
			cancelRequested.delete(model.id);
			setError(result.error.message);
		}
	};

	const handleUse = async (model: ModelInfo) => {
		setError(null);
		setBusyId(model.id);
		const result = await setActiveModel(model.id);
		setBusyId(null);
		if (Result.isError(result)) {
			setError(result.error.message);
			return;
		}
		capture("settings_model_changed", { model: model.id });
		await refresh();
	};

	const partialStats = (model: ModelInfo): DownloadStats | null => {
		const stats = downloadStats()[model.id];
		if (!stats || stats.progress <= 0 || stats.progress >= 100) return null;
		return stats;
	};

	return (
		<div class="space-y-6">
			<p class="font-mono text-xs text-txt-faint">{props.t("settings.modelsDescription")}</p>

			<div class="space-y-3">
				<For each={models()}>
					{(model) => (
						<div
							class={`border p-4 ${
								model.active ? "border-ac bg-ac-bg" : "border-border-strong bg-th-surface"
							}`}
						>
							<div class="flex items-start justify-between gap-3">
								<div class="min-w-0">
									<div class="flex items-center gap-2 flex-wrap">
										<span class="font-mono text-sm text-txt-primary uppercase tracking-wider truncate">
											{model.name}
										</span>
										<Show when={model.recommended}>
											<span class="font-mono text-[10px] uppercase tracking-wider text-ac border border-ac px-1.5 py-0.5">
												{props.t("settings.modelRecommended")}
											</span>
										</Show>
										<Show when={model.experimental}>
											<span class="font-mono text-[10px] uppercase tracking-wider text-txt-muted border border-border-strong px-1.5 py-0.5">
												{props.t("settings.modelExperimental")}
											</span>
										</Show>
									</div>
									<div class="mt-1.5 font-mono text-xs text-txt-faint">
										{model.size_label} · {model.languages}
									</div>
								</div>

								<div class="shrink-0">
									<Show when={model.active}>
										<div class="flex items-center gap-1.5 font-mono text-xs text-success uppercase tracking-wider">
											<Check class="w-4 h-4" />
											{props.t("settings.modelInUse")}
										</div>
									</Show>

									<Show when={!model.active && downloadingId() !== model.id}>
										<Show
											when={model.downloaded}
											fallback={
												<button
													type="button"
													onClick={() => handleDownload(model)}
													class="flex items-center gap-1.5 px-3 py-1.5 bg-ac text-ac-on font-mono text-xs uppercase tracking-wider hover:bg-ac-hover transition-colors"
												>
													<Download class="w-3.5 h-3.5" />
													{props.t("settings.modelDownload")}
												</button>
											}
										>
											<Show
												when={!model.experimental}
												fallback={
													<div class="flex items-center gap-1.5 font-mono text-xs text-txt-muted uppercase tracking-wider">
														<Check class="w-3.5 h-3.5 text-success" />
														{props.t("settings.modelDownloaded")}
													</div>
												}
											>
												<button
													type="button"
													onClick={() => handleUse(model)}
													disabled={busyId() === model.id}
													class="px-3 py-1.5 border border-ac text-ac font-mono text-xs uppercase tracking-wider hover:bg-ac hover:text-ac-on transition-colors disabled:opacity-50"
												>
													{props.t("settings.modelUse")}
												</button>
											</Show>
										</Show>
									</Show>
								</div>
							</div>

							<Show when={downloadingId() === model.id}>
								<div class="mt-3 space-y-2">
									<div class="w-full h-2 bg-border overflow-hidden">
										<div
											class="h-full bg-ac transition-all duration-300"
											style={{ width: `${downloadStats()[model.id]?.progress ?? 0}%` }}
										/>
									</div>
									<div class="flex items-center justify-between gap-2">
										<div class="flex items-center gap-2 font-mono text-xs text-txt-secondary">
											<Loader class="w-3.5 h-3.5 animate-spin" />
											<span>
												{downloadStats()[model.id]?.progress ?? 0}% —{" "}
												{props.t("settings.modelDownloading")}
											</span>
										</div>
										<button
											type="button"
											onClick={() => handleCancel(model)}
											class="px-2 py-1 border border-border-strong text-txt-secondary font-mono text-[10px] uppercase tracking-wider hover:border-ac hover:text-ac transition-colors"
										>
											{props.t("settings.cancel")}
										</button>
									</div>
									<Show when={downloadStats()[model.id]}>
										{(stats) => (
											<div class="font-mono text-[11px] text-txt-faint tabular-nums">
												<span>
													{props.t("settings.modelDownloadedOfTotal", {
														downloaded: formatMb(stats().downloadedBytes),
														total: formatMb(stats().totalBytes),
													})}
												</span>
												<Show when={stats().bytesPerSecond !== null}>
													<span>
														{" · "}
														{props.t("settings.modelDownloadSpeed", {
															speed: formatMb(stats().bytesPerSecond ?? 0),
														})}
													</span>
												</Show>
												<Show when={stats().etaSeconds !== null}>
													<span>
														{" · "}
														{props.t("settings.modelEta", {
															eta: formatEta(stats().etaSeconds ?? 0),
														})}
													</span>
												</Show>
											</div>
										)}
									</Show>
								</div>
							</Show>

							<Show when={downloadingId() !== model.id && !model.downloaded && partialStats(model)}>
								<p class="mt-3 font-mono text-[11px] text-txt-faint leading-relaxed">
									{props.t("settings.modelResumeNote")}
								</p>
							</Show>

							<Show when={model.experimental}>
								<p class="mt-3 font-mono text-[11px] text-txt-faint leading-relaxed">
									{props.t("settings.modelExperimentalNote")}
								</p>
							</Show>
						</div>
					)}
				</For>
			</div>

			<Show when={error() !== null}>
				<div class="space-y-1">
					<div class="flex items-center gap-2 font-mono text-xs text-ac">
						<AlertCircle class="w-4 h-4" />
						<span>{props.t("settings.modelDownloadFailed")}</span>
					</div>
					<Show when={error()}>
						<p class="font-mono text-[11px] text-txt-faint break-all">{error()}</p>
					</Show>
				</div>
			</Show>
		</div>
	);
}
