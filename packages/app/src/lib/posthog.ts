import posthog from "posthog-js";

const POSTHOG_KEY = "phc_n9Bb8O4Xw0diLGriQ4FKjrfHvUnFWDYcwo0fdBbd36m";
const POSTHOG_HOST = "https://eu.i.posthog.com";

let initialized = false;

export function initPostHog() {
	if (initialized) return;

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
	posthog.capture(event, properties);
}
