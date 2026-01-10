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
		system: string;
		currentTheme: string;
		language: string;
		english: string;
		russian: string;
		audio: string;
		hotkey: string;
		appearance: string;
		microphone: string;
		defaultMicrophone: string;
		microphoneDescription: string;
		recordingHotkey: string;
		pressHotkey: string;
		hotkeyDescription: string;
		change: string;
		cancel: string;
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
		today: string;
		yesterday: string;
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
	onboarding: {
		step: string;
		of: string;
		next: string;
		back: string;
		getStarted: string;
		microphoneTitle: string;
		microphoneDescription: string;
		selectMicrophone: string;
		refreshDevices: string;
		hotkeyTitle: string;
		hotkeyDescription: string;
		currentHotkey: string;
		recordHotkey: string;
		pressKeys: string;
		languageTitle: string;
		languageDescription: string;
		completionTitle: string;
		completionDescription: string;
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
		system: "System",
		currentTheme: "Current theme:",
		language: "Language",
		english: "English",
		russian: "Russian",
		audio: "Audio",
		hotkey: "Hotkey",
		appearance: "Appearance",
		microphone: "Microphone",
		defaultMicrophone: "System Default",
		microphoneDescription: "Select the microphone to use for voice recording.",
		recordingHotkey: "Recording Hotkey",
		pressHotkey: "Press a key combination...",
		hotkeyDescription: "Press this key combination to start or stop recording.",
		change: "Change",
		cancel: "Cancel",
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
		today: "Today",
		yesterday: "Yesterday",
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
	onboarding: {
		step: "Step",
		of: "of",
		next: "Next",
		back: "Back",
		getStarted: "Get Started",
		microphoneTitle: "Select Your Microphone",
		microphoneDescription:
			"Choose which microphone VoxFusion should use for voice recording. You can change this later in Settings.",
		selectMicrophone: "Select microphone",
		refreshDevices: "Refresh devices",
		hotkeyTitle: "Set Your Hotkey",
		hotkeyDescription:
			"This keyboard shortcut will start and stop voice recording from anywhere on your Mac.",
		currentHotkey: "Current hotkey",
		recordHotkey: "Record New Hotkey",
		pressKeys: "Press your desired key combination...",
		languageTitle: "Choose Your Language",
		languageDescription: "Select your preferred language for the interface.",
		completionTitle: "You're All Set!",
		completionDescription: "VoxFusion is ready to help you with voice transcription.",
	},
};
