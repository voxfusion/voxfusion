import { Result } from "better-result";
import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import AppIcon from "../components/AppIcon";
import AppSearchCombobox, { AppRowSkeleton, SKELETON_ROWS } from "../components/AppSearchCombobox";
import StyleSelect from "../components/StyleSelect";
import { useI18n } from "../i18n";
import {
	type AppInstruction,
	type AppStyle,
	type InstalledApp,
	deleteAppInstruction,
	getCachedInstalledApps,
	listAppInstructions,
	listInstalledApps,
	setAppInstruction,
} from "../lib/commands/apps";
import { capture } from "../lib/posthog";
import { makeStyleLabel } from "../lib/styleLabel";

export default function StylePerApp() {
	const [t] = useI18n();
	const cachedApps = getCachedInstalledApps();
	const [installedApps, setInstalledApps] = createSignal<InstalledApp[]>(cachedApps ?? []);
	const [instructions, setInstructions] = createSignal<AppInstruction[]>([]);
	const [searchQuery, setSearchQuery] = createSignal("");
	const [searchOpen, setSearchOpen] = createSignal(false);
	const [loading, setLoading] = createSignal(cachedApps === null);

	const fetchInstructions = async () => {
		const result = await listAppInstructions();
		if (Result.isOk(result)) setInstructions(result.value);
	};

	onMount(async () => {
		capture("$pageview", { $current_url: "/style/per-app" });
		const [apps, instr] = await Promise.all([listInstalledApps(), listAppInstructions()]);
		if (Result.isOk(apps)) setInstalledApps(apps.value);
		if (Result.isOk(instr)) setInstructions(instr.value);
		setLoading(false);
	});

	const configuredBundleIds = createMemo(() => new Set(instructions().map((i) => i.bundle_id)));

	const iconByBundleId = createMemo(() => {
		const map = new Map<string, string>();
		for (const app of installedApps()) {
			if (app.icon_data_url) map.set(app.bundle_id, app.icon_data_url);
		}
		return map;
	});

	const handleAddApp = async (app: InstalledApp) => {
		const result = await setAppInstruction(app.bundle_id, app.name, "default");
		if (Result.isError(result)) return;
		capture("app_instruction_added", { style: "default" });
		setSearchQuery("");
		setSearchOpen(false);
		await fetchInstructions();
	};

	const handleChangeStyle = async (instruction: AppInstruction, style: AppStyle) => {
		setInstructions(instructions().map((i) => (i.id === instruction.id ? { ...i, style } : i)));
		const result = await setAppInstruction(instruction.bundle_id, instruction.app_name, style);
		if (Result.isOk(result)) capture("app_instruction_style_changed", { style });
		else await fetchInstructions();
	};

	const handleDelete = async (id: string) => {
		capture("app_instruction_deleted");
		setInstructions(instructions().filter((i) => i.id !== id));
		const result = await deleteAppInstruction(id);
		if (Result.isError(result)) await fetchInstructions();
	};

	const styleLabel = makeStyleLabel(t);

	return (
		<div>
			<div class="mb-4 flex items-center justify-between">
				<p class="text-txt-muted font-mono text-xs">{t("appInstructions.description")}</p>
				<Show when={instructions().length > 0}>
					<span class="text-txt-muted font-mono text-xs uppercase">
						{t("appInstructions.appCount").replace("{count}", String(instructions().length))}
					</span>
				</Show>
			</div>

			<AppSearchCombobox
				apps={installedApps()}
				excludedIds={configuredBundleIds()}
				loading={loading()}
				searchQuery={searchQuery()}
				searchOpen={searchOpen()}
				onSearchQueryChange={setSearchQuery}
				onSearchOpenChange={setSearchOpen}
				onSelect={handleAddApp}
			/>

			<Show when={loading()}>
				<div class="space-y-1">
					<For each={SKELETON_ROWS}>
						{() => <AppRowSkeleton bordered trailingClass="w-40 h-6" />}
					</For>
				</div>
			</Show>

			<Show when={!loading() && instructions().length === 0}>
				<div class="bg-th-surface border border-border p-12 flex flex-col items-center justify-center text-center">
					<div class="font-mono text-txt-muted text-sm mb-4">
						<span class="text-ac">[INFO]</span> NO_APPS_CONFIGURED
					</div>
					<p class="text-txt-secondary font-mono text-xs uppercase tracking-wide">
						{t("appInstructions.emptyState")}
					</p>
					<p class="text-txt-muted font-mono text-xs mt-2 max-w-sm">
						{t("appInstructions.emptyStateDescription")}
					</p>
				</div>
			</Show>

			<Show when={!loading() && instructions().length > 0}>
				<div class="space-y-1">
					<For each={instructions()}>
						{(instruction) => (
							<div class="bg-th-surface border border-border px-4 py-3 flex items-center justify-between gap-3 group hover:border-border-strong transition-colors">
								<AppIcon
									src={iconByBundleId().get(instruction.bundle_id) ?? null}
									alt={instruction.app_name}
								/>
								<div class="flex flex-col min-w-0 flex-1">
									<span class="text-txt-primary font-mono truncate">{instruction.app_name}</span>
									<span class="text-txt-muted font-mono text-xs truncate">
										{instruction.bundle_id}
									</span>
								</div>
								<StyleSelect
									value={instruction.style}
									labelFor={styleLabel}
									onChange={(style) => handleChangeStyle(instruction, style)}
								/>
								<button
									type="button"
									onClick={() => handleDelete(instruction.id)}
									class="text-txt-muted hover:text-ac opacity-0 group-hover:opacity-100 transition-all font-mono text-xs uppercase tracking-wider"
									title={t("appInstructions.delete")}
								>
									[DEL]
								</button>
							</div>
						)}
					</For>
				</div>
			</Show>
		</div>
	);
}
