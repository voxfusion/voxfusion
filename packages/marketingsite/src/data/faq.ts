import type { TranslationKey } from "../i18n/utils";

/** FAQ entries in display order; shared by the accordion and the FAQPage schema. */
export const faqNumbers = [1, 2, 3, 4, 5, 6, 7, 8] as const;

export function faqItems(t: (key: TranslationKey) => string) {
	return faqNumbers.map((n) => ({
		id: `0${n}`,
		question: t(`faq.item${n}.question`),
		answer: t(`faq.item${n}.answer`),
	}));
}
