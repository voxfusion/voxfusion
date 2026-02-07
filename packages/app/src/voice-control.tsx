import { listen } from "@tauri-apps/api/event";
import { ErrorBoundary } from "solid-js";
import { render } from "solid-js/web";
import "./styles.css";
import { applyTheme, loadSettings } from "./lib/settingsStore";
import VoiceControl from "./pages/VoiceControl";

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

render(
	() => (
		<ErrorBoundary fallback={<div />}>
			<VoiceControl />
		</ErrorBoundary>
	),
	root
);
