import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import { Mic, ChevronDown, Check, RefreshCw } from "lucide-solid";
import { useI18n } from "../../../i18n";
import { useAudioDevices } from "../../../hooks/useAudioDevices";
import { useSettings, updateMicrophone } from "../../../lib/settingsStore";

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
				class="flex items-center justify-between w-full px-4 py-3 bg-[#0a0a0a] border border-[#333] text-[#e0e0e0] hover:border-[#ff3e00] focus:outline-none focus:border-[#ff3e00] transition-colors font-mono text-sm"
			>
				<span class="truncate">{selectedLabel()}</span>
				<ChevronDown
					class={`w-5 h-5 ml-2 text-[#666] transition-transform ${isOpen() ? "rotate-180" : ""}`}
				/>
			</button>

			<Show when={isOpen()}>
				<div class="absolute z-50 w-full mt-1 bg-[#111] border border-[#333] max-h-60 overflow-auto">
					<For each={props.options}>
						{(option) => (
							<button
								type="button"
								onClick={() => {
									props.onChange(option.value);
									setIsOpen(false);
								}}
								class={`flex items-center justify-between w-full px-4 py-3 text-left hover:bg-[#1a1a1a] transition-colors font-mono text-sm ${
									option.value === props.value
										? "bg-[#ff3e00]/10 text-[#ff3e00]"
										: "text-[#e0e0e0]"
								}`}
							>
								<span class="truncate">{option.label}</span>
								<Show when={option.value === props.value}>
									<Check class="w-4 h-4 text-[#ff3e00]" />
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
			<div class="font-mono text-[#ff3e00] text-sm mb-8 tracking-wider">
				[STEP_04] &gt; MICROPHONE_SELECT
			</div>

			{/* Card container */}
			<div class="border border-[#222] bg-[#111] p-8">
				<div class="w-16 h-16 border border-[#333] flex items-center justify-center mx-auto mb-6">
					<Mic class="w-8 h-8 text-[#ff3e00]" />
				</div>

				<h2 class="font-mono text-xl uppercase tracking-wider text-[#e0e0e0] mb-3">
					{t("onboarding.microphoneTitle")}
				</h2>

				<p class="font-mono text-sm text-[#888] mb-8">
					{t("onboarding.microphoneDescription")}
				</p>

				<div class="space-y-3">
					<div class="flex items-center justify-end">
						<button
							type="button"
							onClick={fetchDevices}
							disabled={isLoading()}
							class="flex items-center gap-2 px-3 py-1.5 font-mono text-xs text-[#666] hover:text-[#ff3e00] border border-transparent hover:border-[#333] transition-colors disabled:opacity-50 uppercase tracking-wider"
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
