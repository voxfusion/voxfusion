import { For } from "solid-js";

interface StepIndicatorProps {
	currentStep: number;
	totalSteps: number;
}

export default function StepIndicator(props: StepIndicatorProps) {
	const formatStep = (step: number) => step.toString().padStart(2, "0");

	return (
		<div class="flex items-center gap-4 font-mono">
			<For each={Array.from({ length: props.totalSteps }, (_, i) => i + 1)}>
				{(step) => (
					<div class="flex items-center">
						<div
							class={`text-sm tracking-wider transition-all ${
								step < props.currentStep
									? "text-[#ff3e00]"
									: step === props.currentStep
										? "text-[#ff3e00] font-bold"
										: "text-[#666]"
							}`}
						>
							{formatStep(step)}
						</div>
						{step < props.totalSteps && (
							<div
								class={`w-8 h-px mx-3 transition-colors ${
									step < props.currentStep ? "bg-[#ff3e00]" : "bg-[#333]"
								}`}
							/>
						)}
					</div>
				)}
			</For>
		</div>
	);
}
