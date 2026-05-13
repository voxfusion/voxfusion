import { RefreshCw } from "lucide-solid";
import { createSignal, onCleanup, type Accessor } from "solid-js";
import type { I18nContextType } from "../../i18n";
import { capture } from "../../lib/posthog";
import type { AudioDevice, Settings } from "../../lib/settingsStore";
import { updateMicrophone, updateMuteMediaWhileRecording } from "../../lib/settingsStore";
import ToggleOption from "../ToggleOption";
import Select from "./Select";
import type { SelectOption } from "./types";

interface AudioSettingsProps {
	t: I18nContextType[0];
	settings: Accessor<Settings>;
	audioDevices: AudioDevice[];
	onRefreshDevices: () => void;
}

const REFRESH_ANIMATION_MS = 600;

export default function AudioSettings(props: AudioSettingsProps) {
	const [isRefreshAnimating, setIsRefreshAnimating] = createSignal(false);
	let refreshAnimationTimeout: ReturnType<typeof setTimeout> | undefined;

	const microphoneOptions = (): SelectOption[] => {
		const defaultDevice = props.audioDevices.find((d) => d.isDefault);
		const otherDevices = props.audioDevices.filter((d) => !d.isDefault);

		return [
			{
				value: "default",
				label: defaultDevice
					? `${props.t("settings.defaultMicrophone")} (${defaultDevice.name})`
					: props.t("settings.defaultMicrophone"),
			},
			...otherDevices.map((device) => ({
				value: device.name,
				label: device.name,
			})),
		];
	};

	const handleRefreshDevices = () => {
		if (isRefreshAnimating()) return;

		setIsRefreshAnimating(true);
		props.onRefreshDevices();
		refreshAnimationTimeout = setTimeout(() => {
			setIsRefreshAnimating(false);
			refreshAnimationTimeout = undefined;
		}, REFRESH_ANIMATION_MS);
	};

	onCleanup(() => {
		clearTimeout(refreshAnimationTimeout);
	});

	return (
		<div class="space-y-6">
			<div>
				<div class="flex items-center justify-between mb-3">
					<div class="font-mono text-txt-muted text-xs uppercase tracking-wider">INPUT_DEVICE</div>
					<button
						type="button"
						onClick={handleRefreshDevices}
						disabled={isRefreshAnimating()}
						class="p-1.5 text-txt-muted hover:text-ac transition-colors disabled:opacity-50"
						title="Refresh devices"
					>
						<RefreshCw
							class={`w-4 h-4 ${isRefreshAnimating() ? "animate-refresh-devices-once" : ""}`}
						/>
					</button>
				</div>
				<Select
					value={props.settings().selectedMicrophoneId ?? "default"}
					options={microphoneOptions()}
					onChange={(value) => {
						capture("settings_microphone_changed");
						updateMicrophone(value === "default" ? null : value);
					}}
				/>
				<p class="mt-3 font-mono text-xs text-txt-faint">
					{props.t("settings.microphoneDescription")}
				</p>
			</div>

			<ToggleOption
				label={props.t("settings.muteMediaWhileRecording")}
				description={props.t("settings.muteMediaWhileRecordingDescription")}
				isEnabled={props.settings().muteMediaWhileRecording}
				onChange={(enabled) => {
					capture("settings_mute_media_while_recording_changed", { enabled });
					updateMuteMediaWhileRecording(enabled);
				}}
			/>
		</div>
	);
}
