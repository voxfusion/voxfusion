import { MONTHLY_TRANSCRIPTION_WORD_LIMITS } from "@voxfusion/shared/subscriptionPlans";

const freePlanWordLimit = MONTHLY_TRANSCRIPTION_WORD_LIMITS.free.toLocaleString("ru-RU");

export const pricing = {
	"pricing.tag": "Цены",
	"pricing.title": "Начните бесплатно, обновляйтесь по необходимости",
	"pricing.description":
		"Карта не требуется. Отмена в любое время.",
	"pricing.free.name": "Бесплатно",
	"pricing.free.price": "0 ₽",
	"pricing.free.period": "навсегда",
	"pricing.free.description":
		"Идеально для знакомства с голосовой диктовкой",
	"pricing.free.feature1": `${freePlanWordLimit} слов в месяц`,
	"pricing.free.feature2": "Точная транскрипция",
	"pricing.free.feature3": "Свой словарь (50 терминов)",
	"pricing.free.feature4": "История транскрипций",
	"pricing.free.feature5": "Английский и русский",
	"pricing.free.cta": "Начать бесплатно",
	"pricing.pro.badge": "Популярный",
	"pricing.pro.name": "Pro",
	"pricing.pro.price": "300 ₽",
	"pricing.pro.period": "в месяц",
	"pricing.pro.description":
		"Для тех, кто диктует каждый день",
	"pricing.pro.feature1": "Безлимитная",
	"pricing.pro.feature1.suffix": " транскрипция",
	"pricing.pro.feature2": "Все из бесплатного тарифа",
	"pricing.pro.feature3": "Приоритетная обработка",
	"pricing.pro.feature4": "Приоритетная поддержка",
	"pricing.pro.cta": "Начать Pro",
	"pricing.faq": "Вопросы? Напишите на",
	"pricing.faq.suffix": "- отвечаем в течение 24 часов.",

	"cta.title": "Готовы печатать голосом?",
	"cta.description":
		"Скачайте VoxFusion бесплатно. Начните диктовать за секунды.",
	"cta.button": "Скачать для Mac",
} as const;
