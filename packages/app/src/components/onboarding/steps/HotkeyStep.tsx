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
			<div class="w-20 h-20 bg-primary-100 dark:bg-primary-900/30 rounded-full flex items-center justify-center mx-auto mb-6">
				<Keyboard class="w-10 h-10 text-primary-600 dark:text-primary-400" />
			</div>

			<h2 class="text-2xl font-bold text-slate-900 dark:text-white mb-3">
				{t("onboarding.hotkeyTitle")}
			</h2>

			<p class="text-slate-600 dark:text-slate-400 mb-8">
				{t("onboarding.hotkeyDescription")}
			</p>

			<div class="space-y-4">
				<div
					class={`px-6 py-4 border-2 rounded-xl font-mono text-lg ${
						isRecording()
							? "border-primary-500 bg-primary-50 dark:bg-primary-900/20 text-primary-700 dark:text-primary-400"
							: "border-slate-200 dark:border-midnight-600 bg-slate-50 dark:bg-midnight-800 text-slate-700 dark:text-slate-300"
					}`}
				>
					{isRecording()
						? pendingHotkey() || t("onboarding.pressKeys")
						: settings().hotkey}
				</div>

				<button
					type="button"
					onClick={toggleRecording}
					class={`px-6 py-3 rounded-xl font-medium transition-colors ${
						isRecording()
							? "bg-slate-200 dark:bg-midnight-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-midnight-600"
							: "bg-primary-500 text-white hover:bg-primary-600"
					}`}
				>
					{isRecording() ? t("settings.cancel") : t("onboarding.recordHotkey")}
				</button>
			</div>
		</div>
	);
}
