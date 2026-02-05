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
			class={`relative flex flex-col items-center p-6 border transition-all bg-[#111] ${
				props.isSelected
					? "border-[#ff3e00]"
					: "border-[#333] hover:border-[#ff3e00]"
			}`}
		>
			<div class="text-5xl mb-3">{props.value === "en" ? "🇺🇸" : "🇷🇺"}</div>
			<span
				class={`font-mono text-lg uppercase tracking-wider ${props.isSelected ? "text-[#ff3e00]" : "text-[#e0e0e0]"}`}
			>
				{props.nativeLabel}
			</span>
			<span class="font-mono text-sm text-[#666] mt-1">{props.label}</span>

			<Show when={props.isSelected}>
				<div class="absolute top-3 right-3 w-6 h-6 bg-[#ff3e00] flex items-center justify-center">
					<Check class="w-4 h-4 text-black" />
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
			{/* Terminal-style header */}
			<div class="font-mono text-[#ff3e00] text-sm mb-8 tracking-wider">
				[STEP_01] &gt; LANGUAGE_SELECT
			</div>

			{/* Card container */}
			<div class="border border-[#222] bg-[#111] p-8">
				<div class="w-16 h-16 border border-[#333] flex items-center justify-center mx-auto mb-6">
					<Globe class="w-8 h-8 text-[#ff3e00]" />
				</div>

				<h2 class="font-mono text-xl uppercase tracking-wider text-[#e0e0e0] mb-3">
					{t("onboarding.languageTitle")}
				</h2>

				<p class="font-mono text-sm text-[#888] mb-8">
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
		</div>
	);
}
