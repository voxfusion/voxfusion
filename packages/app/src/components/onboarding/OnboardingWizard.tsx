import { Show, createSignal } from "solid-js";
import { useI18n } from "../../i18n";
import { capture } from "../../lib/posthog";
import { updateOnboardingStep } from "../../lib/settingsStore";
import StepIndicator from "./StepIndicator";
import AccessibilityPermissionStep from "./steps/AccessibilityPermissionStep";
import CompletionStep from "./steps/CompletionStep";
import HotkeyStep from "./steps/HotkeyStep";
import LearningStep from "./steps/LearningStep";
import MicrophonePermissionStep from "./steps/MicrophonePermissionStep";
import MicrophoneStep from "./steps/MicrophoneStep";

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
	const [learningCompleted, setLearningCompleted] = createSignal(false);

	const goToNext = () => {
		if (currentStep() < TOTAL_STEPS) {
			capture("onboarding_step_completed", { step: currentStep() });
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
		if (step === 1) return micPermissionGranted();
		if (step === 2) return accessibilityPermissionGranted();
		if (step === 5) return learningCompleted();
		return true;
	};

	const formatStep = (step: number) => step.toString().padStart(2, "0");

	return (
		<div class="fixed inset-0 bg-th-base flex flex-col items-center justify-center p-8">
			{/* Grid overlay background */}
			<div
				class="absolute inset-0 pointer-events-none"
				style={{
					"background-image":
						"linear-gradient(var(--color-grid-line) 1px, transparent 1px), linear-gradient(90deg, var(--color-grid-line) 1px, transparent 1px)",
					"background-size": "40px 40px",
				}}
			/>

			<div class="absolute top-0 left-0 right-0 h-6 z-50" data-tauri-drag-region />

			{/* Progress fraction */}
			<div class="absolute top-8 right-8 font-mono text-txt-muted text-sm tracking-wider">
				{formatStep(currentStep())}/{formatStep(TOTAL_STEPS)}
			</div>

			<div class="mb-12">
				<StepIndicator currentStep={currentStep()} totalSteps={TOTAL_STEPS} />
			</div>

			<div
				class={`flex-1 flex items-center justify-center w-full transition-all duration-300 ${currentStep() === 5 ? "max-w-4xl" : "max-w-2xl"}`}
			>
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
						<MicrophonePermissionStep onPermissionChange={setMicPermissionGranted} />
					</Show>
					<Show when={currentStep() === 2}>
						<AccessibilityPermissionStep onPermissionChange={setAccessibilityPermissionGranted} />
					</Show>
					<Show when={currentStep() === 3}>
						<MicrophoneStep />
					</Show>
					<Show when={currentStep() === 4}>
						<HotkeyStep />
					</Show>
					<Show when={currentStep() === 5}>
						<LearningStep onTranscriptionComplete={() => setLearningCompleted(true)} />
					</Show>
					<Show when={currentStep() === 6}>
						<CompletionStep />
					</Show>
				</div>
			</div>

			<div
				class={`flex items-center justify-between w-full mt-8 transition-all duration-300 ${currentStep() === 5 ? "max-w-4xl" : "max-w-2xl"}`}
			>
				<Show when={currentStep() > 1} fallback={<div />}>
					<button
						type="button"
						onClick={goToPrevious}
						class="px-6 py-3 font-mono text-txt-secondary hover:text-txt-primary transition-colors uppercase tracking-wider text-sm border border-border-strong hover:border-ac"
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
							class="px-8 py-3 bg-ac text-ac-on font-mono font-bold uppercase tracking-wider text-sm hover:bg-ac-hover transition-colors"
						>
							{t("onboarding.getStarted")}
						</button>
					}
				>
					<button
						type="button"
						onClick={goToNext}
						disabled={!canProceed()}
						class="px-8 py-3 bg-ac text-ac-on font-mono font-bold uppercase tracking-wider text-sm hover:bg-ac-hover transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
					>
						{t("onboarding.next")}
					</button>
				</Show>
			</div>
		</div>
	);
}
