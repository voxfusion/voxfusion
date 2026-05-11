import posthog from "posthog-js";

const POSTHOG_KEY = import.meta.env.VITE_POSTHOG_KEY;
const POSTHOG_HOST = import.meta.env.VITE_POSTHOG_HOST ?? "https://eu.i.posthog.com";

let initialized = false;

export function initPostHog() {
	if (initialized) return;
	if (!POSTHOG_KEY) return;

	posthog.init(POSTHOG_KEY, {
		api_host: POSTHOG_HOST,
		capture_pageview: false,
		capture_pageleave: false,
		persistence: "localStorage",
		autocapture: false,
	});

	initialized = true;
}

export function capture(event: string, properties?: Record<string, unknown>) {
	if (!initialized) return;
	posthog.capture(event, properties);
}
