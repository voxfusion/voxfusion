import { createSignal } from "solid-js";
import { getAudioInputDevices, type AudioDevice } from "../lib/settingsStore";

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
		} catch (err) {
			console.error("Failed to fetch audio devices:", err);
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
