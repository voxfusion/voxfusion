import { Check, ChevronDown, Mic, RefreshCw } from "lucide-solid";
import { For, Show, createEffect, createSignal, onCleanup } from "solid-js";
import { useAudioDevices } from "../../../hooks/useAudioDevices";
import { useI18n } from "../../../i18n";
import { updateMicrophone, useSettings } from "../../../lib/settingsStore";

interface SelectOption {
	value: string;
	label: string;
}

interface SelectProps {
	value: string;
	options: SelectOption[];
	onChange: (value: string) => void;
	placeholder?: string;
}

function Select(props: SelectProps) {
	const [isOpen, setIsOpen] = createSignal(false);
	let containerRef: HTMLDivElement | undefined;

	const selectedLabel = () => {
		const option = props.options.find((o) => o.value === props.value);
		return option?.label ?? props.placeholder ?? "";
	};

	const handleClickOutside = (e: MouseEvent) => {
		if (containerRef && !containerRef.contains(e.target as Node)) {
			setIsOpen(false);
		}
	};

	createEffect(() => {
		if (isOpen()) {
			document.addEventListener("click", handleClickOutside);
		} else {
			document.removeEventListener("click", handleClickOutside);
		}
	});

	onCleanup(() => {
		document.removeEventListener("click", handleClickOutside);
	});

	return (
		<div ref={containerRef} class="relative">
			<button
				type="button"
				onClick={() => setIsOpen(!isOpen())}
				class="flex items-center justify-between w-full px-4 py-3 bg-th-base border border-border-strong text-txt-primary hover:border-ac focus:outline-none focus:border-ac transition-colors font-mono text-sm"
			>
				<span class="truncate">{selectedLabel()}</span>
				<ChevronDown
					class={`w-5 h-5 ml-2 text-txt-muted transition-transform ${isOpen() ? "rotate-180" : ""}`}
				/>
			</button>

			<Show when={isOpen()}>
				<div class="absolute z-50 w-full mt-1 bg-th-surface border border-border-strong max-h-60 overflow-auto">
					<For each={props.options}>
						{(option) => (
							<button
								type="button"
								onClick={() => {
									props.onChange(option.value);
									setIsOpen(false);
								}}
								class={`flex items-center justify-between w-full px-4 py-3 text-left hover:bg-th-hover transition-colors font-mono text-sm ${
									option.value === props.value ? "bg-ac-bg text-ac" : "text-txt-primary"
								}`}
							>
								<span class="truncate">{option.label}</span>
								<Show when={option.value === props.value}>
									<Check class="w-4 h-4 text-ac" />
								</Show>
							</button>
						)}
					</For>
				</div>
			</Show>
		</div>
	);
}

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
				[STEP_03] &gt; MICROPHONE_SELECT
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
