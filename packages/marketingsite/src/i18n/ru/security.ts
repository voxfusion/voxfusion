export const security = {
	"security.title": "Security",
	"security.meta.description":
		"Learn how VoxFusion protects your data with on-device transcription, local-only storage, and Apple-notarized builds with no server-side audio processing.",
	"security.lastUpdated": "Last updated: November 5, 2026",
	"security.intro":
		"VoxFusion treats user data protection as a priority. VoxFusion is a local-first application that transcribes audio on your Mac so audio never leaves your device. This document describes our measures under Federal Law No. 149-FZ dated July 27, 2006 “On Information, Information Technologies, and Information Protection.”",
	"security.section1.title": "1. Security architecture",
	"security.section1.p1":
		"1.1. Speech recognition runs on the User’s device with a local model. The Service never transmits audio data over the network.",
	"security.section1.p2":
		"1.2. The Service stores transcribed text only on the User’s device. No server-side component receives audio or transcribed text.",
	"security.section1.p3":
		"1.3. The app requests only the permissions it needs: microphone access and Accessibility permission for text insertion.",
	"security.section2.title": "2. Data protection at rest",
	"security.section2.p1":
		"2.1. VoxFusion stores the local transcription database in the macOS application support directory, where macOS file-system permissions protect it.",
	"security.section2.p2":
		"2.2. VoxFusion stores the custom dictionary and app settings locally on the device.",
	"security.section2.p3":
		"2.3. VoxFusion saves audio recordings on the User’s device. While you dictate, the app writes each recording to ~/Library/Application Support/io.voxfusion.app/recordings under macOS file-system permissions. The app links each recording to its entry in transcription history. It keeps only the latest 20 recordings and deletes older recordings automatically after each successful transcription. It keeps any recording from a failed transcription so you can retry it. You cannot currently turn off local recording storage.",
	"security.section2.p4":
		"2.4. Delete an entry on the Home screen to delete that transcription and its recording. Open Settings → Privacy → Clear History to delete every saved transcription and recording on the device. Wait until any active dictation finishes before you clear history, because the app keeps the active recording and saves its transcription when it finishes. The app removes deleted recordings directly from disk instead of the Trash. If you remove the app alone, recordings and history stay on disk. To remove all local data, also delete the ~/Library/Application Support/io.voxfusion.app folder.",
	"security.section3.title": "3. Application security",
	"security.section3.p1":
		"3.1. We sign the app with an Apple developer certificate, and Apple notarizes the app.",
	"security.section3.p2":
		"3.2. VoxFusion delivers automatic updates over a secure channel and verifies their digital signatures.",
	"security.section3.p3":
		"3.3. The app runs with the minimum macOS privileges required to operate.",
	"security.section4.title": "4. Incident response",
	"security.section4.p1":
		"4.1. If you discover a security vulnerability, please report it to security@voxfusion.io.",
	"security.section4.p2":
		"4.2. We confirm receipt within 24 hours, provide an assessment within 72 hours, and issue fixes for critical vulnerabilities as quickly as possible.",
	"security.section5.title": "5. Compliance",
	"security.section5.p1":
		"5.1. Because our servers process no personal data, the Service minimizes data-processing risk by design.",
	"security.section5.p2": "5.2. Send security inquiries to security@voxfusion.io.",
} as const;
