import { Keyboard } from "lucide-solid";
import { useHotkeyRecorder } from "../../../hooks/useHotkeyRecorder";
import { useI18n } from "../../../i18n";
import {
	hotkeyDisplayName,
	validateHandsFreeHotkey,
	validateHoldToSpeakHotkey,
} from "../../../lib/hotkeyUtils";
import { updateHoldToSpeakHotkey, useSettings } from "../../../lib/settingsStore";

export default function HotkeyStep() {
	const [t] = useI18n();
	const settings = useSettings();
	const { isRecording, pendingHotkey, error, toggleRecording } = useHotkeyRecorder({
		validator: validateHandsFreeHotkey,
		recorderId: "handsfree",
	});
	const {
		isRecording: isRecordingHoldToSpeak,
		pendingHotkey: pendingHoldToSpeakHotkey,
		error: holdToSpeakError,
		toggleRecording: toggleHoldToSpeakRecording,
	} = useHotkeyRecorder({
		onHotkeyRecorded: updateHoldToSpeakHotkey,
		validator: validateHoldToSpeakHotkey,
		recorderId: "holdtospeak",
	});

	return (
		<div class="text-center max-w-md mx-auto">
			{/* Terminal-style header */}
			<div class="font-mono text-ac text-sm mb-8 tracking-wider">[STEP_04] &gt; HOTKEY_CONFIG</div>

			{/* Card container */}
			<div class="border border-border bg-th-surface p-8">
				<div class="w-16 h-16 border border-border-strong flex items-center justify-center mx-auto mb-6">
					<Keyboard class="w-8 h-8 text-ac" />
				</div>

				<h2 class="font-mono text-xl uppercase tracking-wider text-txt-primary mb-3">
					{t("onboarding.hotkeyTitle")}
				</h2>

				<p class="font-mono text-sm text-txt-secondary mb-8">{t("onboarding.hotkeyDescription")}</p>

				<div class="space-y-4">
					<div class="font-mono text-xs text-txt-muted uppercase tracking-wider text-left">
						{t("onboarding.handsFreeHotkey")}
					</div>
					<div
						class={`px-6 py-4 border font-mono text-lg ${
							isRecording()
								? "border-ac bg-ac-bg text-ac"
								: "border-border-strong bg-th-base text-txt-primary"
						}`}
					>
						{isRecording()
							? pendingHotkey() || t("onboarding.pressKeys")
							: hotkeyDisplayName(settings().hotkey)}
					</div>

					<button
						type="button"
						onClick={toggleRecording}
						disabled={holdToSpeakError() !== null || (isRecordingHoldToSpeak() && !isRecording())}
						class={`px-6 py-3 font-mono font-bold uppercase tracking-wider text-sm transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
							isRecording()
								? "border border-border-strong text-txt-secondary hover:border-ac hover:text-txt-primary bg-transparent"
								: "bg-ac text-ac-on hover:bg-ac-hover"
						}`}
					>
						{isRecording() ? t("settings.cancel") : t("onboarding.recordHotkey")}
					</button>
					{error() && <div class="font-mono text-xs text-red-500">{error()}</div>}

					<div class="font-mono text-xs text-txt-muted uppercase tracking-wider text-left pt-4">
						{t("onboarding.holdToSpeakHotkey")}
					</div>
					<div
						class={`px-6 py-4 border font-mono text-lg ${
							isRecordingHoldToSpeak()
								? "border-ac bg-ac-bg text-ac"
								: "border-border-strong bg-th-base text-txt-primary"
						}`}
					>
						{isRecordingHoldToSpeak()
							? pendingHoldToSpeakHotkey() || t("onboarding.pressKeys")
							: hotkeyDisplayName(settings().holdToSpeakHotkey)}
					</div>

					<button
						type="button"
						onClick={toggleHoldToSpeakRecording}
						disabled={error() !== null || (isRecording() && !isRecordingHoldToSpeak())}
						class={`px-6 py-3 font-mono font-bold uppercase tracking-wider text-sm transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
							isRecordingHoldToSpeak()
								? "border border-border-strong text-txt-secondary hover:border-ac hover:text-txt-primary bg-transparent"
								: "bg-ac text-ac-on hover:bg-ac-hover"
						}`}
					>
						{isRecordingHoldToSpeak()
							? t("settings.cancel")
							: t("onboarding.recordHoldToSpeakHotkey")}
					</button>
					{holdToSpeakError() && (
						<div class="font-mono text-xs text-red-500">{holdToSpeakError()}</div>
					)}
				</div>
			</div>
		</div>
	);
}
