import {
	translations,
	defaultLang,
	languages,
	type TranslationKey,
} from "./translations";

export function getLangFromUrl(url: URL) {
	const [, lang] = url.pathname.split("/");
	if (lang in translations) return lang as keyof typeof translations;
	return defaultLang;
}

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
