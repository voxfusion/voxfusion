import { Show, type Accessor } from "solid-js";
import type { I18nContextType } from "../../i18n";
import { hotkeyDisplayName } from "../../lib/hotkeyUtils";
import type { Settings } from "../../lib/settingsStore";

interface HotkeySettingsProps {
	t: I18nContextType[0];
	settings: Accessor<Settings>;
	isRecordingHotkey: boolean;
	pendingHotkey: string | null;
	hotkeyError: string | null;
	onToggleHotkeyRecording: () => void;
	isRecordingHoldToSpeakHotkey: boolean;
	pendingHoldToSpeakHotkey: string | null;
	holdToSpeakHotkeyError: string | null;
	onToggleHoldToSpeakHotkeyRecording: () => void;
}

export default function HotkeySettings(props: HotkeySettingsProps) {
	return (
		<div class="space-y-6">
			<div>
				<div class="font-mono text-txt-muted text-xs uppercase tracking-wider block mb-3">
					HANDS_FREE_TRIGGER
				</div>
				<div class="flex items-center gap-3">
					<div
						class={`flex-1 px-4 py-3 border font-mono text-center text-sm ${
							props.isRecordingHotkey
								? "border-ac bg-ac-bg text-ac"
								: "border-border-strong bg-th-surface text-txt-primary"
						}`}
					>
						{props.isRecordingHotkey
							? props.pendingHotkey || "_ WAITING FOR INPUT _"
							: hotkeyDisplayName(props.settings().hotkey)}
					</div>
					<button
						type="button"
						onClick={props.onToggleHotkeyRecording}
						disabled={
							props.holdToSpeakHotkeyError !== null ||
							(props.isRecordingHoldToSpeakHotkey && !props.isRecordingHotkey)
						}
						class={`px-4 py-3 font-mono text-xs uppercase tracking-wider transition-colors disabled:opacity-50 ${
							props.isRecordingHotkey
								? "bg-border text-txt-secondary hover:bg-border-strong"
								: "bg-ac text-ac-on hover:bg-ac-hover"
						}`}
					>
						{props.isRecordingHotkey ? "[CANCEL]" : "[CHANGE]"}
					</button>
				</div>
				<Show when={props.hotkeyError}>
					<div class="font-mono text-xs text-red-500">{props.hotkeyError}</div>
				</Show>
				<p class="mt-3 font-mono text-xs text-txt-faint">{props.t("settings.hotkeyDescription")}</p>
			</div>
			<div>
				<div class="font-mono text-txt-muted text-xs uppercase tracking-wider block mb-3">
					HOLD_TO_SPEAK_TRIGGER
				</div>
				<div class="flex items-center gap-3">
					<div
						class={`flex-1 px-4 py-3 border font-mono text-center text-sm ${
							props.isRecordingHoldToSpeakHotkey
								? "border-ac bg-ac-bg text-ac"
								: "border-border-strong bg-th-surface text-txt-primary"
						}`}
					>
						{props.isRecordingHoldToSpeakHotkey
							? props.pendingHoldToSpeakHotkey || "_ WAITING FOR INPUT _"
							: hotkeyDisplayName(props.settings().holdToSpeakHotkey)}
					</div>
					<button
						type="button"
						onClick={props.onToggleHoldToSpeakHotkeyRecording}
						disabled={
							props.hotkeyError !== null ||
							(props.isRecordingHotkey && !props.isRecordingHoldToSpeakHotkey)
						}
						class={`px-4 py-3 font-mono text-xs uppercase tracking-wider transition-colors disabled:opacity-50 ${
							props.isRecordingHoldToSpeakHotkey
								? "bg-border text-txt-secondary hover:bg-border-strong"
								: "bg-ac text-ac-on hover:bg-ac-hover"
						}`}
					>
						{props.isRecordingHoldToSpeakHotkey ? "[CANCEL]" : "[CHANGE]"}
					</button>
				</div>
				<Show when={props.holdToSpeakHotkeyError}>
					<div class="font-mono text-xs text-red-500">{props.holdToSpeakHotkeyError}</div>
				</Show>
				<p class="mt-3 font-mono text-xs text-txt-faint">
					{props.t("settings.holdToSpeakHotkeyDescription")}
				</p>
			</div>
		</div>
	);
}
