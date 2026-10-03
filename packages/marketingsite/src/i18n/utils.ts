import { type TranslationKey, defaultLang, languages, translations } from "./translations";

export function useTranslations(lang: keyof typeof translations) {
	return function t(key: TranslationKey) {
		return translations[lang][key] || translations[defaultLang][key];
	};
}

export function getLocalizedPath(path: string, lang: keyof typeof translations) {
	if (lang === defaultLang) {
		return path;
	}
	return `/${lang}${path}`;
}

export { translations, defaultLang, languages };
export type { TranslationKey };
