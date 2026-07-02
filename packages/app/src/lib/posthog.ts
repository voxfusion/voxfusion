import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { Result } from "better-result";
import posthog from "posthog-js";
import { loadSettings } from "./settingsStore";

const POSTHOG_KEY = "phc_n9Bb8O4Xw0diLGriQ4FKjrfHvUnFWDYcwo0fdBbd36m";
const POSTHOG_HOST = "https://eu.i.posthog.com";

let initialized = false;
let ready = false;
let analyticsEnabled = false;
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

function initPostHogClient() {
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

/**
 * Applies the user's analytics choice immediately: enabling lazily
 * initializes PostHog and opts back in, disabling opts out of capturing.
 */
export function applyAnalyticsEnabled(enabled: boolean) {
	analyticsEnabled = enabled;

	if (enabled) {
		initPostHogClient();
		if (initialized && posthog.has_opted_out_capturing()) {
			posthog.opt_in_capturing();
		}
	} else {
		pendingEvents.length = 0;
		if (initialized && !posthog.has_opted_out_capturing()) {
			posthog.opt_out_capturing();
		}
	}
}

/**
 * Reads the persisted analytics consent, applies it, and keeps it in sync
 * across windows by re-applying whenever settings change.
 */
export async function initAnalytics() {
	const settings = await Result.tryPromise(() => loadSettings());
	if (Result.isOk(settings)) {
		applyAnalyticsEnabled(settings.value.analyticsEnabled);
	} else {
		// Fail closed: without a readable consent value, don't capture.
		console.warn("Failed to load analytics setting:", settings.error);
	}

	await listen("settings-changed", async () => {
		const current = await Result.tryPromise(() => loadSettings());
		if (Result.isOk(current) && current.value.analyticsEnabled !== analyticsEnabled) {
			applyAnalyticsEnabled(current.value.analyticsEnabled);
		}
	});
}

export function capture(event: string, properties?: Record<string, unknown>) {
	if (!analyticsEnabled || !initialized) return;
	if (!ready) {
		pendingEvents.push([event, properties]);
		return;
	}

	posthog.capture(event, properties);
}
