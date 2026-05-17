const preloaded = new Set<string>();

export function getFaviconUrl(domain: string): string {
	return `https://www.google.com/s2/favicons?domain=${encodeURIComponent(domain)}&sz=64`;
}

export function preloadFavicon(domain: string): void {
	if (preloaded.has(domain)) return;
	preloaded.add(domain);
	const img = new Image();
	img.decoding = "async";
	img.src = getFaviconUrl(domain);
}

export function preloadFavicons(domains: Iterable<string>): void {
	for (const domain of domains) preloadFavicon(domain);
}
