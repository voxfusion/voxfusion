import type { Accessor } from "solid-js";
import type { I18nContextType } from "../../i18n";
import type { Settings } from "../../lib/settingsStore";
import { updateAnalyticsEnabled } from "../../lib/settingsStore";
import ToggleOption from "../ToggleOption";

interface PrivacySettingsProps {
	t: I18nContextType[0];
	settings: Accessor<Settings>;
}

export default function PrivacySettings(props: PrivacySettingsProps) {
	return (
		<div class="space-y-6">
			<ToggleOption
				label={props.t("settings.analytics")}
				description={props.t("settings.analyticsDescription")}
				isEnabled={props.settings().analyticsEnabled}
				onChange={(enabled) => updateAnalyticsEnabled(enabled)}
			/>
			<p class="font-mono text-xs text-txt-faint">{props.t("settings.analyticsNote")}</p>
		</div>
	);
}
