import { createSignal, Show } from "solid-js";
import { useI18n } from "../../i18n";
import StepIndicator from "./StepIndicator";
import MicrophonePermissionStep from "./steps/MicrophonePermissionStep";
import AccessibilityPermissionStep from "./steps/AccessibilityPermissionStep";
import MicrophoneStep from "./steps/MicrophoneStep";
import HotkeyStep from "./steps/HotkeyStep";
import LanguageStep from "./steps/LanguageStep";
import CompletionStep from "./steps/CompletionStep";

interface OnboardingWizardProps {
	onComplete: () => void;
}

const TOTAL_STEPS = 6;

export default function OnboardingWizard(props: OnboardingWizardProps) {
	const [t] = useI18n();
	const [currentStep, setCurrentStep] = createSignal(1);
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
				setCurrentStep(currentStep() + 1);
				setIsAnimating(false);
			}, 200);
		}
	};

	const goToPrevious = () => {
		if (currentStep() > 1) {
			setAnimationDirection("backward");
			setIsAnimating(true);
			setTimeout(() => {
				setCurrentStep(currentStep() - 1);
				setIsAnimating(false);
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

	return (
		<div class="fixed inset-0 bg-slate-100 dark:bg-midnight-900 flex flex-col items-center justify-center p-8">
			{/* Drag region for window */}
			<div class="absolute top-0 left-0 right-0 h-6 z-50" data-tauri-drag-region />

			{/* Step Indicator */}
			<div class="mb-12">
				<StepIndicator currentStep={currentStep()} totalSteps={TOTAL_STEPS} />
			</div>

			{/* Step Content */}
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

			{/* Navigation */}
			<div class="flex items-center justify-between w-full max-w-2xl mt-8">
				<Show when={currentStep() > 1} fallback={<div />}>
					<button
						type="button"
						onClick={goToPrevious}
						class="px-6 py-3 text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-white transition-colors font-medium"
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
							class="px-8 py-3 bg-primary-500 text-white rounded-xl font-medium hover:bg-primary-600 transition-colors"
						>
							{t("onboarding.getStarted")}
						</button>
					}
				>
					<button
						type="button"
						onClick={goToNext}
						disabled={!canProceed()}
						class="px-8 py-3 bg-primary-500 text-white rounded-xl font-medium hover:bg-primary-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
					>
						{t("onboarding.next")}
					</button>
				</Show>
			</div>
		</div>
	);
}
