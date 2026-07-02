import { Check, Copy } from "lucide-solid";
import { For, Show } from "solid-js";
import { useI18n } from "../../i18n";
import type { SettingsSection } from "./types";

type SidebarLabelKey =
	| "settings.audio"
	| "settings.models"
	| "settings.hotkeys"
	| "settings.appearance"
	| "settings.language"
	| "sidebar.privacy";

interface SidebarItem {
	id: SettingsSection;
	num: string;
	labelKey: SidebarLabelKey;
}

interface SettingsSidebarProps {
	activeSection: SettingsSection;
	appVersion: string;
	versionCopied: boolean;
	onSectionChange: (section: SettingsSection) => void;
	onCopyVersion: () => void;
}

const sidebarItems: SidebarItem[] = [
	{ id: "audio", num: "01", labelKey: "settings.audio" },
	{ id: "model", num: "02", labelKey: "settings.models" },
	{ id: "hotkey", num: "03", labelKey: "settings.hotkeys" },
	{ id: "appearance", num: "04", labelKey: "settings.appearance" },
	{ id: "language", num: "05", labelKey: "settings.language" },
	{ id: "privacy", num: "06", labelKey: "sidebar.privacy" },
];

export default function SettingsSidebar(props: SettingsSidebarProps) {
	const [t] = useI18n();

	return (
		<div class="w-56 bg-th-base border-r border-border flex flex-col">
			<div class="px-4 py-4 border-b border-border">
				<h2 class="font-mono text-ac text-sm tracking-wider uppercase">
					[VOXFUSION] &gt; {t("settings.title")}
				</h2>
			</div>
			<nav class="flex-1 py-2">
				<For each={sidebarItems}>
					{(item) => (
						<button
							type="button"
							onClick={() => props.onSectionChange(item.id)}
							class={`flex items-center gap-3 w-full px-4 py-3 font-mono text-xs tracking-wider transition-colors ${
								props.activeSection === item.id
									? "text-ac border-l-2 border-ac bg-th-surface"
									: "text-txt-muted hover:text-txt-secondary hover:bg-th-surface border-l-2 border-transparent"
							}`}
						>
							<span class={props.activeSection === item.id ? "text-ac" : "text-txt-faint"}>
								{item.num}
							</span>
							<span class="uppercase">{t(item.labelKey)}</span>
						</button>
					)}
				</For>
			</nav>
			<Show when={props.appVersion}>
				<div class="px-4 py-3 border-t border-border">
					<button
						type="button"
						onClick={props.onCopyVersion}
						class="group flex items-center gap-1.5 font-mono text-[10px] text-txt-faint hover:text-txt-muted transition-all active:scale-95"
					>
						<span>v{props.appVersion}</span>
						<Show
							when={props.versionCopied}
							fallback={
								<Copy class="w-3 h-3 opacity-0 group-hover:opacity-100 transition-opacity" />
							}
						>
							<Check class="w-3 h-3 text-ac" />
						</Show>
					</button>
				</div>
			</Show>
		</div>
	);
}
