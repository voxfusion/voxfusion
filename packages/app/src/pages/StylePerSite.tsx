import { Result } from "better-result";
import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import AddSiteForm from "../components/AddSiteForm";
import SiteIcon from "../components/SiteIcon";
import StyleSelect from "../components/StyleSelect";
import { useI18n } from "../i18n";
import {
	type AppStyle,
	type SiteStyle,
	deleteSiteStyle,
	listSiteStyles,
	setSiteStyle,
} from "../lib/commands/apps";
import { preloadFavicons } from "../lib/favicons";
import { capture } from "../lib/posthog";

export default function StylePerSite() {
	const [t] = useI18n();
	const [sites, setSites] = createSignal<SiteStyle[]>([]);
	const [loading, setLoading] = createSignal(true);

	const fetchSites = async () => {
		const result = await listSiteStyles();
		if (Result.isOk(result)) {
			setSites(result.value);
			preloadFavicons(result.value.map((s) => s.domain));
		}
	};

	onMount(async () => {
		capture("$pageview", { $current_url: "/style/sites" });
		setLoading(true);
		await fetchSites();
		setLoading(false);
	});

	const configuredDomains = createMemo(() => new Set(sites().map((s) => s.domain)));

	const addSite = async (domain: string) => {
		if (configuredDomains().has(domain)) return;
		const result = await setSiteStyle(domain, "default");
		if (Result.isError(result)) return;
		capture("site_style_added", { style: "default" });
		await fetchSites();
	};

	const handleChangeStyle = async (site: SiteStyle, style: AppStyle) => {
		setSites(sites().map((s) => (s.id === site.id ? { ...s, style } : s)));
		const result = await setSiteStyle(site.domain, style);
		if (Result.isOk(result)) capture("site_style_changed", { style });
		else await fetchSites();
	};

	const handleDelete = async (id: string) => {
		capture("site_style_deleted");
		setSites(sites().filter((s) => s.id !== id));
		const result = await deleteSiteStyle(id);
		if (Result.isError(result)) await fetchSites();
	};

	const styleLabel = (style: AppStyle) => {
		switch (style) {
			case "professional":
				return t("appInstructions.styles.professional");
			case "casual":
				return t("appInstructions.styles.casual");
			case "agents":
				return t("appInstructions.styles.agents");
			case "default":
				return t("appInstructions.styles.default");
		}
	};

	return (
		<div>
			<div class="mb-4 flex items-center justify-between">
				<p class="text-txt-muted font-mono text-xs">{t("style.perSiteDescription")}</p>
				<Show when={sites().length > 0}>
					<span class="text-txt-muted font-mono text-xs uppercase">
						{t("dictionary.siteCount").replace("{count}", String(sites().length))}
					</span>
				</Show>
			</div>

			<AddSiteForm onAdd={addSite} />

			<Show when={!loading() && sites().length === 0}>
				<div class="bg-th-surface border border-border p-12 flex flex-col items-center justify-center text-center">
					<div class="font-mono text-txt-muted text-sm mb-4">
						<span class="text-ac">[INFO]</span> NO_SITES_CONFIGURED
					</div>
					<p class="text-txt-secondary font-mono text-xs uppercase tracking-wide">
						{t("style.perSiteEmptyState")}
					</p>
					<p class="text-txt-muted font-mono text-xs mt-2 max-w-sm">
						{t("style.perSiteEmptyStateDescription")}
					</p>
				</div>
			</Show>

			<Show when={!loading() && sites().length > 0}>
				<div class="space-y-1">
					<For each={sites()}>
						{(site) => (
							<div class="bg-th-surface border border-border px-4 py-3 flex items-center justify-between gap-3 group hover:border-border-strong transition-colors">
								<SiteIcon domain={site.domain} sizeClass="w-8 h-8" />
								<div class="flex flex-col min-w-0 flex-1">
									<span class="text-txt-primary font-mono truncate">{site.domain}</span>
								</div>
								<StyleSelect
									value={site.style}
									labelFor={styleLabel}
									onChange={(style) => handleChangeStyle(site, style)}
								/>
								<button
									type="button"
									onClick={() => handleDelete(site.id)}
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
