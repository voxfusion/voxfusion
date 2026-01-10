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
				class="flex items-center justify-between w-full px-4 py-3 bg-white dark:bg-midnight-800 border border-slate-200 dark:border-midnight-600 rounded-xl text-slate-900 dark:text-slate-100 hover:bg-slate-50 dark:hover:bg-midnight-700 focus:outline-none focus:ring-2 focus:ring-primary-500 focus:border-primary-500 transition-colors"
			>
				<span class="truncate">{selectedLabel()}</span>
				<ChevronDown
					class={`w-5 h-5 ml-2 text-slate-500 dark:text-slate-400 transition-transform ${isOpen() ? "rotate-180" : ""}`}
				/>
			</button>

			<Show when={isOpen()}>
				<div class="absolute z-50 w-full mt-2 bg-white dark:bg-midnight-800 border border-slate-200 dark:border-midnight-600 rounded-xl shadow-lg max-h-60 overflow-auto">
					<For each={props.options}>
						{(option) => (
							<button
								type="button"
								onClick={() => {
									props.onChange(option.value);
									setIsOpen(false);
								}}
								class={`flex items-center justify-between w-full px-4 py-3 text-left hover:bg-slate-100 dark:hover:bg-midnight-700 transition-colors first:rounded-t-xl last:rounded-b-xl ${
									option.value === props.value
										? "bg-primary-50 dark:bg-primary-900/20 text-primary-700 dark:text-primary-400"
										: "text-slate-900 dark:text-slate-100"
								}`}
							>
								<span class="truncate">{option.label}</span>
								<Show when={option.value === props.value}>
									<Check class="w-4 h-4 text-primary-600 dark:text-primary-400" />
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
			<div class="w-20 h-20 bg-primary-100 dark:bg-primary-900/30 rounded-full flex items-center justify-center mx-auto mb-6">
				<Mic class="w-10 h-10 text-primary-600 dark:text-primary-400" />
			</div>

			<h2 class="text-2xl font-bold text-slate-900 dark:text-white mb-3">
				{t("onboarding.microphoneTitle")}
			</h2>

			<p class="text-slate-600 dark:text-slate-400 mb-8">
				{t("onboarding.microphoneDescription")}
			</p>

			<div class="space-y-3">
				<div class="flex items-center justify-end">
					<button
						type="button"
						onClick={fetchDevices}
						disabled={isLoading()}
						class="flex items-center gap-2 px-3 py-1.5 text-sm text-slate-500 dark:text-slate-400 hover:text-slate-700 dark:hover:text-slate-200 hover:bg-slate-100 dark:hover:bg-midnight-700 rounded-lg transition-colors disabled:opacity-50"
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
	);
}
