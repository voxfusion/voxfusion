export type Translations = {
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
		hotkeys: string;
		appearance: string;
		microphone: string;
		defaultMicrophone: string;
		microphoneDescription: string;
		muteMediaWhileRecording: string;
		muteMediaWhileRecordingDescription: string;
		recordingHotkey: string;
		pressHotkey: string;
		hotkeyDescription: string;
		holdToSpeakHotkeyDescription: string;
		change: string;
		cancel: string;
		models: string;
		modelsDescription: string;
		modelInUse: string;
		modelUse: string;
		modelRecommended: string;
		modelExperimental: string;
		modelDownloaded: string;
		modelDownload: string;
		modelDownloading: string;
		modelDownloadFailed: string;
		modelExperimentalNote: string;
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
		style: string;
		account: string;
		settings: string;
		privacy: string;
	};
	style: {
		defaultStyleTitle: string;
		defaultStyleDescription: string;
		perAppTitle: string;
		tabDefault: string;
		tabPerApp: string;
		tabSites: string;
		perSiteDescription: string;
		perSiteEmptyState: string;
		perSiteEmptyStateDescription: string;
		descriptions: {
			professional: string;
			casual: string;
			agents: string;
			default: string;
		};
	};
	appInstructions: {
		description: string;
		searchPlaceholder: string;
		appCount: string;
		emptyState: string;
		emptyStateDescription: string;
		delete: string;
		noAppsDetected: string;
		noMatches: string;
		styles: {
			professional: string;
			casual: string;
			agents: string;
			default: string;
		};
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
		defaultSectionTitle: string;
		perAppTitle: string;
		perAppDescription: string;
		perAppEmptyState: string;
		perAppEmptyStateDescription: string;
		expand: string;
		collapse: string;
		tabDefault: string;
		tabPerApp: string;
		tabSites: string;
		sitesDescription: string;
		sitesEmptyState: string;
		sitesEmptyStateDescription: string;
		sitesDomainPlaceholder: string;
		sitesAddSite: string;
		sitesUseCurrent: string;
		sitesInvalidDomain: string;
		sitesNoCurrentSite: string;
		siteCount: string;
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
		modelDownloadTitle: string;
		modelDownloadDescription: string;
		modelDownloadComplete: string;
		modelDownloading: string;
		modelSize: string;
		downloadModel: string;
		retryDownload: string;
		modelDownloadNote: string;
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
	home: {
		yourTranscriptions: "Your Transcriptions",
		pressToRecord: "Press Left Control+Left Option to start a new recording",
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
		hotkeys: "Hotkeys",
		appearance: "Appearance",
		microphone: "Microphone",
		defaultMicrophone: "System Default",
		microphoneDescription: "Select the microphone to use for voice recording.",
		muteMediaWhileRecording: "Mute Media During Recording",
		muteMediaWhileRecordingDescription:
			"Temporarily mute system audio while recording and restore it afterward.",
		recordingHotkey: "Recording Hotkey",
		pressHotkey: "Press a key combination...",
		hotkeyDescription: "Press this key combination to start or stop recording.",
		holdToSpeakHotkeyDescription: "Hold this key combination to record, then release it to stop.",
		change: "Change",
		cancel: "Cancel",
		models: "Models",
		modelsDescription:
			"Choose the speech-to-text model used for transcription. Download additional models to switch between them.",
		modelInUse: "In use",
		modelUse: "Use",
		modelRecommended: "Recommended",
		modelExperimental: "Experimental",
		modelDownloaded: "Downloaded",
		modelDownload: "Download",
		modelDownloading: "Downloading…",
		modelDownloadFailed: "Download failed",
		modelExperimentalNote:
			"On-device transcription with this model is experimental and not yet available.",
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
			"Use Left Control+Left Option to start recording and create your first transcription",
		tryAgain: "Try again",
		noMore: "No more transcriptions",
		today: "Today",
		yesterday: "Yesterday",
	},
	sidebar: {
		home: "Home",
		dictionary: "Dictionary",
		style: "Style",
		account: "Account",
		settings: "Settings",
		privacy: "Privacy",
	},
	style: {
		defaultStyleTitle: "Default Style",
		defaultStyleDescription:
			"Used everywhere except for apps or sites configured on the other tabs. Switch between styles to preview what each one does.",
		perAppTitle: "Per-App Override",
		tabDefault: "Default",
		tabPerApp: "Per-App",
		tabSites: "Per-Site",
		perSiteDescription:
			"When you're focused on a browser tab, this style overrides the per-app and default style.",
		perSiteEmptyState: "No sites configured",
		perSiteEmptyStateDescription:
			"Add a domain or capture the current browser tab, then pick a style for it.",
		descriptions: {
			professional:
				"Formal business communication. Complete sentences, precise grammar and punctuation, no slang or contractions. Best for email, documents, reports, and formal messaging.",
			casual:
				"Conversational tone with contractions and everyday vocabulary. Sparse punctuation, lowercase throughout. Best for chat apps, personal notes, and quick messages.",
			agents:
				"Technical instructions for coding agents. Imperative voice, software engineering vocabulary, file paths and identifiers preserved. Best for prompting Claude Code, Cursor, and similar tools.",
			default:
				"Clean dictation with standard sentence structure, accurate punctuation, and proper capitalization. No specific tone applied — a neutral baseline.",
		},
	},
	appInstructions: {
		description: "Apps listed here override the default style above.",
		searchPlaceholder: "Search installed applications...",
		appCount: "{count} apps",
		emptyState: "No apps configured",
		emptyStateDescription:
			"Search above to add an app and choose a transcription style for it.",
		delete: "Remove",
		noAppsDetected: "No installed applications detected",
		noMatches: "No matching applications",
		styles: {
			professional: "Professional",
			casual: "Casual",
			agents: "Agents",
			default: "Default",
		},
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
		defaultSectionTitle: "Default Dictionary",
		perAppTitle: "Per-App Dictionary",
		perAppDescription:
			"When an app is focused, its words are used alongside the default dictionary above.",
		perAppEmptyState: "No apps configured",
		perAppEmptyStateDescription:
			"Search above to add an app, then expand it to add custom words for that app.",
		expand: "Expand",
		collapse: "Collapse",
		tabDefault: "Default",
		tabPerApp: "Per-App",
		tabSites: "Per-Site",
		sitesDescription:
			"When you're focused on a browser tab, that site's words are used alongside the default dictionary.",
		sitesEmptyState: "No sites configured",
		sitesEmptyStateDescription:
			"Add a domain or capture the current browser tab, then expand it to add custom words for that site.",
		sitesDomainPlaceholder: "example.com",
		sitesAddSite: "Add Site",
		sitesUseCurrent: "Use current site",
		sitesInvalidDomain: "Enter a valid domain",
		sitesNoCurrentSite: "No browser site detected",
		siteCount: "{count} sites",
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
		modelDownloadTitle: "Download Whisper Model",
		modelDownloadDescription:
			"Download the Whisper V3 Large Turbo model for local, offline transcription.",
		modelDownloadComplete: "Model downloaded successfully",
		modelDownloading: "Downloading...",
		modelSize: "Model size",
		downloadModel: "Download Model",
		retryDownload: "Retry Download",
		modelDownloadNote:
			"This model enables offline transcription. Download requires internet connection.",
	},
	update: {
		available: "Update Available",
		newVersion: "Version",
		downloading: "Downloading update...",
		ignore: "Ignore",
		downloadAndRestart: "Download & Restart",
	},
};
