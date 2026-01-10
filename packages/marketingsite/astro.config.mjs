import { defineConfig } from "astro/config";

// https://astro.build/config
export default defineConfig({
	site: "https://voxfusion.com",
	output: "static",
	i18n: {
		defaultLocale: "en",
		locales: ["en", "ru"],
		routing: {
			prefixDefaultLocale: false,
		},
	},
});
