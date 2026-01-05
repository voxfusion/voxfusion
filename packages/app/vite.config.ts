import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { resolve } from "node:path";

export default defineConfig({
	plugins: [solid()],
	clearScreen: false,
	build: {
		rollupOptions: {
			input: {
				main: resolve(__dirname, "index.html"),
				"voice-control": resolve(__dirname, "voice-control.html"),
			},
		},
	},
	server: {
		port: 1420,
		strictPort: true,
		watch: {
			ignored: ["**/src-tauri/**"],
		},
	},
});
