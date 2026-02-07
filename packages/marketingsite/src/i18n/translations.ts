import { common } from "./ru/common";
import { hero } from "./ru/hero";
import { features } from "./ru/features";
import { demo } from "./ru/demo";
import { pricing } from "./ru/pricing";
import { downloads } from "./ru/downloads";
import { privacy } from "./ru/privacy";
import { terms } from "./ru/terms";
import { security } from "./ru/security";

export const languages = {
	ru: "Русский",
};

export const defaultLang = "ru";

export const translations = {
	ru: {
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

export type TranslationKey = keyof (typeof translations)["ru"];
