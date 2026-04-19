export const security = {
	"security.title": "Security",
	"security.lastUpdated": "Last updated: February 7, 2026",
	"security.intro":
		"Protecting our users’ data is a priority for VoxFusion. This document describes the measures we take to protect information in accordance with Federal Law No. 149-FZ dated July 27, 2006 “On Information, Information Technologies, and Information Protection” and Order No. 21 of the Federal Service for Technical and Export Control of Russia dated February 18, 2013.",
	"security.section1.title": "1. Security architecture",
	"security.section1.p1":
		"1.1. Audio data is transmitted over an encrypted channel (TLS 1.3) and is not stored on servers after transcription is complete.",
	"security.section1.p2":
		"1.2. Transcription results are stored only on the user’s device. The server side does not store text results.",
	"security.section1.p3":
		"1.3. The app requests only the permissions it needs: microphone access and Accessibility services for text insertion.",
	"security.section2.title": "2. Data protection in transit",
	"security.section2.p1": "2.1. All network connections use TLS 1.3 with modern cipher suites.",
	"security.section2.p2":
		"2.2. Certificate pinning is implemented to mitigate man-in-the-middle (MITM) attacks.",
	"security.section2.p3":
		"2.3. API keys and authentication tokens are stored in the macOS Keychain.",
	"security.section3.title": "3. Data protection at rest",
	"security.section3.p1":
		"3.1. The local database containing transcription history is protected by macOS encryption mechanisms.",
	"security.section3.p2":
		"3.2. The custom dictionary and app settings are stored in encrypted application storage.",
	"security.section3.p3":
		"3.3. Temporary audio files are deleted immediately after transcription completes.",
	"security.section4.title": "4. Infrastructure security",
	"security.section4.p1":
		"4.1. Server infrastructure is hosted in ISO 27001-certified data centers.",
	"security.section4.p2":
		"4.2. Server access is restricted and controlled with multi-factor authentication.",
	"security.section4.p3": "4.3. All administrator actions and critical system events are logged.",
	"security.section5.title": "5. Application security",
	"security.section5.p1":
		"5.1. The app is signed with an Apple developer certificate and notarized by Apple.",
	"security.section5.p2":
		"5.2. Automatic updates are delivered over a secure channel with digital signature verification.",
	"security.section5.p3": "5.3. The app runs inside the macOS sandbox with minimal privileges.",
	"security.section6.title": "6. Incident response",
	"security.section6.p1":
		"6.1. If you discover a security vulnerability, please report it to security@voxfusion.com.",
	"security.section6.p2":
		"6.2. We commit to confirming receipt within 24 hours, providing an assessment within 72 hours, and issuing fixes for critical vulnerabilities as quickly as possible.",
	"security.section6.p3":
		"6.3. In the event of a personal data breach, users and authorized authorities will be notified in accordance with Article 21 of Federal Law No. 152-FZ.",
	"security.section7.title": "7. Compliance",
	"security.section7.p1":
		"7.1. Personal data is processed in accordance with Federal Law No. 152-FZ “On Personal Data.”",
	"security.section7.p2":
		"7.2. Organizational and technical safeguards comply with the requirements of FSTEC Russia Order No. 21.",
	"security.section7.p3": "7.3. Security contact: security@voxfusion.com.",
} as const;
