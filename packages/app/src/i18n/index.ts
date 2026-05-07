import { flatten, resolveTemplate, translator } from "@solid-primitives/i18n";
import { createSignal } from "solid-js";
import { createContext, useContext } from "solid-js";
import { de } from "./translations/de";
import { type Translations, en } from "./translations/en";
import { es } from "./translations/es";
import { fr } from "./translations/fr";
import { it } from "./translations/it";
import { ru } from "./translations/ru";
import { zh } from "./translations/zh";

export type Locale = "en" | "ru" | "es" | "zh" | "de" | "fr" | "it";

const ALL_LOCALES: Locale[] = ["en", "ru", "es", "zh", "de", "fr", "it"];

const dictionaries: Record<Locale, Translations> = {
	en,
	ru,
	es,
	zh,
	de,
	fr,
	it,
};

const flattenedDictionaries = {
	en: flatten(en),
	ru: flatten(ru),
	es: flatten(es),
	zh: flatten(zh),
	de: flatten(de),
	fr: flatten(fr),
	it: flatten(it),
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
	return "en";
}

export function setStoredLocale(locale: Locale) {
	localStorage.setItem("locale", locale);
}

export { dictionaries };
