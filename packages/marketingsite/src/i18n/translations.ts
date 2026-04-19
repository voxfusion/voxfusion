import { common } from "./ru/common";
import { demo } from "./ru/demo";
import { downloads } from "./ru/downloads";
import { features } from "./ru/features";
import { hero } from "./ru/hero";
import { pricing } from "./ru/pricing";
import { privacy } from "./ru/privacy";
import { security } from "./ru/security";
import { terms } from "./ru/terms";

export const languages = {
	en: "English",
};

export const defaultLang = "en";

export const translations = {
	en: {
		...common,
		...hero,
		...features,
		...demo,
		...pricing,
		...downloads,
		...privacy,
		...terms,
		...security,
	},
} as const;

export type TranslationKey = keyof (typeof translations)["en"];
