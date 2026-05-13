import { listen } from "@tauri-apps/api/event";
import { relaunch } from "@tauri-apps/plugin-process";
import { type Update, check } from "@tauri-apps/plugin-updater";
import { Download } from "lucide-solid";
import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { useI18n } from "../i18n";

const UPDATE_CHECK_INTERVAL_MS = 10_000;

export default function UpdateNotification() {
	const [t] = useI18n();
	const [update, setUpdate] = createSignal<Update | null>(null);
	const [isVisible, setIsVisible] = createSignal(false);
	const [isDownloading, setIsDownloading] = createSignal(false);
	const [downloadProgress, setDownloadProgress] = createSignal(0);
	let isCheckingForUpdates = false;

	const checkForUpdates = async () => {
		if (isCheckingForUpdates || isDownloading()) return;

		isCheckingForUpdates = true;
		try {
			const available = await check();
			if (available) {
				setUpdate(available);
				setIsVisible(true);
			} else {
				setUpdate(null);
				setIsVisible(false);
			}
		} catch {
			// Update check failed
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

	const handleDownloadAndRestart = async () => {
		const updateInfo = update();
		if (!updateInfo) return;

		setIsDownloading(true);
		try {
			await updateInfo.downloadAndInstall((event) => {
				if (event.event === "Started" && event.data.contentLength) {
					setDownloadProgress(0);
				} else if (event.event === "Progress") {
					const progress = downloadProgress() + event.data.chunkLength;
					setDownloadProgress(progress);
				} else if (event.event === "Finished") {
					setDownloadProgress(100);
				}
			});
			await relaunch();
		} catch {
			setIsDownloading(false);
		}
	};

	return (
		<Show when={isVisible() && update()}>
			<div class="border-t border-border">
				<div class="px-6 py-3">
					<div class="mb-1.5">
						<span class="font-mono uppercase tracking-wider text-[10px] text-ac flex items-center gap-1.5">
							<Download class="h-2.5 w-2.5" />
							{t("update.available")}
						</span>
					</div>

					<div class="font-mono text-xs tabular-nums mb-2">
						<span class="text-txt-primary">v{update()?.version}</span>
					</div>

					<Show
						when={!isDownloading()}
						fallback={
							<>
								<div class="w-full h-1 bg-border overflow-hidden">
									<div
										class="h-full bg-ac transition-all duration-300"
										style={{ width: `${Math.min(downloadProgress() / 10, 100)}%` }}
									/>
								</div>
								<p class="mt-1.5 font-mono uppercase tracking-wider text-[10px] text-txt-muted">
									{t("update.downloading")}
								</p>
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
