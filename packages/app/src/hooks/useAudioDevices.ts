import { createSignal } from "solid-js";
import { type AudioDevice, getAudioInputDevices } from "../lib/settingsStore";

export function useAudioDevices() {
	const [devices, setDevices] = createSignal<AudioDevice[]>([]);
	const [isLoading, setIsLoading] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);

	const fetchDevices = async () => {
		setIsLoading(true);
		setError(null);
		const fetchedDevices = await getAudioInputDevices();
		setDevices(fetchedDevices);
		setIsLoading(false);
	};

	return {
		devices,
		isLoading,
		error,
		fetchDevices,
	};
}
