import { Result } from "better-result";
import { ChevronDown } from "lucide-solid";
import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import AddSiteForm from "../components/AddSiteForm";
import SiteIcon from "../components/SiteIcon";
import { useI18n } from "../i18n";
import {
	type SiteDictionary,
	addSiteDictionaryWord,
	deleteSiteDictionary,
	deleteSiteDictionaryWord,
	listSiteDictionaries,
	updateSiteDictionaryWord,
} from "../lib/commands/dictionary";
import { preloadFavicons } from "../lib/favicons";
import { capture } from "../lib/posthog";

export default function DictionarySites() {
	const [t] = useI18n();
	const [siteDicts, setSiteDicts] = createSignal<SiteDictionary[]>([]);
	const [pendingSites, setPendingSites] = createSignal<SiteDictionary[]>([]);
	const [loading, setLoading] = createSignal(true);

	const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
	const [newWordBySite, setNewWordBySite] = createSignal<Record<string, string>>({});
	const [addingSite, setAddingSite] = createSignal<Set<string>>(new Set());
	const [editingWordId, setEditingWordId] = createSignal<string | null>(null);
	const [editingWord, setEditingWord] = createSignal("");

	const fetchSiteDicts = async () => {
		const result = await listSiteDictionaries();
		if (Result.isOk(result)) {
			setSiteDicts(result.value);
			preloadFavicons(result.value.map((g) => g.domain));
		}
	};

	onMount(async () => {
		capture("$pageview", { $current_url: "/dictionary/sites" });
		setLoading(true);
		await fetchSiteDicts();
		setLoading(false);
	});

	const allSiteGroups = createMemo(() => {
		const persisted = siteDicts();
		const persistedDomains = new Set(persisted.map((d) => d.domain));
		const pending = pendingSites().filter((p) => !persistedDomains.has(p.domain));
		return [...persisted, ...pending];
	});

	const configuredDomains = createMemo(() => new Set(allSiteGroups().map((g) => g.domain)));

	const addSite = (domain: string) => {
		if (configuredDomains().has(domain)) {
			setExpanded((prev) => {
				const next = new Set(prev);
				next.add(domain);
				return next;
			});
			return;
		}
		setPendingSites((prev) => [...prev, { domain, words: [] }]);
		setExpanded((prev) => {
			const next = new Set(prev);
			next.add(domain);
			return next;
		});
		capture("site_dictionary_site_added");
	};

	const toggleExpanded = (domain: string) => {
		setExpanded((prev) => {
			const next = new Set(prev);
			if (next.has(domain)) next.delete(domain);
			else next.add(domain);
			return next;
		});
	};

	const handleAddWord = async (group: SiteDictionary) => {
		const word = (newWordBySite()[group.domain] ?? "").trim();
		if (!word) return;
		if (addingSite().has(group.domain)) return;

		setAddingSite((prev) => {
			const next = new Set(prev);
			next.add(group.domain);
			return next;
		});
		const result = await addSiteDictionaryWord(group.domain, word);
		if (Result.isOk(result)) {
			capture("site_dictionary_word_added");
			setNewWordBySite((prev) => ({ ...prev, [group.domain]: "" }));
			setPendingSites((prev) => prev.filter((p) => p.domain !== group.domain));
			await fetchSiteDicts();
		}
		setAddingSite((prev) => {
			const next = new Set(prev);
			next.delete(group.domain);
			return next;
		});
	};

	const handleWordKeyDown = (e: KeyboardEvent, group: SiteDictionary) => {
		if (e.key === "Enter") handleAddWord(group);
	};

	const startEditWord = (wordId: string, word: string) => {
		setEditingWordId(wordId);
		setEditingWord(word);
	};

	const cancelEditWord = () => {
		setEditingWordId(null);
		setEditingWord("");
	};

	const handleEditWord = async (group: SiteDictionary, wordId: string) => {
		const word = editingWord().trim();
		if (!word) return;
		capture("site_dictionary_word_edited");
		setSiteDicts(
			siteDicts().map((g) =>
				g.domain === group.domain
					? { ...g, words: g.words.map((w) => (w.id === wordId ? { ...w, word } : w)) }
					: g
			)
		);
		setEditingWordId(null);
		setEditingWord("");
		const result = await updateSiteDictionaryWord(wordId, word);
		if (Result.isError(result)) await fetchSiteDicts();
	};

	const handleEditWordKeyDown = (e: KeyboardEvent, group: SiteDictionary, wordId: string) => {
		if (e.key === "Enter") handleEditWord(group, wordId);
		else if (e.key === "Escape") cancelEditWord();
	};

	const handleDeleteWord = async (group: SiteDictionary, wordId: string) => {
		capture("site_dictionary_word_deleted");
		setSiteDicts(
			siteDicts().map((g) =>
				g.domain === group.domain
					? { ...g, words: g.words.filter((w) => w.id !== wordId) }
					: g
			)
		);
		const result = await deleteSiteDictionaryWord(wordId);
		if (Result.isError(result)) await fetchSiteDicts();
	};

	const handleRemoveSite = async (group: SiteDictionary) => {
		capture("site_dictionary_site_removed");
		setPendingSites((prev) => prev.filter((p) => p.domain !== group.domain));
		setSiteDicts((prev) => prev.filter((g) => g.domain !== group.domain));
		setExpanded((prev) => {
			const next = new Set(prev);
			next.delete(group.domain);
			return next;
		});
		if (group.words.length > 0) {
			const result = await deleteSiteDictionary(group.domain);
			if (Result.isError(result)) await fetchSiteDicts();
		}
	};

	return (
		<div>
			<div class="mb-4 flex items-center justify-between">
				<p class="text-txt-muted font-mono text-xs">{t("dictionary.sitesDescription")}</p>
				<Show when={allSiteGroups().length > 0}>
					<span class="text-txt-muted font-mono text-xs uppercase">
						{t("dictionary.siteCount").replace("{count}", String(allSiteGroups().length))}
					</span>
				</Show>
			</div>

			<AddSiteForm onAdd={addSite} />

			<Show when={!loading() && allSiteGroups().length === 0}>
				<div class="bg-th-surface border border-border p-12 flex flex-col items-center justify-center text-center">
					<div class="font-mono text-txt-muted text-sm mb-4">
						<span class="text-ac">[INFO]</span> NO_SITES_CONFIGURED
					</div>
					<p class="text-txt-secondary font-mono text-xs uppercase tracking-wide">
						{t("dictionary.sitesEmptyState")}
					</p>
					<p class="text-txt-muted font-mono text-xs mt-2 max-w-sm">
						{t("dictionary.sitesEmptyStateDescription")}
					</p>
				</div>
			</Show>

			<Show when={!loading() && allSiteGroups().length > 0}>
				<div class="space-y-1">
					<For each={allSiteGroups()}>
						{(group) => {
							const isOpen = () => expanded().has(group.domain);
							const wordValue = () => newWordBySite()[group.domain] ?? "";
							const isAdding = () => addingSite().has(group.domain);
							return (
								<div class="bg-th-surface border border-border group hover:border-border-strong transition-colors">
									<div class="flex items-stretch">
										<button
											type="button"
											onClick={() => toggleExpanded(group.domain)}
											class="flex-1 px-4 py-3 flex items-center gap-3 text-left min-w-0"
											title={isOpen() ? t("dictionary.collapse") : t("dictionary.expand")}
										>
											<SiteIcon domain={group.domain} sizeClass="w-8 h-8" />
											<div class="flex flex-col min-w-0 flex-1">
												<span class="text-txt-primary font-mono truncate">{group.domain}</span>
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
											onClick={() => handleRemoveSite(group)}
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
														setNewWordBySite((prev) => ({
															...prev,
															[group.domain]: e.currentTarget.value,
														}))
													}
													onKeyDown={(e) => handleWordKeyDown(e, group)}
													placeholder={t("dictionary.wordPlaceholder")}
													class="flex-1 px-3 py-1.5 bg-th-input border border-border-strong text-txt-primary font-mono text-sm placeholder-txt-muted focus:outline-none focus:border-ac transition-colors"
												/>
												<button
													type="button"
													onClick={() => handleAddWord(group)}
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
																	when={editingWordId() === word.id}
																	fallback={
																		<span class="text-txt-primary font-mono text-sm flex-1 min-w-0 truncate">
																			{word.word}
																		</span>
																	}
																>
																	<input
																		type="text"
																		value={editingWord()}
																		onInput={(e) => setEditingWord(e.currentTarget.value)}
																		onKeyDown={(e) =>
																			handleEditWordKeyDown(e, group, word.id)
																		}
																		class="flex-1 px-2 py-1 bg-th-base border border-border-strong text-txt-primary font-mono text-sm focus:outline-none focus:border-ac transition-colors"
																		autofocus
																	/>
																</Show>
																<div class="flex items-center gap-2 font-mono text-xs shrink-0">
																	<Show
																		when={editingWordId() === word.id}
																		fallback={
																			<>
																				<button
																					type="button"
																					onClick={() => startEditWord(word.id, word.word)}
																					class="text-txt-muted hover:text-ac opacity-0 group-hover/word:opacity-100 transition-all uppercase tracking-wider"
																					title={t("dictionary.edit")}
																				>
																					[EDIT]
																				</button>
																				<button
																					type="button"
																					onClick={() => handleDeleteWord(group, word.id)}
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
																			onClick={() => handleEditWord(group, word.id)}
																			class="text-success hover:opacity-80 uppercase tracking-wider"
																			title={t("dictionary.save")}
																		>
																			[SAVE]
																		</button>
																		<button
																			type="button"
																			onClick={cancelEditWord}
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
