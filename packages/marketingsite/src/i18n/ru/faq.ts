export const faq = {
	"faq.tag": "FAQ",
	"faq.contact.github": "Ask on GitHub",
	"faq.item1.question": "Is VoxFusion free?",
	"faq.item1.answer":
		"Yes. It is open source under the MIT license. It has no subscription and needs no account.",
	"faq.item2.question": "How is it different from the dictation built into macOS?",
	"faq.item2.answer":
		"It uses Whisper Large v3 Turbo or Parakeet. It uses your dictionary for names and terms, and it keeps a history of your dictations. With Whisper, it can also change the tone of the text for each app.",
	"faq.item3.question": "Will it work in the app I use?",
	"faq.item3.answer":
		"If you can type in the app, you can dictate into it. VoxFusion types at your cursor like a keyboard, so it needs no plugins.",
	"faq.item4.question": "Does it work without internet?",
	"faq.item4.answer":
		"Yes, after you download a model once. Whisper is about 1.5 GB. Parakeet is about 745 MB.",
	"faq.item5.question": "Which Macs are supported?",
	"faq.item5.answer":
		"Apple Silicon and Intel Macs with macOS 11 Big Sur or later. Transcription is faster on Apple Silicon.",
	"faq.item6.question": "Why does it ask for Accessibility access?",
	"faq.item6.answer":
		"macOS requires this access for an app that types into other apps. VoxFusion uses the microphone only while you record.",
} as const;
