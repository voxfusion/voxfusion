import { createContext, useContext } from "solid-js";
import { flatten, translator, resolveTemplate } from "@solid-primitives/i18n";
import { createSignal } from "solid-js";
import { en, type Translations } from "./translations/en";
import { ru } from "./translations/ru";

export type Locale = "en" | "ru";

const dictionaries: Record<Locale, Translations> = {
	en,
	ru,
};

const flattenedDictionaries = {
	en: flatten(en),
	ru: flatten(ru),
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
	if (stored === "en" || stored === "ru") {
		return stored;
	}
	return "ru";
}

export function setStoredLocale(locale: Locale) {
	localStorage.setItem("locale", locale);
}

export { dictionaries };
