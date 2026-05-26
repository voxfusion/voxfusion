import {
	debug as logDebug,
	error as logError,
	info as logInfo,
	warn as logWarn,
} from "@tauri-apps/plugin-log";

type DiagnosticLevel = "debug" | "info" | "warn" | "error";
type DiagnosticFields = Record<string, unknown>;

export function logDiagnostic(
	level: DiagnosticLevel,
	target: string,
	message: string,
	fields: DiagnosticFields = {}
): void {
	const logMessage = `${target}.${message}`;
	const options = { keyValues: stringifyFields(fields) };
	const logger = {
		debug: logDebug,
		info: logInfo,
		warn: logWarn,
		error: logError,
	}[level];

	void logger(logMessage, options).catch(() => {
		// Logging must never become part of the app's control flow.
	});
}

export function errorFields(cause: unknown): DiagnosticFields {
	if (cause instanceof Error) {
		return {
			name: cause.name,
			message: cause.message,
			stack: cause.stack,
		};
	}

	return {
		message: String(cause),
	};
}

function stringifyFields(fields: DiagnosticFields): Record<string, string | undefined> {
	return Object.fromEntries(
		Object.entries(fields).map(([key, value]) => [
			key,
			value === undefined ? undefined : serializeField(value),
		])
	);
}

function serializeField(value: unknown): string {
	if (typeof value === "string") return value;

	try {
		return JSON.stringify(value);
	} catch {
		return String(value);
	}
}
