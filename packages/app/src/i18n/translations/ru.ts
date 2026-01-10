import type { Translations } from "./en";

export const ru: Translations = {
	auth: {
		welcome: "Добро пожаловать в VoxFusion",
		signInToContinue: "Войдите, чтобы продолжить",
		continueWithGoogle: "Продолжить с Google",
		googleLogo: "Логотип Google",
		devPasteToken: "Разработка: Вставьте токен для аутентификации",
		pasteTokenPlaceholder: "Вставьте токен сюда",
		authenticate: "Аутентификация",
	},
	home: {
		yourTranscriptions: "Ваши транскрипции",
		pressToRecord: "Нажмите Command+; чтобы начать запись",
	},
	about: {
		title: "О приложении",
		welcomeDescription:
			"Добро пожаловать в VoxFusion - современное десктопное приложение, созданное с SolidJS и Tauri.",
		navigationDescription:
			"Это приложение демонстрирует навигацию по страницам и функциональность маршрутизации.",
	},
	settings: {
		title: "Настройки",
		theme: "Тема",
		dark: "Тёмная",
		light: "Светлая",
		auto: "Авто",
		currentTheme: "Текущая тема:",
		language: "Язык",
		english: "Английский",
		russian: "Русский",
	},
	transcription: {
		notAvailable: "Н/Д",
		processing: "Обработка:",
		duration: "Длительность:",
		copy: "Копировать",
		copied: "Скопировано!",
		goodTranscription: "Хорошая транскрипция",
		poorTranscription: "Плохая транскрипция",
	},
	transcriptionList: {
		failedToFetch: "Не удалось загрузить транскрипции",
		errorOccurred: "Произошла ошибка",
		noTranscriptions: "Пока нет транскрипций",
		useCommandToRecord:
			"Используйте Command+; чтобы начать запись и создать первую транскрипцию",
		tryAgain: "Попробовать снова",
		noMore: "Больше нет транскрипций",
		today: "Сегодня",
		yesterday: "Вчера",
	},
	sidebar: {
		home: "Главная",
		dictionary: "Словарь",
		account: "Аккаунт",
		settings: "Настройки",
		privacy: "Приватность",
		logout: "Выйти",
	},
	dictionary: {
		title: "Словарь",
		description: "Добавьте пользовательские слова для улучшения точности транскрипции",
		addWord: "Добавить",
		wordPlaceholder: "Введите слово или фразу...",
		save: "Сохранить",
		cancel: "Отмена",
		edit: "Редактировать",
		delete: "Удалить",
		emptyState: "Пока нет слов",
		emptyStateDescription:
			"Добавьте слова для улучшения точности транскрипции специальных терминов, имён или часто используемых фраз.",
		wordCount: "{count} слов",
	},
};
