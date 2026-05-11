import { invoke } from "@tauri-apps/api/core";

export interface AudioDeviceCommandResult {
	name: string;
	is_default: boolean;
}

export async function listAudioDevices(): Promise<AudioDeviceCommandResult[]> {
	return invoke<AudioDeviceCommandResult[]>("list_audio_devices");
}

export async function startRecordingWithDevice(deviceName: string | null): Promise<void> {
	await invoke("start_recording_with_device", { deviceName });
}

export async function stopRecordingWithDevice(): Promise<string> {
	return invoke<string>("stop_recording_with_device");
}
