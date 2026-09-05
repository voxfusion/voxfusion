const BYTES_PER_MB = 1024 * 1024;
export const formatMb = (bytes: number): string => (bytes / BYTES_PER_MB).toFixed(1);

export const formatEta = (etaSeconds: number): string => {
	const total = Math.max(0, Math.round(etaSeconds));
	const hours = Math.floor(total / 3600);
	const minutes = Math.floor((total % 3600) / 60);
	const seconds = total % 60;
	if (hours > 0) {
		return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
	}
	return `${minutes}:${String(seconds).padStart(2, "0")}`;
};
