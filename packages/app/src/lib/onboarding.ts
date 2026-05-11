export const ONBOARDING_STEP_COUNT = 7;
export const CURRENT_ONBOARDING_VERSION = 3;
export const MODEL_DOWNLOAD_STEP = 5;

export function normalizeOnboardingStep(
	step: number,
	onboardingComplete: boolean,
	onboardingVersion: number
): number {
	if (onboardingComplete) {
		return 1;
	}

	if (onboardingVersion < CURRENT_ONBOARDING_VERSION) {
		return 1;
	}

	return Math.min(Math.max(step, 1), ONBOARDING_STEP_COUNT);
}
