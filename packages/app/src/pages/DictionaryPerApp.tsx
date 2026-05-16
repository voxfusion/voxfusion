import { ChevronDown, Search } from "lucide-solid";
import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { useI18n } from "../i18n";
import { type InstalledApp, listInstalledApps } from "../lib/commands/apps";
import {
	type AppDictionary,
	addAppDictionaryWord,
	deleteAppDictionary,
	deleteAppDictionaryWord,
	listAppDictionaries,
	updateAppDictionaryWord,
} from "../lib/commands/dictionary";
import { capture } from "../lib/posthog";

const SKELETON_ROWS = Array.from({ length: 4 });

export default function DictionaryPerApp() {
	const [t] = useI18n();
	const [installedApps, setInstalledApps] = createSignal<InstalledApp[]>([]);
	const [appDicts, setAppDicts] = createSignal<AppDictionary[]>([]);
	const [pendingApps, setPendingApps] = createSignal<AppDictionary[]>([]);
	const [appsLoading, setAppsLoading] = createSignal(true);

	const [searchQuery, setSearchQuery] = createSignal("");
	const [searchOpen, setSearchOpen] = createSignal(false);
	const [highlightedIndex, setHighlightedIndex] = createSignal(0);

	const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
	const [newWordByApp, setNewWordByApp] = createSignal<Record<string, string>>({});
	const [addingApp, setAddingApp] = createSignal<Set<string>>(new Set());
	const [editingAppWordId, setEditingAppWordId] = createSignal<string | null>(null);
	const [editingAppWord, setEditingAppWord] = createSignal("");

	let searchContainerRef: HTMLDivElement | undefined;
	const optionRefs = new Map<number, HTMLButtonElement>();

	const fetchAppDicts = async () => {
		const result = await listAppDictionaries();
		setAppDicts(result);
	};

	onMount(async () => {
		capture("$pageview", { $current_url: "/dictionary/per-app" });
		setAppsLoading(true);
		try {
			const apps = await listInstalledApps();
			setInstalledApps(apps);
		} catch (err) {
			console.error("Failed to load installed apps", err);
		}
		try {
			const dicts = await listAppDictionaries();
			setAppDicts(dicts);
		} catch (err) {
			console.error("Failed to load app dictionaries", err);
		}
		setAppsLoading(false);
	});

	const allAppGroups = createMemo(() => {
		const persisted = appDicts();
		const persistedIds = new Set(persisted.map((d) => d.bundle_id));
		const pending = pendingApps().filter((p) => !persistedIds.has(p.bundle_id));
		return [...persisted, ...pending];
	});

	const configuredBundleIds = createMemo(
		() => new Set(allAppGroups().map((d) => d.bundle_id))
	);

	const iconByBundleId = createMemo(() => {
		const map = new Map<string, string>();
		for (const app of installedApps()) {
			if (app.icon_data_url) map.set(app.bundle_id, app.icon_data_url);
		}
		return map;
	});

	const filteredApps = createMemo(() => {
		const query = searchQuery().trim().toLowerCase();
		const configured = configuredBundleIds();
		return installedApps()
			.filter((app) => !configured.has(app.bundle_id))
			.filter((app) => {
				if (!query) return true;
				return (
					app.name.toLowerCase().includes(query) ||
					app.bundle_id.toLowerCase().includes(query)
				);
			})
			.slice(0, 50);
	});

	createEffect(() => {
		filteredApps();
		searchQuery();
		setHighlightedIndex(0);
	});

	createEffect(() => {
		if (!searchOpen()) return;
		const el = optionRefs.get(highlightedIndex());
		el?.scrollIntoView({ block: "nearest" });
	});

	const handleClickOutside = (e: MouseEvent) => {
		if (searchContainerRef && !searchContainerRef.contains(e.target as Node)) {
			setSearchOpen(false);
		}
	};

	createEffect(() => {
		if (searchOpen()) {
			document.addEventListener("click", handleClickOutside);
		} else {
			document.removeEventListener("click", handleClickOutside);
		}
	});

	onCleanup(() => {
		document.removeEventListener("click", handleClickOutside);
	});

	const handleSearchKeyDown = (e: KeyboardEvent) => {
		const items = filteredApps();
		if (e.key === "ArrowDown") {
			e.preventDefault();
			setSearchOpen(true);
			if (items.length === 0) return;
			setHighlightedIndex((i) => Math.min(items.length - 1, i + 1));
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			if (items.length === 0) return;
			setHighlightedIndex((i) => Math.max(0, i - 1));
		} else if (e.key === "Enter") {
			if (!searchOpen() || items.length === 0) return;
			e.preventDefault();
			const idx = Math.min(highlightedIndex(), items.length - 1);
			const target = items[idx];
			if (target) handleAddApp(target);
		} else if (e.key === "Escape") {
			if (!searchOpen()) return;
			e.preventDefault();
			setSearchOpen(false);
		}
	};

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

	const handleAddAppWord = async (group: AppDictionary) => {
		const word = (newWordByApp()[group.bundle_id] ?? "").trim();
		if (!word) return;
		if (addingApp().has(group.bundle_id)) return;

		setAddingApp((prev) => {
			const next = new Set(prev);
			next.add(group.bundle_id);
			return next;
		});
		try {
			await addAppDictionaryWord(group.bundle_id, group.app_name, word);
			capture("app_dictionary_word_added");
			setNewWordByApp((prev) => ({ ...prev, [group.bundle_id]: "" }));
			setPendingApps((prev) => prev.filter((p) => p.bundle_id !== group.bundle_id));
			await fetchAppDicts();
		} finally {
			setAddingApp((prev) => {
				const next = new Set(prev);
				next.delete(group.bundle_id);
				return next;
			});
		}
	};

	const handleAppWordKeyDown = (e: KeyboardEvent, group: AppDictionary) => {
		if (e.key === "Enter") handleAddAppWord(group);
	};

	const startEditAppWord = (wordId: string, word: string) => {
		setEditingAppWordId(wordId);
		setEditingAppWord(word);
	};

	const cancelEditAppWord = () => {
		setEditingAppWordId(null);
		setEditingAppWord("");
	};

	const handleEditAppWord = async (group: AppDictionary, wordId: string) => {
		const word = editingAppWord().trim();
		if (!word) return;
		capture("app_dictionary_word_edited");
		setAppDicts(
			appDicts().map((g) =>
				g.bundle_id === group.bundle_id
					? { ...g, words: g.words.map((w) => (w.id === wordId ? { ...w, word } : w)) }
					: g
			)
		);
		setEditingAppWordId(null);
		setEditingAppWord("");
		await updateAppDictionaryWord(wordId, word);
	};

	const handleEditAppWordKeyDown = (
		e: KeyboardEvent,
		group: AppDictionary,
		wordId: string,
	) => {
		if (e.key === "Enter") handleEditAppWord(group, wordId);
		else if (e.key === "Escape") cancelEditAppWord();
	};

	const handleDeleteAppWord = async (group: AppDictionary, wordId: string) => {
		capture("app_dictionary_word_deleted");
		setAppDicts(
			appDicts().map((g) =>
				g.bundle_id === group.bundle_id
					? { ...g, words: g.words.filter((w) => w.id !== wordId) }
					: g
			)
		);
		await deleteAppDictionaryWord(wordId);
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
			await deleteAppDictionary(group.bundle_id);
		}
	};

	return (
		<div>
			<div class="mb-4 flex items-center justify-between">
				<p class="text-txt-muted font-mono text-xs">
					{t("dictionary.perAppDescription")}
				</p>
				<Show when={allAppGroups().length > 0}>
					<span class="text-txt-muted font-mono text-xs uppercase">
						{t("appInstructions.appCount").replace(
							"{count}",
							String(allAppGroups().length)
						)}
					</span>
				</Show>
			</div>

			<div ref={searchContainerRef} class="relative mb-6">
				<div class="bg-th-surface border border-border p-4">
					<div class="flex items-center gap-3">
						<Search class="w-4 h-4 text-txt-muted shrink-0" />
						<input
							type="text"
							value={searchQuery()}
							onInput={(e) => {
								setSearchQuery(e.currentTarget.value);
								setSearchOpen(true);
							}}
							onFocus={() => setSearchOpen(true)}
							onKeyDown={handleSearchKeyDown}
							placeholder={t("appInstructions.searchPlaceholder")}
							class="flex-1 bg-transparent text-txt-primary font-mono placeholder-txt-muted focus:outline-none"
						/>
					</div>
				</div>

				<Show when={searchOpen()}>
					<div class="absolute z-50 w-full mt-1 bg-th-surface border border-border-strong max-h-72 overflow-auto">
						<Show
							when={!appsLoading()}
							fallback={<For each={SKELETON_ROWS}>{() => <AppRowSkeleton />}</For>}
						>
							<Show
								when={filteredApps().length > 0}
								fallback={
									<div class="px-4 py-3 font-mono text-xs text-txt-muted uppercase tracking-wide">
										{installedApps().length === 0
											? t("appInstructions.noAppsDetected")
											: t("appInstructions.noMatches")}
									</div>
								}
							>
								<For each={filteredApps()}>
									{(app, index) => (
										<button
											type="button"
											ref={(el) => optionRefs.set(index(), el)}
											onClick={() => handleAddApp(app)}
											onMouseEnter={() => setHighlightedIndex(index())}
											class={`w-full px-4 py-2.5 text-left transition-colors font-mono text-sm flex items-center gap-3 ${
												highlightedIndex() === index() ? "bg-th-hover" : ""
											}`}
										>
											<AppIcon src={app.icon_data_url} alt={app.name} />
											<div class="flex flex-col gap-0.5 min-w-0 flex-1">
												<span class="text-txt-primary truncate">{app.name}</span>
												<span class="text-txt-muted text-xs truncate">
													{app.bundle_id}
												</span>
											</div>
										</button>
									)}
								</For>
							</Show>
						</Show>
					</div>
				</Show>
			</div>

			<Show when={appsLoading()}>
				<div class="space-y-1">
					<For each={SKELETON_ROWS}>{() => <AppRowSkeleton bordered />}</For>
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
				<div class="space-y-1">
					<For each={allAppGroups()}>
						{(group) => {
							const isOpen = () => expanded().has(group.bundle_id);
							const wordValue = () => newWordByApp()[group.bundle_id] ?? "";
							const isAdding = () => addingApp().has(group.bundle_id);
							return (
								<div class="bg-th-surface border border-border group hover:border-border-strong transition-colors">
									<div class="flex items-stretch">
										<button
											type="button"
											onClick={() => toggleExpanded(group.bundle_id)}
											class="flex-1 px-4 py-3 flex items-center gap-3 text-left min-w-0"
											title={
												isOpen() ? t("dictionary.collapse") : t("dictionary.expand")
											}
										>
											<AppIcon
												src={iconByBundleId().get(group.bundle_id) ?? null}
												alt={group.app_name}
											/>
											<div class="flex flex-col min-w-0 flex-1">
												<span class="text-txt-primary font-mono truncate">
													{group.app_name}
												</span>
												<span class="text-txt-muted font-mono text-xs truncate">
													{group.bundle_id}
												</span>
											</div>
											<span class="text-txt-muted font-mono text-xs uppercase tracking-wider shrink-0">
												{t("dictionary.wordCount").replace(
													"{count}",
													String(group.words.length)
												)}
											</span>
											<ChevronDown
												class={`w-4 h-4 text-txt-muted shrink-0 transition-transform ${
													isOpen() ? "rotate-180" : ""
												}`}
											/>
										</button>
										<button
											type="button"
											onClick={() => handleRemoveApp(group)}
											class="px-3 flex items-center text-txt-muted hover:text-ac opacity-0 group-hover:opacity-100 transition-all font-mono text-xs uppercase tracking-wider shrink-0"
											title={t("appInstructions.delete")}
										>
											[DEL]
										</button>
									</div>
									<Show when={isOpen()}>
										<div class="border-t border-border px-4 py-3 space-y-2">
											<div class="flex gap-2">
												<input
													type="text"
													value={wordValue()}
													onInput={(e) =>
														setNewWordByApp((prev) => ({
															...prev,
															[group.bundle_id]: e.currentTarget.value,
														}))
													}
													onKeyDown={(e) => handleAppWordKeyDown(e, group)}
													placeholder={t("dictionary.wordPlaceholder")}
													class="flex-1 px-3 py-1.5 bg-th-input border border-border-strong text-txt-primary font-mono text-sm placeholder-txt-muted focus:outline-none focus:border-ac transition-colors"
												/>
												<button
													type="button"
													onClick={() => handleAddAppWord(group)}
													disabled={!wordValue().trim() || isAdding()}
													class="flex items-center gap-1 px-3 py-1.5 bg-ac text-ac-on font-mono uppercase tracking-wider text-xs hover:bg-ac-hover disabled:opacity-50 transition-colors"
												>
													<span>+</span>
													{t("dictionary.addWord")}
												</button>
											</div>

											<Show when={group.words.length > 0}>
												<div class="space-y-1 pt-1">
													<For each={group.words}>
														{(word) => (
															<div class="px-3 py-2 bg-th-input border border-border flex items-center justify-between gap-2 group/word">
																<Show
																	when={editingAppWordId() === word.id}
																	fallback={
																		<span class="text-txt-primary font-mono text-sm flex-1 min-w-0 truncate">
																			{word.word}
																		</span>
																	}
																>
																	<input
																		type="text"
																		value={editingAppWord()}
																		onInput={(e) =>
																			setEditingAppWord(e.currentTarget.value)
																		}
																		onKeyDown={(e) =>
																			handleEditAppWordKeyDown(e, group, word.id)
																		}
																		class="flex-1 px-2 py-1 bg-th-base border border-border-strong text-txt-primary font-mono text-sm focus:outline-none focus:border-ac transition-colors"
																		autofocus
																	/>
																</Show>
																<div class="flex items-center gap-2 font-mono text-xs shrink-0">
																	<Show
																		when={editingAppWordId() === word.id}
																		fallback={
																			<>
																				<button
																					type="button"
																					onClick={() =>
																						startEditAppWord(word.id, word.word)
																					}
																					class="text-txt-muted hover:text-ac opacity-0 group-hover/word:opacity-100 transition-all uppercase tracking-wider"
																					title={t("dictionary.edit")}
																				>
																					[EDIT]
																				</button>
																				<button
																					type="button"
																					onClick={() => handleDeleteAppWord(group, word.id)}
																					class="text-txt-muted hover:text-ac opacity-0 group-hover/word:opacity-100 transition-all uppercase tracking-wider"
																					title={t("dictionary.delete")}
																				>
																					[DEL]
																				</button>
																			</>
																		}
																	>
																		<button
																			type="button"
																			onClick={() => handleEditAppWord(group, word.id)}
																			class="text-success hover:opacity-80 uppercase tracking-wider"
																			title={t("dictionary.save")}
																		>
																			[SAVE]
																		</button>
																		<button
																			type="button"
																			onClick={cancelEditAppWord}
																			class="text-txt-muted hover:text-txt-secondary uppercase tracking-wider"
																			title={t("dictionary.cancel")}
																		>
																			[CANCEL]
																		</button>
																	</Show>
																</div>
															</div>
														)}
													</For>
												</div>
											</Show>
										</div>
									</Show>
								</div>
							);
						}}
					</For>
				</div>
			</Show>
		</div>
	);
}

interface AppRowSkeletonProps {
	bordered?: boolean;
}

function AppRowSkeleton(props: AppRowSkeletonProps) {
	return (
		<div
			class={`px-4 py-2.5 flex items-center gap-3 animate-pulse ${
				props.bordered ? "bg-th-surface border border-border" : ""
			}`}
		>
			<div class="w-8 h-8 shrink-0 bg-th-input" />
			<div class="flex flex-col gap-1.5 min-w-0 flex-1">
				<div class="h-3 w-32 bg-th-input" />
				<div class="h-2.5 w-48 bg-th-input opacity-60" />
			</div>
			<Show when={props.bordered}>
				<div class="w-20 h-4 bg-th-input shrink-0" />
			</Show>
		</div>
	);
}

interface AppIconProps {
	src: string | null;
	alt: string;
}

function AppIcon(props: AppIconProps) {
	return (
		<Show
			when={props.src}
			fallback={
				<div class="w-8 h-8 shrink-0 bg-th-input border border-border flex items-center justify-center text-txt-muted font-mono text-xs">
					{props.alt.charAt(0).toUpperCase()}
				</div>
			}
		>
			{(src) => (
				<img src={src()} alt={props.alt} class="w-8 h-8 shrink-0" draggable={false} />
			)}
		</Show>
	);
}
