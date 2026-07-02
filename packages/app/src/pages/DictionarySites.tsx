import { Result } from "better-result";
import { Show, createMemo, createSignal, onMount } from "solid-js";
import AddSiteForm from "../components/AddSiteForm";
import SiteIcon from "../components/SiteIcon";
import WordListEditor from "../components/WordListEditor";
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

	const addWord = (group: SiteDictionary, word: string) =>
		addSiteDictionaryWord(group.domain, word);

	const handleWordAdded = async (group: SiteDictionary) => {
		capture("site_dictionary_word_added");
		setPendingSites((prev) => prev.filter((p) => p.domain !== group.domain));
		await fetchSiteDicts();
	};

	const handleEditWord = async (group: SiteDictionary, wordId: string, word: string) => {
		capture("site_dictionary_word_edited");
		setSiteDicts(
			siteDicts().map((g) =>
				g.domain === group.domain
					? { ...g, words: g.words.map((w) => (w.id === wordId ? { ...w, word } : w)) }
					: g
			)
		);
		const result = await updateSiteDictionaryWord(wordId, word);
		if (Result.isError(result)) await fetchSiteDicts();
	};

	const handleDeleteWord = async (group: SiteDictionary, wordId: string) => {
		capture("site_dictionary_word_deleted");
		setSiteDicts(
			siteDicts().map((g) =>
				g.domain === group.domain ? { ...g, words: g.words.filter((w) => w.id !== wordId) } : g
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
				<WordListEditor
					groups={allSiteGroups()}
					keyOf={(group) => group.domain}
					icon={(group) => <SiteIcon domain={group.domain} sizeClass="w-8 h-8" />}
					title={(group) => group.domain}
					expanded={expanded()}
					onToggle={toggleExpanded}
					onRemoveGroup={handleRemoveSite}
					addWord={addWord}
					onWordAdded={handleWordAdded}
					onEditWord={handleEditWord}
					onDeleteWord={handleDeleteWord}
				/>
			</Show>
		</div>
	);
}
