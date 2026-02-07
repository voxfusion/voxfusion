import { relaunch } from "@tauri-apps/plugin-process";
import { type Update, check } from "@tauri-apps/plugin-updater";
import { Download } from "lucide-solid";
import { Show, createSignal, onMount } from "solid-js";
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
				<div class="bg-th-surface border border-border overflow-hidden">
					<div class="p-4">
						<div class="flex items-start gap-3">
							<div class="flex-shrink-0 w-10 h-10 bg-ac-bg border border-ac flex items-center justify-center">
								<Download class="h-5 w-5 text-ac" />
							</div>
							<div class="flex-1 min-w-0">
								<div class="flex items-start justify-between gap-2">
									<h3 class="text-sm font-mono uppercase tracking-wider text-txt-primary">
										{t("update.available")}
									</h3>
									<button
										type="button"
										onClick={handleIgnore}
										disabled={isDownloading()}
										class="font-mono text-txt-muted hover:text-ac transition-colors disabled:opacity-50"
									>
										[X]
									</button>
								</div>
								<p class="mt-1 text-sm font-mono text-txt-secondary">
									{t("update.newVersion")} {update()?.version}
								</p>
							</div>
						</div>

						<Show when={isDownloading()}>
							<div class="mt-3">
								<div class="w-full h-1.5 bg-border overflow-hidden">
									<div
										class="h-full bg-ac transition-all duration-300"
										style={{ width: `${Math.min(downloadProgress() / 10, 100)}%` }}
									/>
								</div>
								<p class="mt-1.5 text-xs font-mono text-txt-secondary">{t("update.downloading")}</p>
							</div>
						</Show>

						<Show when={!isDownloading()}>
							<div class="mt-3 flex gap-2">
								<button
									type="button"
									onClick={handleIgnore}
									class="flex-1 px-3 py-2 text-sm font-mono bg-th-base border border-border-strong text-txt-secondary hover:border-txt-muted transition-colors"
								>
									{t("update.ignore")}
								</button>
								<button
									type="button"
									onClick={handleDownloadAndRestart}
									class="flex-1 px-3 py-2 text-sm font-mono uppercase bg-ac text-ac-on hover:bg-ac-hover transition-colors"
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
