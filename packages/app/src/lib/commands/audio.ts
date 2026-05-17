import type { CommandResult } from "./invokeResult";
import { invokeResult } from "./invokeResult";

export interface AudioDeviceCommandResult {
	name: string;
	is_default: boolean;
}

export async function listAudioDevices(): Promise<CommandResult<AudioDeviceCommandResult[]>> {
	return invokeResult<AudioDeviceCommandResult[]>("list_audio_devices");
}

export async function startRecordingWithDevice(
	deviceName: string | null
): Promise<CommandResult<void>> {
	return invokeResult<void>("start_recording_with_device", { deviceName });
}

export async function stopRecordingWithDevice(): Promise<CommandResult<string>> {
	return invokeResult<string>("stop_recording_with_device");
}
