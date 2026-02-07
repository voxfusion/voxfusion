import posthog from "posthog-js";

const POSTHOG_KEY = "phc_n9Bb8O4Xw0diLGriQ4FKjrfHvUnFWDYcwo0fdBbd36m";
const POSTHOG_HOST = "https://eu.i.posthog.com";

let initialized = false;

export function initPostHog() {
	if (initialized) return;

	posthog.init(POSTHOG_KEY, {
		api_host: POSTHOG_HOST,
		capture_pageview: false, // We track SolidJS route changes manually
		capture_pageleave: false,
		persistence: "localStorage",
		autocapture: false, // We instrument events explicitly
	});

	initialized = true;
}

export function identifyUser(userId: string, properties?: Record<string, unknown>) {
	posthog.identify(userId, properties);
}

export function resetUser() {
	posthog.reset();
}

export function capture(event: string, properties?: Record<string, unknown>) {
	posthog.capture(event, properties);
}
