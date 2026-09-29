import {
	LogicalPosition,
	cursorPosition,
	getCurrentWindow,
	monitorFromPoint,
	primaryMonitor,
} from "@tauri-apps/api/window";

export const VOICE_CONTROL_WINDOW_HEIGHT = 28;
const BOTTOM_PADDING = 20;

export async function repositionVoiceControlWindow(width: number) {
	const cursor = await cursorPosition();
	const monitor = (await monitorFromPoint(cursor.x, cursor.y)) ?? (await primaryMonitor());
	if (!monitor) return;

	const origin = monitor.position.toLogical(monitor.scaleFactor);
	const size = monitor.size.toLogical(monitor.scaleFactor);
	const target = new LogicalPosition(
		origin.x + (size.width - width) / 2,
		origin.y + size.height - VOICE_CONTROL_WINDOW_HEIGHT - BOTTOM_PADDING
	);

	// A replacement display can have the same origin but different dimensions.
	// Read the actual position too, since macOS can move the window on disconnect.
	const window = getCurrentWindow();
	const [position, scaleFactor] = await Promise.all([window.outerPosition(), window.scaleFactor()]);
	const current = position.toLogical(scaleFactor);
	const tolerance = 0.5 / scaleFactor;
	if (Math.abs(current.x - target.x) <= tolerance && Math.abs(current.y - target.y) <= tolerance) {
		return;
	}

	// macOS interprets physical positions using the window's old display scale.
	// Logical coordinates keep moves between Retina and external displays correct.
	await window.setPosition(target);
}
