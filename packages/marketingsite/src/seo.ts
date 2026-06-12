const SITE_URL = "https://voxfusion.com";

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
