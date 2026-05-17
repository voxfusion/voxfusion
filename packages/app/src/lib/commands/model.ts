import type { CommandResult } from "./invokeResult";
import { invokeResult } from "./invokeResult";

export async function checkModelStatus(): Promise<CommandResult<boolean>> {
	return invokeResult<boolean>("check_model_status");
}

export async function downloadWhisperModel(): Promise<CommandResult<void>> {
	return invokeResult<void>("download_whisper_model");
}
