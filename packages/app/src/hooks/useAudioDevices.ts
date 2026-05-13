import { createSignal, onCleanup } from "solid-js";
import {
	type AudioDevice,
	getAudioInputDevices,
	normalizeSelectedMicrophone,
} from "../lib/settingsStore";

export function useAudioDevices() {
	const [devices, setDevices] = createSignal<AudioDevice[]>([]);
	const [isLoading, setIsLoading] = createSignal(false);
	const [error, setError] = createSignal<string | null>(null);

	const fetchDevices = async (showLoading = true) => {
		if (showLoading) {
			setIsLoading(true);
			setError(null);
		}
		try {
			const fetchedDevices = await getAudioInputDevices();
			setDevices(fetchedDevices);
			await normalizeSelectedMicrophone(fetchedDevices);
		} catch {
			setError("Failed to fetch audio devices");
		} finally {
			if (showLoading) {
				setIsLoading(false);
			}
		}
	};

	const refreshInterval = setInterval(() => fetchDevices(false), 1000);

	if (typeof navigator !== "undefined" && navigator.mediaDevices) {
		let refreshTimeout: ReturnType<typeof setTimeout> | undefined;
		const handleDeviceChange = () => {
			clearTimeout(refreshTimeout);
			refreshTimeout = setTimeout(() => fetchDevices(false), 250);
		};

		navigator.mediaDevices.addEventListener("devicechange", handleDeviceChange);
		onCleanup(() => {
			clearTimeout(refreshTimeout);
			navigator.mediaDevices.removeEventListener("devicechange", handleDeviceChange);
		});
	}

	onCleanup(() => clearInterval(refreshInterval));

	return {
		devices,
		isLoading,
		error,
		fetchDevices,
	};
}
