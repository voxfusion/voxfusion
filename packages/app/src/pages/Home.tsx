import { onMount } from "solid-js";
import TranscriptionList from "../components/TranscriptionList";
import { useI18n } from "../i18n";
import { hotkeyDisplayName } from "../lib/hotkeyUtils";
import { capture } from "../lib/posthog";
import { useSettings } from "../lib/settingsStore";

export default function Home() {
	const [t] = useI18n();
	const settings = useSettings();

	onMount(() => {
		capture("$pageview", { $current_url: "/" });
	});

	return (
		<div class="min-h-screen px-6 py-8">
			<div class="max-w-2xl mx-auto">
				{/* Terminal-style section header */}
				<div class="flex items-center gap-3 mb-6">
					<span class="text-ac font-mono text-sm">[HOME]</span>
					<span class="text-txt-muted font-mono text-sm">&gt;</span>
					<h1 class="text-txt-primary font-mono uppercase tracking-wider text-sm">
						{t("home.yourTranscriptions")}
					</h1>
					<div class="flex-1 h-px bg-border" />
				</div>

				{/* Subtitle */}
				<p class="text-txt-muted font-mono text-xs mb-8">
					{t("home.pressToRecord", { hotkey: hotkeyDisplayName(settings().hotkey) })}
				</p>

				<TranscriptionList />
			</div>
		</div>
	);
}
