import type { I18nContextType } from "../i18n";
import type { AppStyle } from "./commands/apps";

export function makeStyleLabel(t: I18nContextType[0]) {
	return (style: AppStyle): string => {
		switch (style) {
			case "professional":
				return t("appInstructions.styles.professional");
			case "casual":
				return t("appInstructions.styles.casual");
			case "agents":
				return t("appInstructions.styles.agents");
			case "default":
				return t("appInstructions.styles.default");
		}
	};
}
