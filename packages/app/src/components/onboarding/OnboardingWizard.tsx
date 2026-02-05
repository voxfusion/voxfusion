import { createSignal, Show } from "solid-js";
import { useI18n } from "../../i18n";
import { updateOnboardingStep } from "../../lib/settingsStore";
import StepIndicator from "./StepIndicator";
import MicrophonePermissionStep from "./steps/MicrophonePermissionStep";
import AccessibilityPermissionStep from "./steps/AccessibilityPermissionStep";
import MicrophoneStep from "./steps/MicrophoneStep";
import HotkeyStep from "./steps/HotkeyStep";
import LanguageStep from "./steps/LanguageStep";
import CompletionStep from "./steps/CompletionStep";

interface OnboardingWizardProps {
	initialStep: number;
	onComplete: () => void;
}

const TOTAL_STEPS = 6;

export default function OnboardingWizard(props: OnboardingWizardProps) {
	const [t] = useI18n();
	const [currentStep, setCurrentStep] = createSignal(props.initialStep);
	const [isAnimating, setIsAnimating] = createSignal(false);
	const [animationDirection, setAnimationDirection] = createSignal<"forward" | "backward">(
		"forward"
	);
	const [micPermissionGranted, setMicPermissionGranted] = createSignal(false);
	const [accessibilityPermissionGranted, setAccessibilityPermissionGranted] = createSignal(false);

	const goToNext = () => {
		if (currentStep() < TOTAL_STEPS) {
			setAnimationDirection("forward");
			setIsAnimating(true);
			setTimeout(() => {
				const nextStep = currentStep() + 1;
				setCurrentStep(nextStep);
				setIsAnimating(false);
				updateOnboardingStep(nextStep);
			}, 200);
		}
	};

	const goToPrevious = () => {
		if (currentStep() > 1) {
			setAnimationDirection("backward");
			setIsAnimating(true);
			setTimeout(() => {
				const prevStep = currentStep() - 1;
				setCurrentStep(prevStep);
				setIsAnimating(false);
				updateOnboardingStep(prevStep);
			}, 200);
		}
	};

	const handleComplete = () => {
		props.onComplete();
	};

	const canProceed = () => {
		const step = currentStep();
		if (step === 2) return micPermissionGranted();
		if (step === 3) return accessibilityPermissionGranted();
		return true;
	};

	const formatStep = (step: number) => step.toString().padStart(2, "0");

	return (
		<div class="fixed inset-0 bg-[#0a0a0a] flex flex-col items-center justify-center p-8">
			{/* Grid overlay background */}
			<div
				class="absolute inset-0 opacity-[0.03] pointer-events-none"
				style={{
					"background-image":
						"linear-gradient(#fff 1px, transparent 1px), linear-gradient(90deg, #fff 1px, transparent 1px)",
					"background-size": "40px 40px",
				}}
			/>

			<div class="absolute top-0 left-0 right-0 h-6 z-50" data-tauri-drag-region />

			{/* Progress fraction */}
			<div class="absolute top-8 right-8 font-mono text-[#666] text-sm tracking-wider">
				{formatStep(currentStep())}/{formatStep(TOTAL_STEPS)}
			</div>

			<div class="mb-12">
				<StepIndicator currentStep={currentStep()} totalSteps={TOTAL_STEPS} />
			</div>

			<div class="flex-1 flex items-center justify-center w-full max-w-2xl">
				<div
					class={`w-full transition-all duration-200 ${
						isAnimating()
							? animationDirection() === "forward"
								? "opacity-0 translate-x-8"
								: "opacity-0 -translate-x-8"
							: "opacity-100 translate-x-0"
					}`}
				>
					<Show when={currentStep() === 1}>
						<LanguageStep />
					</Show>
					<Show when={currentStep() === 2}>
						<MicrophonePermissionStep onPermissionChange={setMicPermissionGranted} />
					</Show>
					<Show when={currentStep() === 3}>
						<AccessibilityPermissionStep onPermissionChange={setAccessibilityPermissionGranted} />
					</Show>
					<Show when={currentStep() === 4}>
						<MicrophoneStep />
					</Show>
					<Show when={currentStep() === 5}>
						<HotkeyStep />
					</Show>
					<Show when={currentStep() === 6}>
						<CompletionStep />
					</Show>
				</div>
			</div>

			<div class="flex items-center justify-between w-full max-w-2xl mt-8">
				<Show when={currentStep() > 1} fallback={<div />}>
					<button
						type="button"
						onClick={goToPrevious}
						class="px-6 py-3 font-mono text-[#888] hover:text-[#e0e0e0] transition-colors uppercase tracking-wider text-sm border border-[#333] hover:border-[#ff3e00]"
					>
						{t("onboarding.back")}
					</button>
				</Show>

				<Show
					when={currentStep() < TOTAL_STEPS}
					fallback={
						<button
							type="button"
							onClick={handleComplete}
							class="px-8 py-3 bg-[#ff3e00] text-black font-mono font-bold uppercase tracking-wider text-sm hover:bg-[#ff5722] transition-colors"
						>
							{t("onboarding.getStarted")}
						</button>
					}
				>
					<button
						type="button"
						onClick={goToNext}
						disabled={!canProceed()}
						class="px-8 py-3 bg-[#ff3e00] text-black font-mono font-bold uppercase tracking-wider text-sm hover:bg-[#ff5722] transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
					>
						{t("onboarding.next")}
					</button>
				</Show>
			</div>
		</div>
	);
}
