export const MONTHLY_TRANSCRIPTION_WORD_LIMITS = {
	free: 100_000,
	pro: 1_000_000,
} as const;

export type SubscriptionPlan = keyof typeof MONTHLY_TRANSCRIPTION_WORD_LIMITS;
