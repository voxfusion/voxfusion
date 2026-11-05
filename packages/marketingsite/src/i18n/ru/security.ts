export const security = {
	"security.title": "Security",
	"security.meta.description":
		"How VoxFusion protects your data: on-device transcription, no server-side audio processing, signed and notarized builds, and local-only storage.",
	"security.lastUpdated": "Last updated: November 5, 2026",
	"security.intro":
		"Protecting our users’ data is a priority for VoxFusion. VoxFusion is a local-first application — audio is transcribed on-device and never leaves your Mac. This document describes the measures we take to protect information in accordance with Federal Law No. 149-FZ dated July 27, 2006 “On Information, Information Technologies, and Information Protection.”",
	"security.section1.title": "1. Security architecture",
	"security.section1.p1":
		"1.1. Speech recognition runs entirely on the User’s device using a local model. Audio data is never transmitted over the network by the Service.",
	"security.section1.p2":
		"1.2. Transcribed text is stored only on the User’s device. There is no server-side component that receives audio or transcribed text.",
	"security.section1.p3":
		"1.3. The app requests only the permissions it needs: microphone access and Accessibility services for text insertion.",
	"security.section2.title": "2. Data protection at rest",
	"security.section2.p1":
		"2.1. The local database containing transcription history is stored under the macOS application support directory and protected by macOS file-system permissions.",
	"security.section2.p2":
		"2.2. The custom dictionary and app settings are stored locally on the device.",
	"security.section2.p3":
		"2.3. Audio recordings are saved on the User’s device. While you dictate, the app writes the recording to a file in the recordings folder of its application support directory (~/Library/Application Support/io.voxfusion.app/recordings), protected by the same macOS file-system permissions, and links it to the transcription made from it in the transcription history. The app keeps only the most recent recordings (currently the latest 20) and automatically deletes older ones after each successful transcription; a recording whose transcription failed is kept so that it can be retried. Saving recordings cannot currently be turned off.",
	"security.section2.p4":
		"2.4. To delete recordings, delete an entry from the history on the app’s Home screen, which deletes its recording too, or use Settings → Privacy → Clear History, which deletes every saved transcription and every recording kept on the device. Wait until any dictation in progress has finished recording and transcribing before clearing: the recording being processed is kept and its transcription is saved once it completes. Deleted recordings are removed from disk, not moved to the Trash. Removing the app does not delete them by itself; to remove recordings and history completely, also delete the ~/Library/Application Support/io.voxfusion.app folder.",
	"security.section3.title": "3. Application security",
	"security.section3.p1":
		"3.1. The app is signed with an Apple developer certificate and notarized by Apple.",
	"security.section3.p2":
		"3.2. Automatic updates are delivered over a secure channel with digital signature verification.",
	"security.section3.p3":
		"3.3. The app runs with the minimum macOS privileges required to operate.",
	"security.section4.title": "4. Incident response",
	"security.section4.p1":
		"4.1. If you discover a security vulnerability, please report it to security@voxfusion.io.",
	"security.section4.p2":
		"4.2. We commit to confirming receipt within 24 hours, providing an assessment within 72 hours, and issuing fixes for critical vulnerabilities as quickly as possible.",
	"security.section5.title": "5. Compliance",
	"security.section5.p1":
		"5.1. Because no personal data is processed on our servers, the Service minimizes data-processing risk by design.",
	"security.section5.p2": "5.2. Security contact: security@voxfusion.io.",
} as const;
