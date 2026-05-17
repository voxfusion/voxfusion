import { A, useLocation } from "@solidjs/router";
import type { ParentComponent } from "solid-js";
import { useI18n } from "../i18n";

const Dictionary: ParentComponent = (props) => {
	const [t] = useI18n();
	const location = useLocation();

	const isPerApp = () => location.pathname.startsWith("/dictionary/per-app");
	const isSites = () => location.pathname.startsWith("/dictionary/sites");
	const isDefault = () => !isPerApp() && !isSites();

	const tabClass = (active: boolean) =>
		`font-mono text-xs uppercase tracking-wider transition-colors py-2 ${
			active
				? "text-ac"
				: "text-txt-secondary hover:text-txt-primary"
		}`;

	return (
		<div class="min-h-screen bg-th-base px-6 py-8">
			<div class="max-w-2xl mx-auto">
				<div class="mb-6">
					<h1 class="font-mono text-txt-primary text-sm">
						<span class="text-ac">[DICTIONARY]</span>
						<span class="text-txt-muted"> {">"} </span>
						<span class="text-txt-secondary">CUSTOM_TERMS</span>
					</h1>
					<p class="text-txt-muted font-mono text-xs mt-2 uppercase tracking-wide">
						{t("dictionary.description")}
					</p>
				</div>

				<div class="flex items-center gap-4 border-b border-border mb-6">
					<A href="/dictionary" end class={tabClass(isDefault())}>
						[{t("dictionary.tabDefault")}]
					</A>
					<A href="/dictionary/per-app" class={tabClass(isPerApp())}>
						[{t("dictionary.tabPerApp")}]
					</A>
					<A href="/dictionary/sites" class={tabClass(isSites())}>
						[{t("dictionary.tabSites")}]
					</A>
				</div>

				{props.children}
			</div>
		</div>
	);
};

export default Dictionary;
