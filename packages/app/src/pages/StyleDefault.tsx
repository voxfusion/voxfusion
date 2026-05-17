import { For, onMount } from "solid-js";
import { useI18n } from "../i18n";
import { type AppStyle, STYLE_LIST } from "../lib/commands/apps";
import { capture } from "../lib/posthog";
import { updateDefaultStyle, useSettings } from "../lib/settingsStore";

export default function StyleDefault() {
	const [t] = useI18n();
	const settings = useSettings();

	onMount(() => {
		capture("$pageview", { $current_url: "/style" });
	});

	const handleSelect = async (style: AppStyle) => {
		await updateDefaultStyle(style);
		capture("default_style_changed", { style });
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

	const styleDescription = (style: AppStyle) => {
		switch (style) {
			case "professional":
				return t("style.descriptions.professional");
			case "casual":
				return t("style.descriptions.casual");
			case "agents":
				return t("style.descriptions.agents");
			case "default":
				return t("style.descriptions.default");
		}
	};

	return (
		<div>
			<div class="mb-4">
				<p class="text-txt-muted font-mono text-xs">{t("style.defaultStyleDescription")}</p>
			</div>

			<div class="bg-th-surface border border-border">
				<div class="flex border-b border-border">
					<For each={STYLE_LIST}>
						{(style) => {
							const isActive = () => settings().defaultStyle === style;
							return (
								<button
									type="button"
									onClick={() => handleSelect(style)}
									class={`flex-1 px-4 py-2.5 font-mono text-xs uppercase tracking-wider transition-colors border-b-2 ${
										isActive()
											? "text-ac border-ac bg-th-hover"
											: "text-txt-secondary border-transparent hover:text-txt-primary hover:bg-th-hover"
									}`}
								>
									{styleLabel(style)}
								</button>
							);
						}}
					</For>
				</div>
				<p class="px-4 py-4 text-txt-secondary font-mono text-xs leading-relaxed">
					{styleDescription(settings().defaultStyle)}
				</p>
			</div>
		</div>
	);
}
