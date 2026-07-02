import { listen } from "@tauri-apps/api/event";
import { relaunch } from "@tauri-apps/plugin-process";
import { type Update, check } from "@tauri-apps/plugin-updater";
import { Result } from "better-result";
import { Download } from "lucide-solid";
import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { useI18n } from "../i18n";
import { errorFields, logDiagnostic } from "../lib/diagnostics";

const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
const BYTES_PER_MB = 1024 * 1024;

const formatMb = (bytes: number): string => (bytes / BYTES_PER_MB).toFixed(1);

export default function UpdateNotification() {
	const [t] = useI18n();
	const [update, setUpdate] = createSignal<Update | null>(null);
	const [isVisible, setIsVisible] = createSignal(false);
	const [isDownloading, setIsDownloading] = createSignal(false);
	const [downloadedBytes, setDownloadedBytes] = createSignal(0);
	const [contentLength, setContentLength] = createSignal<number | null>(null);
	let isCheckingForUpdates = false;
	let dismissedVersion: string | null = null;

	const downloadPercent = () => {
		const total = contentLength();
		if (!total) return 0;
		return Math.min(100, (downloadedBytes() / total) * 100);
	};

	const checkForUpdates = async () => {
		if (isCheckingForUpdates || isDownloading()) {
			logDiagnostic("debug", "updater", "check_skipped", {
				isCheckingForUpdates,
				isDownloading: isDownloading(),
			});
			return;
		}

		isCheckingForUpdates = true;
		const startedAt = Date.now();
		logDiagnostic("debug", "updater", "check_started");
		try {
			const available = await Result.tryPromise(() => check());
			if (Result.isOk(available) && available.value) {
				if (available.value.version === dismissedVersion) {
					logDiagnostic("debug", "updater", "update_dismissed_for_session", {
						version: available.value.version,
					});
					return;
				}
				logDiagnostic("info", "updater", "update_available", {
					version: available.value.version,
					elapsedMs: Date.now() - startedAt,
				});
				setUpdate(available.value);
				setIsVisible(true);
			} else if (Result.isOk(available)) {
				logDiagnostic("debug", "updater", "no_update", {
					elapsedMs: Date.now() - startedAt,
				});
				setUpdate(null);
				setIsVisible(false);
			} else {
				logDiagnostic("error", "updater", "check_failed", {
					elapsedMs: Date.now() - startedAt,
					error: errorFields(available.error),
				});
			}
		} finally {
			isCheckingForUpdates = false;
		}
	};

	onMount(() => {
		void checkForUpdates();
		const intervalId = window.setInterval(() => {
			void checkForUpdates();
		}, UPDATE_CHECK_INTERVAL_MS);

		let cleanup: (() => void) | undefined;
		let disposed = false;
		void listen("check-for-updates", () => {
			void checkForUpdates();
		}).then((unlisten) => {
			if (disposed) {
				unlisten();
			} else {
				cleanup = unlisten;
			}
		});

		onCleanup(() => {
			disposed = true;
			window.clearInterval(intervalId);
			cleanup?.();
		});
	});

	const handleDismiss = () => {
		dismissedVersion = update()?.version ?? null;
		logDiagnostic("info", "updater", "update_ignored", { version: dismissedVersion });
		setIsVisible(false);
	};

	const handleDownloadAndRestart = async () => {
		const updateInfo = update();
		if (!updateInfo) return;

		logDiagnostic("info", "updater", "download_and_restart_requested", {
			version: updateInfo.version,
		});
		setIsDownloading(true);
		setDownloadedBytes(0);
		setContentLength(null);
		const installed = await Result.tryPromise({
			try: async () => {
				await updateInfo.downloadAndInstall((event) => {
					if (event.event === "Started") {
						logDiagnostic("info", "updater", "download_started", {
							version: updateInfo.version,
							contentLength: event.data.contentLength ?? null,
						});
						setDownloadedBytes(0);
						setContentLength(event.data.contentLength ?? null);
					} else if (event.event === "Progress") {
						setDownloadedBytes((previous) => previous + event.data.chunkLength);
					} else if (event.event === "Finished") {
						logDiagnostic("info", "updater", "download_finished", {
							version: updateInfo.version,
							downloadedBytes: downloadedBytes(),
						});
						const total = contentLength();
						if (total) setDownloadedBytes(total);
					}
				});
				logDiagnostic("warn", "updater", "relaunch_requested", {
					version: updateInfo.version,
				});
				await relaunch();
			},
			catch: (cause) => cause,
		});
		if (Result.isError(installed)) {
			logDiagnostic("error", "updater", "download_or_install_failed", {
				version: updateInfo.version,
				error: errorFields(installed.error),
			});
			setIsDownloading(false);
		}
	};

	return (
		<Show when={isVisible() && update()}>
			<div class="border-t border-border">
				<div class="px-6 py-3">
					<div class="mb-1.5 flex items-center justify-between gap-2">
						<span class="font-mono uppercase tracking-wider text-[10px] text-ac flex items-center gap-1.5">
							<Download class="h-2.5 w-2.5" />
							{t("update.available")}
						</span>
						<Show when={!isDownloading()}>
							<button
								type="button"
								onClick={handleDismiss}
								class="font-mono uppercase tracking-wider text-[10px] text-txt-muted hover:text-txt-primary transition-colors"
							>
								{t("update.ignore")}
							</button>
						</Show>
					</div>

					<div class="font-mono text-xs tabular-nums mb-2">
						<span class="text-txt-primary">
							{t("update.newVersion")} {update()?.version}
						</span>
					</div>

					<Show when={update()?.body}>
						<div class="mb-2 max-h-24 overflow-y-auto border border-border bg-th-surface p-2">
							<p class="font-mono text-[10px] leading-relaxed text-txt-secondary whitespace-pre-wrap">
								{update()?.body}
							</p>
						</div>
					</Show>

					<Show
						when={!isDownloading()}
						fallback={
							<>
								<div class="w-full h-1 bg-border overflow-hidden">
									<div
										class="h-full bg-ac transition-all duration-300"
										style={{ width: `${downloadPercent()}%` }}
									/>
								</div>
								<p class="mt-1.5 font-mono uppercase tracking-wider text-[10px] text-txt-muted">
									{t("update.downloading")}
								</p>
								<Show when={contentLength()}>
									<p class="mt-0.5 font-mono text-[10px] text-txt-muted tabular-nums">
										{formatMb(downloadedBytes())} / {formatMb(contentLength() ?? 0)} MB (
										{Math.round(downloadPercent())}%)
									</p>
								</Show>
							</>
						}
					>
						<button
							type="button"
							onClick={handleDownloadAndRestart}
							class="w-full flex items-center justify-center gap-1.5 px-2 py-1.5 font-mono uppercase tracking-wider text-[10px] border border-ac text-ac hover:bg-ac hover:text-ac-on transition-colors"
						>
							<span aria-hidden="true">→</span>
							{t("update.downloadAndRestart")}
						</button>
					</Show>
				</div>
			</div>
		</Show>
	);
}
