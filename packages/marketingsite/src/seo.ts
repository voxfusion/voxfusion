const SITE_URL = "https://voxfusion.io";

export function softwareApplicationSchema(description: string) {
	return {
		"@context": "https://schema.org",
		"@type": "SoftwareApplication",
		name: "VoxFusion",
		description,
		url: SITE_URL,
		downloadUrl: `${SITE_URL}/download`,
		image: `${SITE_URL}/og.png`,
		operatingSystem: "macOS",
		applicationCategory: "UtilitiesApplication",
		offers: {
			"@type": "Offer",
			price: "0",
			priceCurrency: "USD",
		},
	};
}

export function faqPageSchema(items: { question: string; answer: string }[]) {
	return {
		"@context": "https://schema.org",
		"@type": "FAQPage",
		mainEntity: items.map((item) => ({
			"@type": "Question",
			name: item.question,
			acceptedAnswer: { "@type": "Answer", text: item.answer },
		})),
	};
}

/** Combines several schema.org objects into one JSON-LD document. */
export function schemaGraph(...schemas: Record<string, unknown>[]) {
	return {
		"@context": "https://schema.org",
		"@graph": schemas.map(({ "@context": _context, ...schema }) => schema),
	};
}
