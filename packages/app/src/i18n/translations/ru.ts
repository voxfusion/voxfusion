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
		system: "Системная",
		currentTheme: "Текущая тема:",
		language: "Язык",
		english: "Английский",
		russian: "Русский",
		audio: "Аудио",
		hotkey: "Горячая клавиша",
		appearance: "Внешний вид",
		microphone: "Микрофон",
		defaultMicrophone: "Системный по умолчанию",
		microphoneDescription: "Выберите микрофон для записи голоса.",
		recordingHotkey: "Горячая клавиша записи",
		pressHotkey: "Нажмите комбинацию клавиш...",
		hotkeyDescription: "Нажмите эту комбинацию клавиш для начала или остановки записи.",
		change: "Изменить",
		cancel: "Отмена",
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
		comingSoon: "Скоро",
		comingSoonDescription:
			"Функция словаря позволит вам добавлять пользовательские слова и фразы, которые вы хотите транскрибировать правильно.",
	},
};
