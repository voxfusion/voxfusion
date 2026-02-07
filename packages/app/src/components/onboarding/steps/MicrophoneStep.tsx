import { Mic, RefreshCw } from "lucide-solid";
import { createEffect } from "solid-js";
import { useAudioDevices } from "../../../hooks/useAudioDevices";
import { useI18n } from "../../../i18n";
import { updateMicrophone, useSettings } from "../../../lib/settingsStore";
import Select, { type SelectOption } from "../../Select";

export default function MicrophoneStep() {
	const [t] = useI18n();
	const settings = useSettings();
	const { devices, isLoading, fetchDevices } = useAudioDevices();

	createEffect(() => {
		fetchDevices();
	});

	const microphoneOptions = (): SelectOption[] => {
		const deviceList = devices();
		const defaultDevice = deviceList.find((d) => d.isDefault);
		const otherDevices = deviceList.filter((d) => !d.isDefault);

		return [
			{
				value: "default",
				label: defaultDevice
					? `${t("settings.defaultMicrophone")} (${defaultDevice.name})`
					: t("settings.defaultMicrophone"),
			},
			...otherDevices.map((device) => ({
				value: device.name,
				label: device.name,
			})),
		];
	};

	return (
		<div class="text-center max-w-md mx-auto">
			{/* Terminal-style header */}
			<div class="font-mono text-ac text-sm mb-8 tracking-wider">
				[STEP_04] &gt; MICROPHONE_SELECT
			</div>

			{/* Card container */}
			<div class="border border-border bg-th-surface p-8">
				<div class="w-16 h-16 border border-border-strong flex items-center justify-center mx-auto mb-6">
					<Mic class="w-8 h-8 text-ac" />
				</div>

				<h2 class="font-mono text-xl uppercase tracking-wider text-txt-primary mb-3">
					{t("onboarding.microphoneTitle")}
				</h2>

				<p class="font-mono text-sm text-txt-secondary mb-8">
					{t("onboarding.microphoneDescription")}
				</p>

				<div class="space-y-3">
					<div class="flex items-center justify-end">
						<button
							type="button"
							onClick={fetchDevices}
							disabled={isLoading()}
							class="flex items-center gap-2 px-3 py-1.5 font-mono text-xs text-txt-muted hover:text-ac border border-transparent hover:border-border-strong transition-colors disabled:opacity-50 uppercase tracking-wider"
						>
							<RefreshCw class={`w-4 h-4 ${isLoading() ? "animate-spin" : ""}`} />
							{t("onboarding.refreshDevices")}
						</button>
					</div>

					<Select
						value={settings().selectedMicrophoneId ?? "default"}
						options={microphoneOptions()}
						onChange={(value) => {
							updateMicrophone(value === "default" ? null : value);
						}}
						placeholder={t("onboarding.selectMicrophone")}
					/>
				</div>
			</div>
		</div>
	);
}
