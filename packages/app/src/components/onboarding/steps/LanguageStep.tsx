import { Show } from "solid-js";
import { Globe, Check } from "lucide-solid";
import { useI18n, type Locale } from "../../../i18n";
import { updateLanguage } from "../../../lib/settingsStore";

interface LanguageCardProps {
	value: Locale;
	label: string;
	nativeLabel: string;
	isSelected: boolean;
	onClick: () => void;
}

function LanguageCard(props: LanguageCardProps) {
	return (
		<button
			type="button"
			onClick={props.onClick}
			class={`relative flex flex-col items-center p-6 rounded-xl border-2 transition-all ${
				props.isSelected
					? "border-primary-500 bg-primary-50 dark:bg-primary-900/20"
					: "border-slate-200 dark:border-midnight-600 hover:border-slate-300 dark:hover:border-midnight-500 bg-white dark:bg-midnight-800"
			}`}
		>
			<div class="text-4xl mb-3">{props.value === "en" ? "🇺🇸" : "🇷🇺"}</div>
			<span
				class={`text-lg font-semibold ${props.isSelected ? "text-primary-700 dark:text-primary-400" : "text-slate-900 dark:text-white"}`}
			>
				{props.nativeLabel}
			</span>
			<span class="text-sm text-slate-500 dark:text-slate-400">{props.label}</span>

			<Show when={props.isSelected}>
				<div class="absolute top-3 right-3 w-6 h-6 bg-primary-500 rounded-full flex items-center justify-center">
					<Check class="w-4 h-4 text-white" />
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
		<div class="text-center max-w-md mx-auto">
			<div class="w-20 h-20 bg-primary-100 dark:bg-primary-900/30 rounded-full flex items-center justify-center mx-auto mb-6">
				<Globe class="w-10 h-10 text-primary-600 dark:text-primary-400" />
			</div>

			<h2 class="text-2xl font-bold text-slate-900 dark:text-white mb-3">
				{t("onboarding.languageTitle")}
			</h2>

			<p class="text-slate-600 dark:text-slate-400 mb-8">
				{t("onboarding.languageDescription")}
			</p>

			<div class="grid grid-cols-2 gap-4">
				<LanguageCard
					value="en"
					label="English"
					nativeLabel="English"
					isSelected={locale() === "en"}
					onClick={() => handleLanguageChange("en")}
				/>
				<LanguageCard
					value="ru"
					label="Russian"
					nativeLabel="Русский"
					isSelected={locale() === "ru"}
					onClick={() => handleLanguageChange("ru")}
				/>
			</div>
		</div>
	);
}
