import { For } from "solid-js";
import { Check } from "lucide-solid";

interface StepIndicatorProps {
	currentStep: number;
	totalSteps: number;
}

export default function StepIndicator(props: StepIndicatorProps) {
	return (
		<div class="flex items-center gap-3">
			<For each={Array.from({ length: props.totalSteps }, (_, i) => i + 1)}>
				{(step) => (
					<div class="flex items-center">
						<div
							class={`w-10 h-10 rounded-full flex items-center justify-center font-medium transition-all ${
								step < props.currentStep
									? "bg-primary-500 text-white"
									: step === props.currentStep
										? "bg-primary-500 text-white ring-4 ring-primary-200 dark:ring-primary-900/50"
										: "bg-slate-200 dark:bg-midnight-700 text-slate-500 dark:text-slate-400"
							}`}
						>
							{step < props.currentStep ? <Check class="w-5 h-5" /> : step}
						</div>
						{step < props.totalSteps && (
							<div
								class={`w-12 h-1 mx-2 rounded-full transition-colors ${
									step < props.currentStep
										? "bg-primary-500"
										: "bg-slate-200 dark:bg-midnight-700"
								}`}
							/>
						)}
					</div>
				)}
			</For>
		</div>
	);
}
