import sitemap from "@astrojs/sitemap";
import { defineConfig } from "astro/config";

// https://astro.build/config
export default defineConfig({
	site: "https://voxfusion.com",
	output: "static",
	integrations: [
		sitemap({
			filter: (page) => !page.includes("/404"),
		}),
	],
	i18n: {
		defaultLocale: "en",
		locales: ["en"],
		routing: {
			prefixDefaultLocale: false,
		},
	},
});
