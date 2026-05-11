import type { Locale } from "../../i18n";
import { capture } from "../../lib/posthog";
import { updateLanguage } from "../../lib/settingsStore";
import Select from "./Select";
import type { SelectOption } from "./types";

interface LanguageSettingsProps {
	locale: Locale;
	setLocale: (locale: Locale) => void;
}

const languageOptions: SelectOption[] = [
	{ value: "en", label: "ENGLISH" },
	{ value: "ru", label: "RUSSIAN" },
	{ value: "es", label: "ESPAÑOL" },
	{ value: "zh", label: "中文" },
	{ value: "de", label: "DEUTSCH" },
	{ value: "fr", label: "FRANÇAIS" },
	{ value: "it", label: "ITALIANO" },
];

export default function LanguageSettings(props: LanguageSettingsProps) {
	return (
		<div class="space-y-6">
			<div>
				<div class="font-mono text-txt-muted text-xs uppercase tracking-wider block mb-3">
					INTERFACE_LANGUAGE
				</div>
				<Select
					value={props.locale}
					options={languageOptions}
					onChange={(value) => {
						capture("settings_language_changed", { language: value });
						updateLanguage(value as Locale, props.setLocale);
					}}
				/>
			</div>
		</div>
	);
}
