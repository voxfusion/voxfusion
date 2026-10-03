export const faq = {
	"faq.tag": "FAQ",
	"faq.title": "Questions, answered",
	"faq.description": "Something else on your mind? Open an issue on GitHub or write to us.",
	"faq.contact.github": "Open an issue",
	"faq.item1.question": "Is VoxFusion really free?",
	"faq.item1.answer":
		"Yes. VoxFusion is free and open source under the MIT license. There is no subscription, no trial, and no account to create.",
	"faq.item2.question": "How is it different from the dictation built into macOS?",
	"faq.item2.answer":
		"Built-in dictation is fine for a quick sentence. VoxFusion is made for writing all day: it runs Whisper Large v3 Turbo, spells the names and jargon you teach it, matches its tone to the app you're in, and keeps a history of what you dictated. It's also open source, so you can check exactly what happens to your audio.",
	"faq.item3.question": "Will it work in the app I use?",
	"faq.item3.answer":
		"If you can type in it, you can dictate into it. VoxFusion types at your cursor the way a keyboard would, so it needs no plugins or integrations: mail, chat, documents, browsers, code editors, terminals.",
	"faq.item4.question": "Does it work without internet?",
	"faq.item4.answer":
		"Yes. You need a connection once, to download the speech model (about 1.5 GB for Whisper or 745 MB for Parakeet). After that, dictation works fully offline.",
	"faq.item5.question": "Which languages can I dictate in?",
	"faq.item5.answer":
		"The default Whisper Large v3 Turbo model understands 99 languages and detects the one you're speaking. The lighter Parakeet model covers 25 European languages.",
	"faq.item6.question": "Which Macs are supported?",
	"faq.item6.answer":
		"Apple Silicon and Intel Macs running macOS 11 Big Sur or later. Transcription is fastest on Apple Silicon.",
	"faq.item7.question": "Why does it ask for Accessibility access?",
	"faq.item7.answer":
		"macOS requires that permission for any app that types into other apps. VoxFusion uses it to notice your shortcut and to type your words at the cursor. The microphone is only used while you're recording.",
	"faq.item8.question": "Does VoxFusion collect any data?",
	"faq.item8.answer":
		"Your audio and your text never leave your Mac. During setup you choose whether to share anonymous usage events, such as which features are used and the app version, and you can switch that off at any time in Settings.",
} as const;
