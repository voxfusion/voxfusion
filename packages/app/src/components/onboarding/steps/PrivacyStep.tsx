import { ShieldCheck } from "lucide-solid";
import { useI18n } from "../../../i18n";
import { updateAnalyticsEnabled, useSettings } from "../../../lib/settingsStore";
import ToggleOption from "../../ToggleOption";

export default function PrivacyStep() {
	const [t] = useI18n();
	const settings = useSettings();

	return (
		<div class="text-center max-w-md mx-auto">
			{/* Terminal-style header */}
			<div class="font-mono text-ac text-sm mb-8 tracking-wider">[STEP_05] &gt; PRIVACY_CHOICE</div>

			{/* Card container */}
			<div class="border border-border bg-th-surface p-8">
				<div class="w-16 h-16 border border-border-strong flex items-center justify-center mx-auto mb-6">
					<ShieldCheck class="w-8 h-8 text-ac" />
				</div>

				<h2 class="font-mono text-xl uppercase tracking-wider text-txt-primary mb-3">
					{t("onboarding.privacyTitle")}
				</h2>

				<p class="font-mono text-sm text-txt-secondary mb-8">
					{t("onboarding.privacyDescription")}
				</p>

				<div class="text-left border border-border-strong bg-th-base p-4">
					<ToggleOption
						label={t("onboarding.privacyToggleLabel")}
						description={t("onboarding.privacyToggleDescription")}
						isEnabled={settings().analyticsEnabled}
						onChange={(enabled) => updateAnalyticsEnabled(enabled)}
					/>
				</div>
			</div>
		</div>
	);
}
