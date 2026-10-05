export const faq = {
	"faq.tag": "FAQ",
	"faq.title": "Questions, answered.",
	"faq.contact.github": "Open an issue",
	"faq.item1.question": "Is VoxFusion really free?",
	"faq.item1.answer":
		"Yes. Free and open source under the MIT license. No subscription, no trial, no account.",
	"faq.item2.question": "How is it different from the dictation built into macOS?",
	"faq.item2.answer":
		"It runs Whisper Large v3 Turbo, spells the names and jargon you teach it, matches its tone to the app you're in, and keeps a history of what you dictated.",
	"faq.item3.question": "Will it work in the app I use?",
	"faq.item3.answer":
		"If you can type in it, you can dictate into it. VoxFusion types at your cursor the way a keyboard would, so it needs no plugins.",
	"faq.item4.question": "Does it work without internet?",
	"faq.item4.answer":
		"Yes, after a one-time model download: about 1.5 GB for Whisper or 745 MB for Parakeet.",
	"faq.item5.question": "Which languages can I dictate in?",
	"faq.item5.answer":
		"Whisper Large v3 Turbo understands 99 languages and detects the one you're speaking. The lighter Parakeet model covers 25 European languages.",
	"faq.item6.question": "Which Macs are supported?",
	"faq.item6.answer":
		"Apple Silicon and Intel Macs running macOS 11 Big Sur or later. Transcription is fastest on Apple Silicon.",
	"faq.item7.question": "Why does it ask for Accessibility access?",
	"faq.item7.answer":
		"macOS requires it for any app that types into other apps. The microphone is only used while you're recording.",
	"faq.item8.question": "Does VoxFusion collect any data?",
	"faq.item8.answer":
		"Your audio and your text never leave your Mac. Anonymous usage analytics are optional, and you can switch them off in Settings.",
} as const;
