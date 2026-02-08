import { createSignal } from "solid-js";
import { type AudioDevice, getAudioInputDevices } from "../lib/settingsStore";

export function useAudioDevices() {
	const [devices, setDevices] = createSignal<AudioDevice[]>([]);
	const [isLoading, setIsLoading] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);

	const fetchDevices = async () => {
		setIsLoading(true);
		setError(null);
		try {
			const fetchedDevices = await getAudioInputDevices();
			setDevices(fetchedDevices);
		} catch {
			setError("Failed to fetch audio devices");
		} finally {
			setIsLoading(false);
		}
	};

	return {
		devices,
		isLoading,
		error,
		fetchDevices,
	};
}
