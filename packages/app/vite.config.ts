import { resolve } from "node:path";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

export default defineConfig({
	plugins: [solid()],
	clearScreen: false,
	build: {
		sourcemap: false,
		minify: "terser",
		terserOptions: {
			compress: {
				drop_console: true,
				drop_debugger: true,
			},
		},
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
