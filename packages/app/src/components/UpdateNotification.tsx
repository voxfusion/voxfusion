import { createSignal, onMount, Show } from "solid-js";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { Download, X } from "lucide-solid";
import { useI18n } from "../i18n";

export default function UpdateNotification() {
	const [t] = useI18n();
	const [update, setUpdate] = createSignal<Update | null>(null);
	const [isVisible, setIsVisible] = createSignal(false);
	const [isDownloading, setIsDownloading] = createSignal(false);
	const [downloadProgress, setDownloadProgress] = createSignal(0);

	onMount(async () => {
		try {
			const available = await check();
			if (available) {
				setUpdate(available);
				setIsVisible(true);
			}
		} catch (error) {
			console.error("Failed to check for updates:", error);
		}
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
		} catch (error) {
			console.error("Failed to download and install update:", error);
			setIsDownloading(false);
		}
	};

	const handleIgnore = () => {
		setIsVisible(false);
	};

	return (
		<Show when={isVisible() && update()}>
			<div class="fixed bottom-4 right-4 z-50 max-w-sm">
				<div class="bg-white dark:bg-midnight-800 rounded-xl shadow-2xl border border-slate-200 dark:border-midnight-600 overflow-hidden">
					<div class="p-4">
						<div class="flex items-start gap-3">
							<div class="flex-shrink-0 w-10 h-10 bg-primary-100 dark:bg-primary-900/30 rounded-lg flex items-center justify-center">
								<Download class="h-5 w-5 text-primary-600 dark:text-primary-400" />
							</div>
							<div class="flex-1 min-w-0">
								<div class="flex items-start justify-between gap-2">
									<h3 class="text-sm font-semibold text-slate-900 dark:text-white">
										{t("update.available")}
									</h3>
									<button
										type="button"
										onClick={handleIgnore}
										disabled={isDownloading()}
										class="p-1 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 rounded transition-colors disabled:opacity-50"
									>
										<X class="w-4 h-4" />
									</button>
								</div>
								<p class="mt-1 text-sm text-slate-500 dark:text-slate-400">
									{t("update.newVersion")} {update()?.version}
								</p>
							</div>
						</div>

						<Show when={isDownloading()}>
							<div class="mt-3">
								<div class="w-full h-1.5 bg-slate-200 dark:bg-midnight-700 rounded-full overflow-hidden">
									<div
										class="h-full bg-primary-500 rounded-full transition-all duration-300"
										style={{ width: `${Math.min(downloadProgress() / 10, 100)}%` }}
									/>
								</div>
								<p class="mt-1.5 text-xs text-slate-500 dark:text-slate-400">
									{t("update.downloading")}
								</p>
							</div>
						</Show>

						<Show when={!isDownloading()}>
							<div class="mt-3 flex gap-2">
								<button
									type="button"
									onClick={handleIgnore}
									class="flex-1 px-3 py-2 text-sm font-medium text-slate-700 dark:text-slate-300 bg-slate-100 dark:bg-midnight-700 hover:bg-slate-200 dark:hover:bg-midnight-600 rounded-lg transition-colors"
								>
									{t("update.ignore")}
								</button>
								<button
									type="button"
									onClick={handleDownloadAndRestart}
									class="flex-1 px-3 py-2 text-sm font-medium text-white bg-primary-500 hover:bg-primary-600 rounded-lg transition-colors"
								>
									{t("update.downloadAndRestart")}
								</button>
							</div>
						</Show>
					</div>
				</div>
			</div>
		</Show>
	);
}
