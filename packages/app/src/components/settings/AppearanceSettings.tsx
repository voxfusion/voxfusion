import { type Accessor, For } from "solid-js";
import { capture } from "../../lib/posthog";
import type { Settings, Theme } from "../../lib/settingsStore";
import { updateTheme } from "../../lib/settingsStore";
import ThemeOption from "./ThemeOption";

interface AppearanceSettingsProps {
	settings: Accessor<Settings>;
}

const themeOptions: { value: Theme; label: string }[] = [
	{ value: "light", label: "LIGHT" },
	{ value: "dark", label: "DARK" },
	{ value: "system", label: "SYSTEM" },
];

export default function AppearanceSettings(props: AppearanceSettingsProps) {
	return (
		<div class="space-y-6">
			<div>
				<div class="font-mono text-txt-muted text-xs uppercase tracking-wider block mb-4">
					THEME_MODE
				</div>
				<div class="grid grid-cols-3 gap-4">
					<For each={themeOptions}>
						{(theme) => (
							<ThemeOption
								value={theme.value}
								label={theme.label}
								isSelected={props.settings().theme === theme.value}
								onClick={() => {
									capture("settings_theme_changed", { theme: theme.value });
									updateTheme(theme.value);
								}}
							/>
						)}
					</For>
				</div>
			</div>
		</div>
	);
}
