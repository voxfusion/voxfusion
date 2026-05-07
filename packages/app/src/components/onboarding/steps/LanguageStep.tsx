import { Check, Globe } from "lucide-solid";
import { For, Show } from "solid-js";
import { type Locale, useI18n } from "../../../i18n";
import { updateLanguage } from "../../../lib/settingsStore";

interface LanguageOption {
	value: Locale;
	label: string;
	nativeLabel: string;
	flag: string;
}

const languages: LanguageOption[] = [
	{ value: "en", label: "English", nativeLabel: "English", flag: "\u{1F1FA}\u{1F1F8}" },
	{ value: "es", label: "Spanish", nativeLabel: "Espa\u00F1ol", flag: "\u{1F1EA}\u{1F1F8}" },
	{ value: "zh", label: "Chinese", nativeLabel: "\u4E2D\u6587", flag: "\u{1F1E8}\u{1F1F3}" },
	{ value: "ja", label: "Japanese", nativeLabel: "\u65E5\u672C\u8A9E", flag: "\u{1F1EF}\u{1F1F5}" },
	{ value: "ko", label: "Korean", nativeLabel: "\uD55C\uAD6D\uC5B4", flag: "\u{1F1F0}\u{1F1F7}" },
	{ value: "de", label: "German", nativeLabel: "Deutsch", flag: "\u{1F1E9}\u{1F1EA}" },
	{ value: "fr", label: "French", nativeLabel: "Fran\u00E7ais", flag: "\u{1F1EB}\u{1F1F7}" },
	{ value: "it", label: "Italian", nativeLabel: "Italiano", flag: "\u{1F1EE}\u{1F1F9}" },
	{ value: "sv", label: "Swedish", nativeLabel: "Svenska", flag: "\u{1F1F8}\u{1F1EA}" },
	{
		value: "hi",
		label: "Hindi",
		nativeLabel: "\u0939\u093F\u0928\u094D\u0926\u0940",
		flag: "\u{1F1EE}\u{1F1F3}",
	},
	{
		value: "uk",
		label: "Ukrainian",
		nativeLabel: "\u0423\u043A\u0440\u0430\u0457\u043D\u0441\u044C\u043A\u0430",
		flag: "\u{1F1FA}\u{1F1E6}",
	},
	{
		value: "ru",
		label: "Russian",
		nativeLabel: "\u0420\u0443\u0441\u0441\u043A\u0438\u0439",
		flag: "\u{1F1F7}\u{1F1FA}",
	},
];

interface LanguageCardProps {
	option: LanguageOption;
	isSelected: boolean;
	onClick: () => void;
}

function LanguageCard(props: LanguageCardProps) {
	return (
		<button
			type="button"
			onClick={props.onClick}
			class={`relative flex flex-col items-center p-4 border transition-all bg-th-surface ${
				props.isSelected ? "border-ac" : "border-border-strong hover:border-ac"
			}`}
		>
			<div class="text-3xl mb-2">{props.option.flag}</div>
			<span
				class={`font-mono text-sm uppercase tracking-wider ${props.isSelected ? "text-ac" : "text-txt-primary"}`}
			>
				{props.option.nativeLabel}
			</span>
			<span class="font-mono text-[10px] text-txt-muted mt-0.5">{props.option.label}</span>

			<Show when={props.isSelected}>
				<div class="absolute top-2 right-2 w-5 h-5 bg-ac flex items-center justify-center">
					<Check class="w-3 h-3 text-ac-on" />
				</div>
			</Show>
		</button>
	);
}

export default function LanguageStep() {
	const [t, { locale, setLocale }] = useI18n();

	const handleLanguageChange = (lang: Locale) => {
		updateLanguage(lang, setLocale);
	};

	return (
		<div class="text-center max-w-lg mx-auto">
			{/* Terminal-style header */}
			<div class="font-mono text-ac text-sm mb-8 tracking-wider">
				[STEP_01] &gt; LANGUAGE_SELECT
			</div>

			{/* Card container */}
			<div class="border border-border bg-th-surface p-6">
				<div class="w-16 h-16 border border-border-strong flex items-center justify-center mx-auto mb-6">
					<Globe class="w-8 h-8 text-ac" />
				</div>

				<h2 class="font-mono text-xl uppercase tracking-wider text-txt-primary mb-3">
					{t("onboarding.languageTitle")}
				</h2>

				<p class="font-mono text-sm text-txt-secondary mb-6">
					{t("onboarding.languageDescription")}
				</p>

				<div class="grid grid-cols-3 gap-3 max-h-[340px] overflow-y-auto">
					<For each={languages}>
						{(lang) => (
							<LanguageCard
								option={lang}
								isSelected={locale() === lang.value}
								onClick={() => handleLanguageChange(lang.value)}
							/>
						)}
					</For>
				</div>
			</div>
		</div>
	);
}
