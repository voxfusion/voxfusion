import type { CommandResult } from "./invokeResult";
import { invokeResult } from "./invokeResult";

export async function muteMediaForRecording(): Promise<CommandResult<void>> {
	return invokeResult<void>("mute_media_for_recording");
}

export async function restoreMediaAfterRecording(): Promise<CommandResult<void>> {
	return invokeResult<void>("restore_media_after_recording");
}
