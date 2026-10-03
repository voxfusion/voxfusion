export const faq = {
	"faq.tag": "FAQ",
	"faq.title": "Questions, answered",
	"faq.description":
		"Something else on your mind? Open an issue on GitHub or write to hello@voxfusion.com.",
	"faq.item1.question": "Is VoxFusion really free?",
	"faq.item1.answer":
		"Yes. VoxFusion is free and open source under the MIT license. There is no subscription, no trial, and no account to create.",
	"faq.item2.question": "Does it work without internet?",
	"faq.item2.answer":
		"Yes. You need a connection once, to download the speech model (about 1.5 GB for Whisper or 745 MB for Parakeet). After that, dictation works fully offline.",
	"faq.item3.question": "Which languages can I dictate in?",
	"faq.item3.answer":
		"The default Whisper Large v3 Turbo model understands 99 languages and detects the one you're speaking. The lighter Parakeet model covers 25 European languages.",
	"faq.item4.question": "Which Macs are supported?",
	"faq.item4.answer":
		"Apple Silicon and Intel Macs running macOS 10.15 Catalina or later. Transcription is fastest on Apple Silicon.",
	"faq.item5.question": "Why does it ask for Accessibility access?",
	"faq.item5.answer":
		"macOS requires that permission for any app that types into other apps. VoxFusion uses it to notice your shortcut and to type your words at the cursor. The microphone is only used while you're recording.",
	"faq.item6.question": "Does VoxFusion collect any data?",
	"faq.item6.answer":
		"Your audio and your text never leave your Mac. During setup you choose whether to share anonymous usage events, such as which features are used and the app version, and you can switch that off at any time in Settings.",
} as const;
