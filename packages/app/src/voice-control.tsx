import { listen } from "@tauri-apps/api/event";
import { render } from "solid-js/web";
import "./styles.css";
import { type Theme, loadSettings } from "./lib/settingsStore";
import VoiceControl from "./pages/VoiceControl";

function applyTheme(theme: Theme): void {
	const root = document.documentElement;
	if (theme === "system") {
		const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
		root.classList.toggle("dark", prefersDark);
	} else {
		root.classList.toggle("dark", theme === "dark");
	}
}

// Apply theme on load and listen for changes
loadSettings().then((settings) => applyTheme(settings.theme));

listen("settings-changed", async () => {
	const settings = await loadSettings();
	applyTheme(settings.theme);
});

window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", async () => {
	const settings = await loadSettings();
	if (settings.theme === "system") {
		applyTheme("system");
	}
});

const root = document.getElementById("root");

if (!root) {
	throw new Error("Root element not found");
}

render(() => <VoiceControl />, root);
