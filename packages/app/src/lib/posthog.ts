import { getVersion } from "@tauri-apps/api/app";
import { Result } from "better-result";
import posthog from "posthog-js";

const POSTHOG_KEY = "phc_n9Bb8O4Xw0diLGriQ4FKjrfHvUnFWDYcwo0fdBbd36m";
const POSTHOG_HOST = "https://eu.i.posthog.com";

let initialized = false;
let ready = false;
const pendingEvents: Array<[string, Record<string, unknown> | undefined]> = [];

async function registerAppVersion() {
	const appVersion = await Result.tryPromise(() => getVersion());
	if (Result.isOk(appVersion)) {
		posthog.register({ app_version: appVersion.value });
	} else {
		console.warn("Failed to register PostHog app version:", appVersion.error);
	}
	ready = true;
	for (const [event, properties] of pendingEvents.splice(0)) {
		posthog.capture(event, properties);
	}
}

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
	void registerAppVersion();
}

export function capture(event: string, properties?: Record<string, unknown>) {
	if (!initialized) return;
	if (!ready) {
		pendingEvents.push([event, properties]);
		return;
	}

	posthog.capture(event, properties);
}
