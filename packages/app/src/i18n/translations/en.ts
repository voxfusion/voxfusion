export type Translations = {
	auth: {
		welcome: string;
		signInToContinue: string;
		continueWithGoogle: string;
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
		spanish: string;
		chinese: string;
		japanese: string;
		korean: string;
		german: string;
		french: string;
		italian: string;
		swedish: string;
		hindi: string;
		ukrainian: string;
		audio: string;
		hotkey: string;
		appearance: string;
		microphone: string;
		defaultMicrophone: string;
		microphoneDescription: string;
		audioQuality: string;
		audioQualityDescription: string;
		audioQualityHigh: string;
		audioQualityHighDescription: string;
		audioQualityMedium: string;
		audioQualityMediumDescription: string;
		audioQualityLow: string;
		audioQualityLowDescription: string;
		recordingHotkey: string;
		pressHotkey: string;
		hotkeyDescription: string;
		holdToSpeakHotkeyDescription: string;
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
		wordsUsed: string;
		limitReached: string;
		proPlan: string;
		unlimited: string;
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
		handsFreeHotkey: string;
		holdToSpeakHotkey: string;
		recordHotkey: string;
		recordHoldToSpeakHotkey: string;
		pressKeys: string;
		languageTitle: string;
		languageDescription: string;
		completionTitle: string;
		completionDescription: string;
		micPermissionTitle: string;
		micPermissionDescription: string;
		micPermissionGranted: string;
		micPermissionNotGranted: string;
		grantMicPermission: string;
		checkingPermission: string;
		accessibilityTitle: string;
		accessibilityDescription: string;
		accessibilityGranted: string;
		accessibilityNotGranted: string;
		openSystemPreferences: string;
		accessibilityInstructions: string;
		learningTitle: string;
		learningDescription: string;
		learningStep1Prefix: string;
		learningStep1Suffix: string;
		learningStep2: string;
		learningStep3Prefix: string;
		learningStep3Suffix: string;
		learningStep4: string;
		learningHoldToSpeakPrefix: string;
		learningHoldToSpeakSuffix: string;
		learningReady: string;
		learningRecording: string;
		learningProcessing: string;
		learningPlaceholder: string;
		learningError: string;
	};
	update: {
		available: string;
		newVersion: string;
		downloading: string;
		ignore: string;
		downloadAndRestart: string;
	};
};

export const en: Translations = {
	auth: {
		welcome: "Welcome to VoxFusion",
		signInToContinue: "Sign in to continue",
		continueWithGoogle: "Continue with Google",
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
		spanish: "Spanish",
		chinese: "Chinese",
		japanese: "Japanese",
		korean: "Korean",
		german: "German",
		french: "French",
		italian: "Italian",
		swedish: "Swedish",
		hindi: "Hindi",
		ukrainian: "Ukrainian",
		audio: "Audio",
		hotkey: "Hotkey",
		appearance: "Appearance",
		microphone: "Microphone",
		defaultMicrophone: "System Default",
		microphoneDescription: "Select the microphone to use for voice recording.",
		audioQuality: "Audio Quality",
		audioQualityDescription:
			"Lower quality reduces file size for faster uploads and transcriptions.",
		audioQualityHigh: "High",
		audioQualityHighDescription: "Original quality, largest file size",
		audioQualityMedium: "Medium",
		audioQualityMediumDescription: "16kHz mono, ~10x smaller",
		audioQualityLow: "Low",
		audioQualityLowDescription: "8kHz mono, ~20x smaller",
		recordingHotkey: "Recording Hotkey",
		pressHotkey: "Press a key combination...",
		hotkeyDescription: "Press this key combination to start or stop recording.",
		holdToSpeakHotkeyDescription: "Hold this key combination to record, then release it to stop.",
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
		useCommandToRecord: "Use Command+; to start recording and create your first transcription",
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
		wordsUsed: "Words used",
		limitReached: "Limit reached",
		proPlan: "Pro plan",
		unlimited: "Unlimited",
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
			"Set a hands-free shortcut and an optional hold-to-speak shortcut for quick dictation.",
		currentHotkey: "Current hotkey",
		handsFreeHotkey: "Hands-free shortcut",
		holdToSpeakHotkey: "Hold-to-speak shortcut",
		recordHotkey: "Record New Hotkey",
		recordHoldToSpeakHotkey: "Record Hold-to-Speak Hotkey",
		pressKeys: "Press your desired key combination...",
		languageTitle: "Choose Your Language",
		languageDescription: "Select your preferred language for the interface.",
		completionTitle: "You're All Set!",
		completionDescription: "VoxFusion is ready to help you with voice transcription.",
		micPermissionTitle: "Microphone Access",
		micPermissionDescription:
			"VoxFusion needs access to your microphone to record and transcribe your voice.",
		micPermissionGranted: "Microphone access granted",
		micPermissionNotGranted: "Microphone access required",
		grantMicPermission: "Grant Microphone Access",
		checkingPermission: "Checking...",
		accessibilityTitle: "Accessibility Access",
		accessibilityDescription:
			"VoxFusion needs accessibility access to type transcribed text into any application.",
		accessibilityGranted: "Accessibility access granted",
		accessibilityNotGranted: "Accessibility access required",
		openSystemPreferences: "Open System Preferences",
		accessibilityInstructions:
			"Click the lock icon to make changes, then check VoxFusion in the list.",
		learningTitle: "Try It Out",
		learningDescription: "Test the voice-to-text flow before you start.",
		learningStep1Prefix: "Press",
		learningStep1Suffix: "to start recording",
		learningStep2: "Speak into your microphone",
		learningStep3Prefix: "Press",
		learningStep3Suffix: "again to stop",
		learningStep4: "See your transcription appear",
		learningHoldToSpeakPrefix: "Or hold",
		learningHoldToSpeakSuffix: "while speaking, then release to stop",
		learningReady: "Ready",
		learningRecording: "Recording",
		learningProcessing: "Processing",
		learningPlaceholder: "Your transcriptions will appear here",
		learningError: "Transcription failed. Try again.",
	},
	update: {
		available: "Update Available",
		newVersion: "Version",
		downloading: "Downloading update...",
		ignore: "Ignore",
		downloadAndRestart: "Download & Restart",
	},
};
