import { Result } from "better-result";
import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import AppIcon from "../components/AppIcon";
import AppSearchCombobox, { AppRowSkeleton, SKELETON_ROWS } from "../components/AppSearchCombobox";
import WordListEditor from "../components/WordListEditor";
import { useI18n } from "../i18n";
import { type InstalledApp, getCachedInstalledApps, listInstalledApps } from "../lib/commands/apps";
import {
	type AppDictionary,
	addAppDictionaryWord,
	deleteAppDictionary,
	deleteAppDictionaryWord,
	listAppDictionaries,
	updateAppDictionaryWord,
} from "../lib/commands/dictionary";
import { capture } from "../lib/posthog";

export default function DictionaryPerApp() {
	const [t] = useI18n();
	const cachedApps = getCachedInstalledApps();
	const [installedApps, setInstalledApps] = createSignal<InstalledApp[]>(cachedApps ?? []);
	const [appDicts, setAppDicts] = createSignal<AppDictionary[]>([]);
	const [pendingApps, setPendingApps] = createSignal<AppDictionary[]>([]);
	const [appsLoading, setAppsLoading] = createSignal(cachedApps === null);

	const [searchQuery, setSearchQuery] = createSignal("");
	const [searchOpen, setSearchOpen] = createSignal(false);

	const [expanded, setExpanded] = createSignal<Set<string>>(new Set());

	const fetchAppDicts = async () => {
		const result = await listAppDictionaries();
		if (Result.isOk(result)) setAppDicts(result.value);
	};

	onMount(async () => {
		capture("$pageview", { $current_url: "/dictionary/per-app" });
		const apps = await listInstalledApps();
		if (Result.isOk(apps)) setInstalledApps(apps.value);
		else console.error("Failed to load installed apps", apps.error);
		const dicts = await listAppDictionaries();
		if (Result.isOk(dicts)) setAppDicts(dicts.value);
		else console.error("Failed to load app dictionaries", dicts.error);
		setAppsLoading(false);
	});

	const allAppGroups = createMemo(() => {
		const persisted = appDicts();
		const persistedIds = new Set(persisted.map((d) => d.bundle_id));
		const pending = pendingApps().filter((p) => !persistedIds.has(p.bundle_id));
		return [...persisted, ...pending];
	});

	const configuredBundleIds = createMemo(() => new Set(allAppGroups().map((d) => d.bundle_id)));

	const iconByBundleId = createMemo(() => {
		const map = new Map<string, string>();
		for (const app of installedApps()) {
			if (app.icon_data_url) map.set(app.bundle_id, app.icon_data_url);
		}
		return map;
	});

	const handleAddApp = (app: InstalledApp) => {
		setPendingApps((prev) => [
			...prev,
			{ bundle_id: app.bundle_id, app_name: app.name, words: [] },
		]);
		setExpanded((prev) => {
			const next = new Set(prev);
			next.add(app.bundle_id);
			return next;
		});
		setSearchQuery("");
		setSearchOpen(false);
		capture("app_dictionary_app_added");
	};

	const toggleExpanded = (bundleId: string) => {
		setExpanded((prev) => {
			const next = new Set(prev);
			if (next.has(bundleId)) next.delete(bundleId);
			else next.add(bundleId);
			return next;
		});
	};

	const addWord = (group: AppDictionary, word: string) =>
		addAppDictionaryWord(group.bundle_id, group.app_name, word);

	const handleWordAdded = async (group: AppDictionary) => {
		capture("app_dictionary_word_added");
		setPendingApps((prev) => prev.filter((p) => p.bundle_id !== group.bundle_id));
		await fetchAppDicts();
	};

	const handleEditWord = async (group: AppDictionary, wordId: string, word: string) => {
		capture("app_dictionary_word_edited");
		setAppDicts(
			appDicts().map((g) =>
				g.bundle_id === group.bundle_id
					? { ...g, words: g.words.map((w) => (w.id === wordId ? { ...w, word } : w)) }
					: g
			)
		);
		const result = await updateAppDictionaryWord(wordId, word);
		if (Result.isError(result)) await fetchAppDicts();
	};

	const handleDeleteWord = async (group: AppDictionary, wordId: string) => {
		capture("app_dictionary_word_deleted");
		setAppDicts(
			appDicts().map((g) =>
				g.bundle_id === group.bundle_id
					? { ...g, words: g.words.filter((w) => w.id !== wordId) }
					: g
			)
		);
		const result = await deleteAppDictionaryWord(wordId);
		if (Result.isError(result)) await fetchAppDicts();
	};

	const handleRemoveApp = async (group: AppDictionary) => {
		capture("app_dictionary_app_removed");
		setPendingApps((prev) => prev.filter((p) => p.bundle_id !== group.bundle_id));
		setAppDicts((prev) => prev.filter((g) => g.bundle_id !== group.bundle_id));
		setExpanded((prev) => {
			const next = new Set(prev);
			next.delete(group.bundle_id);
			return next;
		});
		if (group.words.length > 0) {
			const result = await deleteAppDictionary(group.bundle_id);
			if (Result.isError(result)) await fetchAppDicts();
		}
	};

	return (
		<div>
			<div class="mb-4 flex items-center justify-between">
				<p class="text-txt-muted font-mono text-xs">{t("dictionary.perAppDescription")}</p>
				<Show when={allAppGroups().length > 0}>
					<span class="text-txt-muted font-mono text-xs uppercase">
						{t("appInstructions.appCount").replace("{count}", String(allAppGroups().length))}
					</span>
				</Show>
			</div>

			<AppSearchCombobox
				apps={installedApps()}
				excludedIds={configuredBundleIds()}
				loading={appsLoading()}
				searchQuery={searchQuery()}
				searchOpen={searchOpen()}
				onSearchQueryChange={setSearchQuery}
				onSearchOpenChange={setSearchOpen}
				onSelect={handleAddApp}
			/>

			<Show when={appsLoading()}>
				<div class="space-y-1">
					<For each={SKELETON_ROWS}>
						{() => <AppRowSkeleton bordered trailingClass="w-20 h-4" />}
					</For>
				</div>
			</Show>

			<Show when={!appsLoading() && allAppGroups().length === 0}>
				<div class="bg-th-surface border border-border p-12 flex flex-col items-center justify-center text-center">
					<div class="font-mono text-txt-muted text-sm mb-4">
						<span class="text-ac">[INFO]</span> NO_APPS_CONFIGURED
					</div>
					<p class="text-txt-secondary font-mono text-xs uppercase tracking-wide">
						{t("dictionary.perAppEmptyState")}
					</p>
					<p class="text-txt-muted font-mono text-xs mt-2 max-w-sm">
						{t("dictionary.perAppEmptyStateDescription")}
					</p>
				</div>
			</Show>

			<Show when={!appsLoading() && allAppGroups().length > 0}>
				<WordListEditor
					groups={allAppGroups()}
					keyOf={(group) => group.bundle_id}
					icon={(group) => (
						<AppIcon src={iconByBundleId().get(group.bundle_id) ?? null} alt={group.app_name} />
					)}
					title={(group) => group.app_name}
					subtitle={(group) => group.bundle_id}
					expanded={expanded()}
					onToggle={toggleExpanded}
					onRemoveGroup={handleRemoveApp}
					addWord={addWord}
					onWordAdded={handleWordAdded}
					onEditWord={handleEditWord}
					onDeleteWord={handleDeleteWord}
				/>
			</Show>
		</div>
	);
}
