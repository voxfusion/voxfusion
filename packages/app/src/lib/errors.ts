import { Result, TaggedError } from "better-result";

export class CommandError extends TaggedError("CommandError")<{
	command: string;
	cause: unknown;
	message: string;
}>() {}

export class StorageError extends TaggedError("StorageError")<{
	operation: string;
	cause: unknown;
	message: string;
}>() {}

class BrowserStorageError extends TaggedError("BrowserStorageError")<{
	key: string;
	cause: unknown;
	message: string;
}>() {}

function errorMessage(error: unknown): string {
	if (error instanceof Error) return error.message;
	if (typeof error === "string") return error;
	return "Unexpected error";
}

export function commandError(command: string, cause: unknown): CommandError {
	return new CommandError({
		command,
		cause,
		message: `${command} failed: ${errorMessage(cause)}`,
	});
}

export function storageError(operation: string, cause: unknown): StorageError {
	return new StorageError({
		operation,
		cause,
		message: `${operation} failed: ${errorMessage(cause)}`,
	});
}

export function saveBrowserValue(key: string, value: string) {
	return Result.try({
		try: () => localStorage.setItem(key, value),
		catch: (cause) =>
			new BrowserStorageError({
				key,
				cause,
				message: `Failed to save ${key}: ${errorMessage(cause)}`,
			}),
	});
}
