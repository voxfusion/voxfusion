import { listen } from "@tauri-apps/api/event";
import { Result } from "better-result";
import { AlertCircle, Check, Download, Loader } from "lucide-solid";
import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import type { I18nContextType } from "../../i18n";
import {
	type ModelDownloadProgress,
	type ModelInfo,
	downloadModel,
	listModels,
	setActiveModel,
} from "../../lib/commands/model";
import { capture } from "../../lib/posthog";

interface ModelSettingsProps {
	t: I18nContextType[0];
}

export default function ModelSettings(props: ModelSettingsProps) {
	const [models, setModels] = createSignal<ModelInfo[]>([]);
	const [progress, setProgress] = createSignal<Record<string, number>>({});
	const [downloadingId, setDownloadingId] = createSignal<string | null>(null);
	const [busyId, setBusyId] = createSignal<string | null>(null);
	const [error, setError] = createSignal<string | null>(null);

	const refresh = async () => {
		const result = await listModels();
		if (Result.isOk(result)) {
			setModels(result.value);
		} else {
			setError(result.error.message);
		}
	};

	onMount(async () => {
		await refresh();

		const unlisten = await listen<ModelDownloadProgress>("model-download-progress", (event) => {
			setProgress((prev) => ({ ...prev, [event.payload.model_id]: event.payload.progress }));
			if (event.payload.progress >= 100) {
				setDownloadingId((current) => (current === event.payload.model_id ? null : current));
				void refresh();
			}
		});
		onCleanup(() => unlisten());
	});

	const handleDownload = async (model: ModelInfo) => {
		setError(null);
		setDownloadingId(model.id);
		setProgress((prev) => ({ ...prev, [model.id]: 0 }));
		capture("settings_model_download_started", { model: model.id });

		const result = await downloadModel(model.id);
		setDownloadingId(null);
		if (Result.isError(result)) {
			setError(result.error.message);
		}
		await refresh();
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
											style={{ width: `${progress()[model.id] ?? 0}%` }}
										/>
									</div>
									<div class="flex items-center gap-2 font-mono text-xs text-txt-secondary">
										<Loader class="w-3.5 h-3.5 animate-spin" />
										<span>
											{progress()[model.id] ?? 0}% — {props.t("settings.modelDownloading")}
										</span>
									</div>
								</div>
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

			<Show when={error()}>
				<div class="flex items-center gap-2 font-mono text-xs text-ac">
					<AlertCircle class="w-4 h-4" />
					<span>
						{props.t("settings.modelDownloadFailed")}: {error()}
					</span>
				</div>
			</Show>
		</div>
	);
}
