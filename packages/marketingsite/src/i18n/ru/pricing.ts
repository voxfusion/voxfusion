import { MONTHLY_TRANSCRIPTION_WORD_LIMITS } from "@voxfusion/shared/subscriptionPlans";

const freePlanWordLimit = MONTHLY_TRANSCRIPTION_WORD_LIMITS.free.toLocaleString("en-US");

export const pricing = {
	"pricing.tag": "Pricing",
	"pricing.title": "Start free, upgrade when you need more",
	"pricing.description": "No card required. Cancel anytime.",
	"pricing.free.name": "Free",
	"pricing.free.price": "$0",
	"pricing.free.period": "forever",
	"pricing.free.description": "Perfect for getting started with voice dictation",
	"pricing.free.feature1": `${freePlanWordLimit} words per month`,
	"pricing.free.feature2": "Accurate transcription",
	"pricing.free.feature3": "Custom dictionary (50 terms)",
	"pricing.free.feature4": "Transcription history",
	"pricing.free.feature5": "English and Russian",
	"pricing.free.cta": "Start free",
	"pricing.pro.badge": "Alpha",
	"pricing.pro.name": "Pro",
	"pricing.pro.price": "$0",
	"pricing.pro.period": "during alpha",
	"pricing.pro.description": "Free during alpha. Later it will be about $5/month.",
	"pricing.pro.feature1": "Unlimited",
	"pricing.pro.feature1.suffix": " transcription",
	"pricing.pro.feature2": "Everything in Free",
	"pricing.pro.feature3": "Priority processing",
	"pricing.pro.feature4": "Priority support",
	"pricing.pro.cta": "Start Pro",
	"pricing.faq": "Questions? Email",
	"pricing.faq.suffix": " and we will reply within 24 hours.",

	"cta.title": "Ready to type with your voice?",
	"cta.description": "Download VoxFusion for free and start dictating in seconds.",
	"cta.button": "Download for Mac",
} as const;
