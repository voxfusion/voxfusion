import { Keyboard } from "lucide-solid";
import { useI18n } from "../../../i18n";
import { useHotkeyRecorder } from "../../../hooks/useHotkeyRecorder";
import { useSettings } from "../../../lib/settingsStore";

export default function HotkeyStep() {
	const [t] = useI18n();
	const settings = useSettings();
	const { isRecording, pendingHotkey, toggleRecording } = useHotkeyRecorder();

	return (
		<div class="text-center max-w-md mx-auto">
			{/* Terminal-style header */}
			<div class="font-mono text-[#ff3e00] text-sm mb-8 tracking-wider">
				[STEP_05] &gt; HOTKEY_CONFIG
			</div>

			{/* Card container */}
			<div class="border border-[#222] bg-[#111] p-8">
				<div class="w-16 h-16 border border-[#333] flex items-center justify-center mx-auto mb-6">
					<Keyboard class="w-8 h-8 text-[#ff3e00]" />
				</div>

				<h2 class="font-mono text-xl uppercase tracking-wider text-[#e0e0e0] mb-3">
					{t("onboarding.hotkeyTitle")}
				</h2>

				<p class="font-mono text-sm text-[#888] mb-8">
					{t("onboarding.hotkeyDescription")}
				</p>

				<div class="space-y-4">
					<div
						class={`px-6 py-4 border font-mono text-lg ${
							isRecording()
								? "border-[#ff3e00] bg-[#ff3e00]/10 text-[#ff3e00]"
								: "border-[#333] bg-[#0a0a0a] text-[#e0e0e0]"
						}`}
					>
						{isRecording()
							? pendingHotkey() || t("onboarding.pressKeys")
							: settings().hotkey}
					</div>

					<button
						type="button"
						onClick={toggleRecording}
						class={`px-6 py-3 font-mono font-bold uppercase tracking-wider text-sm transition-colors ${
							isRecording()
								? "border border-[#333] text-[#888] hover:border-[#ff3e00] hover:text-[#e0e0e0] bg-transparent"
								: "bg-[#ff3e00] text-black hover:bg-[#ff5722]"
						}`}
					>
						{isRecording() ? t("settings.cancel") : t("onboarding.recordHotkey")}
					</button>
				</div>
			</div>
		</div>
	);
}
