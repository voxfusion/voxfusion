import { afterEach, beforeEach, expect, test } from "bun:test";
import { clearMocks, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { LogicalPosition, type Position } from "@tauri-apps/api/window";
import { repositionVoiceControlWindow } from "../src/lib/voiceControlPosition";

function display(width: number, height: number, scaleFactor = 1, x = 0, y = 0) {
	return {
		name: "Display",
		position: { x, y },
		size: { width, height },
		scaleFactor,
		workArea: { position: { x, y }, size: { width, height } },
	};
}

let monitor: ReturnType<typeof display> | null;
let primary: ReturnType<typeof display> | null;
let windowPosition: { x: number; y: number };
let windowScale: number;
let moves: LogicalPosition[];
let failNextMove: boolean;
let originalWindow: PropertyDescriptor | undefined;

beforeEach(() => {
	originalWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
	Object.defineProperty(globalThis, "window", { configurable: true, value: {} });
	monitor = display(2560, 1440);
	primary = monitor;
	windowPosition = { x: 0, y: 0 };
	windowScale = 1;
	moves = [];
	failNextMove = false;
	mockWindows("voice-control");
	mockIPC((command, payload) => {
		switch (command) {
			case "plugin:window|cursor_position":
				return { x: 200, y: 200 };
			case "plugin:window|monitor_from_point":
				return monitor;
			case "plugin:window|primary_monitor":
				return primary;
			case "plugin:window|outer_position":
				return {
					x: Math.round(windowPosition.x * windowScale),
					y: Math.round(windowPosition.y * windowScale),
				};
			case "plugin:window|scale_factor":
				return windowScale;
			case "plugin:window|set_position": {
				if (failNextMove) {
					failNextMove = false;
					throw new Error("Display reconfiguration in progress");
				}
				const { value } = payload as { value: Position };
				expect(value.toJSON()).toHaveProperty("Logical");
				const position = value.toLogical(windowScale);
				moves.push(position);
				windowPosition = position;
				return;
			}
			default:
				throw new Error(`Unexpected IPC command: ${command}`);
		}
	});
});

afterEach(() => {
	clearMocks();
	if (originalWindow) {
		Object.defineProperty(globalThis, "window", originalWindow);
	} else {
		Reflect.deleteProperty(globalThis, "window");
	}
});

test("recenters after unplugging a larger display with the same origin", async () => {
	await repositionVoiceControlWindow(100);
	expect(windowPosition).toEqual(new LogicalPosition(1230, 1392));

	monitor = display(3024, 1964, 2);
	windowScale = 2;
	await repositionVoiceControlWindow(100);
	expect(windowPosition).toEqual(new LogicalPosition(706, 934));
});

test("responds to resolution and scale changes without a monitor-origin change", async () => {
	await repositionVoiceControlWindow(100);
	monitor = display(1920, 1080);
	await repositionVoiceControlWindow(100);
	expect(windowPosition).toEqual(new LogicalPosition(910, 1032));

	monitor = display(1920, 1080, 2);
	windowScale = 2;
	await repositionVoiceControlWindow(100);
	expect(windowPosition).toEqual(new LogicalPosition(430, 492));
});

test("moves to an offset Retina display while the window still has its old scale", async () => {
	await repositionVoiceControlWindow(100);
	monitor = display(3024, 1964, 2, -3024, -400);
	await repositionVoiceControlWindow(100);
	expect(windowPosition).toEqual(new LogicalPosition(-806, 734));

	windowScale = 2;
	await repositionVoiceControlWindow(100);
	expect(moves).toHaveLength(2);
});

test("repairs OS relocation even when the target monitor is unchanged", async () => {
	await repositionVoiceControlWindow(100);
	windowPosition = { x: 1400, y: 1300 };
	await repositionVoiceControlWindow(100);
	expect(windowPosition).toEqual(new LogicalPosition(1230, 1392));
	expect(moves).toHaveLength(2);
});

test("keeps compact, hands-free, and error widths centered", async () => {
	for (const [width, x] of [
		[100, 1230],
		[140, 1210],
		[260, 1150],
	] as const) {
		await repositionVoiceControlWindow(width);
		expect(windowPosition).toEqual(new LogicalPosition(x, 1392));
	}
});

test("does not move an already centered window, including pixel rounding", async () => {
	monitor = display(1921, 1080);
	await repositionVoiceControlWindow(100);
	await repositionVoiceControlWindow(100);
	expect(moves).toHaveLength(1);
});

test("falls back to the primary monitor when the cursor display disappears", async () => {
	monitor = null;
	primary = display(3024, 1964, 2);
	await repositionVoiceControlWindow(100);
	expect(windowPosition).toEqual(new LogicalPosition(706, 934));
});

test("waits for a later check when no display is available", async () => {
	monitor = null;
	primary = null;
	await repositionVoiceControlWindow(100);
	expect(moves).toHaveLength(0);

	monitor = display(3024, 1964, 2);
	await repositionVoiceControlWindow(100);
	expect(windowPosition).toEqual(new LogicalPosition(706, 934));
});

test("retries a failed move on the next check", async () => {
	failNextMove = true;
	await expect(repositionVoiceControlWindow(100)).rejects.toThrow("Display reconfiguration");
	await repositionVoiceControlWindow(100);
	expect(windowPosition).toEqual(new LogicalPosition(1230, 1392));
});
