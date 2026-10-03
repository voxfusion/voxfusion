import { common } from "./ru/common";
import { downloads } from "./ru/downloads";
import { faq } from "./ru/faq";
import { features } from "./ru/features";
import { hero } from "./ru/hero";
import { how } from "./ru/how";
import { privacy } from "./ru/privacy";
import { security } from "./ru/security";
import { speed } from "./ru/speed";
import { terms } from "./ru/terms";
import { why } from "./ru/why";

export const languages = {
	en: "English",
};

export const defaultLang = "en";

export const translations = {
	en: {
		...common,
		...hero,
		...speed,
		...how,
		...features,
		...why,
		...faq,
		...downloads,
		...privacy,
		...terms,
		...security,
	},
} as const;

export type TranslationKey = keyof (typeof translations)["en"];
