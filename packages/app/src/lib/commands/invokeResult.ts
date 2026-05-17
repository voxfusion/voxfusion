import { invoke } from "@tauri-apps/api/core";
import { Result, type Result as ResultType } from "better-result";
import { type CommandError, commandError } from "../errors";

export type CommandResult<T> = ResultType<T, CommandError>;

export function invokeResult<T>(
	command: string,
	args?: Record<string, unknown>
): Promise<CommandResult<T>> {
	return Result.tryPromise({
		try: () => invoke<T>(command, args),
		catch: (cause) => commandError(command, cause),
	});
}
