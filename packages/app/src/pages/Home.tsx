import TranscriptionList from "../components/TranscriptionList";
import { useI18n } from "../i18n";

export default function Home() {
	const [t] = useI18n();

	return (
		<div class="min-h-screen px-6 py-8">
			<div class="max-w-2xl mx-auto">
				{/* Terminal-style section header */}
				<div class="flex items-center gap-3 mb-6">
					<span class="text-[#ff3e00] font-mono text-sm">[HOME]</span>
					<span class="text-[#666] font-mono text-sm">&gt;</span>
					<h1 class="text-[#e0e0e0] font-mono uppercase tracking-wider text-sm">
						{t("home.yourTranscriptions")}
					</h1>
					<div class="flex-1 h-px bg-[#222]" />
				</div>

				{/* Subtitle */}
				<p class="text-[#666] font-mono text-xs mb-8">
					{t("home.pressToRecord")}
				</p>

				<TranscriptionList />
			</div>
		</div>
	);
}
