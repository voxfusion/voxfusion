import { A, useLocation } from "@solidjs/router";
import type { ParentComponent } from "solid-js";
import { useI18n } from "../i18n";

const Style: ParentComponent = (props) => {
	const [t] = useI18n();
	const location = useLocation();

	const isPerApp = () => location.pathname.startsWith("/style/per-app");
	const isSites = () => location.pathname.startsWith("/style/sites");
	const isDefault = () => !isPerApp() && !isSites();

	const tabClass = (active: boolean) =>
		`font-mono text-xs uppercase tracking-wider transition-colors py-2 ${
			active ? "text-ac" : "text-txt-secondary hover:text-txt-primary"
		}`;

	return (
		<div class="min-h-screen bg-th-base px-6 py-8">
			<div class="max-w-2xl mx-auto">
				<div class="mb-6">
					<h1 class="font-mono text-txt-primary text-sm">
						<span class="text-ac">[STYLE]</span>
						<span class="text-txt-muted"> {">"} </span>
						<span class="text-txt-secondary">TRANSCRIPTION_STYLE</span>
					</h1>
				</div>

				<div class="flex items-center gap-4 border-b border-border mb-6">
					<A href="/style" end class={tabClass(isDefault())}>
						[{t("style.tabDefault")}]
					</A>
					<A href="/style/per-app" class={tabClass(isPerApp())}>
						[{t("style.tabPerApp")}]
					</A>
					<A href="/style/sites" class={tabClass(isSites())}>
						[{t("style.tabSites")}]
					</A>
				</div>

				{props.children}
			</div>
		</div>
	);
};

export default Style;
