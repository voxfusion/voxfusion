import { MONTHLY_TRANSCRIPTION_WORD_LIMITS } from "@voxfusion/shared/subscriptionPlans";

const freePlanWordLimit = MONTHLY_TRANSCRIPTION_WORD_LIMITS.free.toLocaleString("en-US");

export const terms = {
	"terms.title": "Terms of Use",
	"terms.lastUpdated": "Last updated: February 7, 2026",
	"terms.intro":
		"These Terms of Use (the “Terms”) govern the relationship between Individual Entrepreneur Grigoriy Alekseevich Tokarev (the “Rights Holder”) and the user (the “User”) in connection with the use of the VoxFusion software (the “Service”). The Terms are drafted in accordance with the Civil Code of the Russian Federation and Law of the Russian Federation No. 2300-1 dated February 7, 1992 “On Consumer Protection.”",
	"terms.section1.title": "1. Subject of the agreement",
	"terms.section1.p1":
		"1.1. The Rights Holder grants the User a non-exclusive license to use the Service under these Terms (Article 1235 of the Civil Code of the Russian Federation).",
	"terms.section1.p2":
		"1.2. The Service is intended to convert voice recordings into text on macOS devices.",
	"terms.section1.p3":
		"1.3. Beginning to use the Service constitutes acceptance of these Terms (Article 438 of the Civil Code of the Russian Federation).",
	"terms.section2.title": "2. Conditions of use",
	"terms.section2.p1":
		"2.1. The User agrees to use the Service only for lawful purposes and in accordance with these Terms.",
	"terms.section2.p2":
		"2.2. The following are prohibited: decompiling, disassembling, or otherwise attempting to extract the source code of the Service; using the Service to violate third-party rights; transferring the license to third parties; or using the Service to create a competing product.",
	"terms.section2.p3":
		"2.3. The User is responsible for the content of dictated text and compliance with applicable law.",
	"terms.section3.title": "3. Plans and payment",
	"terms.section3.p1": `3.1. The Service is available under a free plan limited to ${freePlanWordLimit} words per month and a paid Pro plan with unlimited transcription.`,
	"terms.section3.p2":
		"3.2. During the alpha period, the Pro plan is provided free of charge. After alpha, the planned price is about $5 per month.",
	"terms.section3.p3":
		"3.3. Once paid billing begins, payment will be charged automatically at the start of each billing period unless the User cancels the subscription. Access to paid features remains available until the end of the paid period.",
	"terms.section3.p4":
		"3.4. Refunds are provided in accordance with Russian consumer protection law.",
	"terms.section4.title": "4. Intellectual property",
	"terms.section4.p1":
		"4.1. Exclusive rights to the Service, including the software code, design, trademark, and documentation, belong to the Rights Holder (Articles 1225 and 1259 of the Civil Code of the Russian Federation).",
	"terms.section4.p2": "4.2. Texts obtained as a result of transcription belong to the User.",
	"terms.section5.title": "5. Limitation of liability",
	"terms.section5.p1":
		"5.1. The Service is provided “as is.” The Rights Holder does not guarantee 100% speech recognition accuracy.",
	"terms.section5.p2":
		"5.2. The Rights Holder is not liable for losses arising from improper use of the Service, interruptions in Service operation caused by technical reasons, or actions of third parties.",
	"terms.section5.p3":
		"5.3. The aggregate liability of the Rights Holder is limited to the amount paid by the User for the last 3 months of using the Service.",
	"terms.section6.title": "6. Term and termination",
	"terms.section6.p1":
		"6.1. These Terms take effect when the User begins using the Service and remain in force indefinitely.",
	"terms.section6.p2":
		"6.2. The Rights Holder may stop providing the Service to the User if these Terms are violated.",
	"terms.section6.p3":
		"6.3. The User may stop using the Service at any time by removing the app from the device.",
	"terms.section7.title": "7. Dispute resolution",
	"terms.section7.p1":
		"7.1. All disputes are resolved through negotiations. If no agreement is reached, the dispute will be submitted to a court at the Rights Holder’s location in accordance with Russian law.",
	"terms.section8.title": "8. Final provisions",
	"terms.section8.p1":
		"8.1. The Rights Holder may amend these Terms by notifying the User through the Service or by email at least 14 days before the changes take effect.",
	"terms.section8.p2": "8.2. These Terms are governed by the laws of the Russian Federation.",
	"terms.section8.p3": "8.3. Contact details: hello@voxfusion.com.",
} as const;
