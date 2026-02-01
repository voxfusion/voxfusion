import { defineConfig } from "astro/config";

// https://astro.build/config
export default defineConfig({
	site: "https://voxfusion.com",
	output: "static",
	i18n: {
		defaultLocale: "ru",
		locales: ["ru", "en"],
		routing: {
			prefixDefaultLocale: false,
		},
	},
});
