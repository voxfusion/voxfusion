import { createContext, useContext } from "solid-js";
import { flatten, translator, resolveTemplate } from "@solid-primitives/i18n";
import { createSignal } from "solid-js";
import { en, type Translations } from "./translations/en";
import { ru } from "./translations/ru";
import { es } from "./translations/es";
import { zh } from "./translations/zh";
import { ja } from "./translations/ja";
import { ko } from "./translations/ko";
import { de } from "./translations/de";
import { fr } from "./translations/fr";
import { it } from "./translations/it";
import { sv } from "./translations/sv";
import { hi } from "./translations/hi";
import { uk } from "./translations/uk";

export type Locale = "en" | "ru" | "es" | "zh" | "ja" | "ko" | "de" | "fr" | "it" | "sv" | "hi" | "uk";

const ALL_LOCALES: Locale[] = ["en", "ru", "es", "zh", "ja", "ko", "de", "fr", "it", "sv", "hi", "uk"];

const dictionaries: Record<Locale, Translations> = {
	en,
	ru,
	es,
	zh,
	ja,
	ko,
	de,
	fr,
	it,
	sv,
	hi,
	uk,
};

const flattenedDictionaries = {
	en: flatten(en),
	ru: flatten(ru),
	es: flatten(es),
	zh: flatten(zh),
	ja: flatten(ja),
	ko: flatten(ko),
	de: flatten(de),
	fr: flatten(fr),
	it: flatten(it),
	sv: flatten(sv),
	hi: flatten(hi),
	uk: flatten(uk),
};

export function createAppI18n(initialLocale: Locale = "en") {
	const [locale, setLocale] = createSignal<Locale>(initialLocale);
	const [dict, setDict] = createSignal(flattenedDictionaries[initialLocale]);

	const t = translator(() => dict(), resolveTemplate);

	const switchLocale = (newLocale: Locale) => {
		setLocale(newLocale);
		setDict(flattenedDictionaries[newLocale]);
		setStoredLocale(newLocale);
	};

	return [t, { locale, setLocale: switchLocale }] as const;
}

export type I18nContextType = ReturnType<typeof createAppI18n>;

export const I18nCtx = createContext<I18nContextType>();

export function useI18n() {
	const context = useContext(I18nCtx);
	if (!context) {
		throw new Error("useI18n must be used within an I18nProvider");
	}
	return context;
}

export function getStoredLocale(): Locale {
	const stored = localStorage.getItem("locale");
	if (stored && ALL_LOCALES.includes(stored as Locale)) {
		return stored as Locale;
	}
	return "ru";
}

export function setStoredLocale(locale: Locale) {
	localStorage.setItem("locale", locale);
}

export { dictionaries };
