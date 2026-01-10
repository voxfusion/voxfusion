export type Translations = {
	auth: {
		welcome: string;
		signInToContinue: string;
		continueWithGoogle: string;
		googleLogo: string;
		devPasteToken: string;
		pasteTokenPlaceholder: string;
		authenticate: string;
	};
	home: {
		yourTranscriptions: string;
		pressToRecord: string;
	};
	about: {
		title: string;
		welcomeDescription: string;
		navigationDescription: string;
	};
	settings: {
		title: string;
		theme: string;
		dark: string;
		light: string;
		auto: string;
		currentTheme: string;
		language: string;
		english: string;
		russian: string;
	};
	transcription: {
		notAvailable: string;
		processing: string;
		duration: string;
		copy: string;
		copied: string;
		goodTranscription: string;
		poorTranscription: string;
	};
	transcriptionList: {
		failedToFetch: string;
		errorOccurred: string;
		noTranscriptions: string;
		useCommandToRecord: string;
		tryAgain: string;
		noMore: string;
	};
	sidebar: {
		home: string;
		dictionary: string;
		account: string;
		settings: string;
		privacy: string;
		logout: string;
	};
	dictionary: {
		title: string;
		description: string;
		addWord: string;
		wordPlaceholder: string;
		save: string;
		cancel: string;
		edit: string;
		delete: string;
		emptyState: string;
		emptyStateDescription: string;
		wordCount: string;
	};
};

export const en: Translations = {
	auth: {
		welcome: "Welcome to VoxFusion",
		signInToContinue: "Sign in to continue",
		continueWithGoogle: "Continue with Google",
		googleLogo: "Google logo",
		devPasteToken: "Development: Paste token to authenticate",
		pasteTokenPlaceholder: "Paste token here",
		authenticate: "Authenticate",
	},
	home: {
		yourTranscriptions: "Your Transcriptions",
		pressToRecord: "Press Command+; to start a new recording",
	},
	about: {
		title: "About",
		welcomeDescription:
			"Welcome to VoxFusion - a modern desktop application built with SolidJS and Tauri.",
		navigationDescription:
			"This application demonstrates page navigation and routing functionality.",
	},
	settings: {
		title: "Settings",
		theme: "Theme",
		dark: "Dark",
		light: "Light",
		auto: "Auto",
		currentTheme: "Current theme:",
		language: "Language",
		english: "English",
		russian: "Russian",
	},
	transcription: {
		notAvailable: "N/A",
		processing: "Processing:",
		duration: "Duration:",
		copy: "Copy",
		copied: "Copied!",
		goodTranscription: "Good transcription",
		poorTranscription: "Poor transcription",
	},
	transcriptionList: {
		failedToFetch: "Failed to fetch transcriptions",
		errorOccurred: "An error occurred",
		noTranscriptions: "No transcriptions yet",
		useCommandToRecord:
			"Use Command+; to start recording and create your first transcription",
		tryAgain: "Try again",
		noMore: "No more transcriptions",
	},
	sidebar: {
		home: "Home",
		dictionary: "Dictionary",
		account: "Account",
		settings: "Settings",
		privacy: "Privacy",
		logout: "Log out",
	},
	dictionary: {
		title: "Dictionary",
		description: "Add custom words for better transcription accuracy",
		addWord: "Add",
		wordPlaceholder: "Enter a word or phrase...",
		save: "Save",
		cancel: "Cancel",
		edit: "Edit",
		delete: "Delete",
		emptyState: "No words yet",
		emptyStateDescription:
			"Add words to improve transcription accuracy for specialized terms, names, or phrases you use often.",
		wordCount: "{count} words",
	},
};
