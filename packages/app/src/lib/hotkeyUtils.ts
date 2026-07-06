import { listen } from "@tauri-apps/api/event";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { Result } from "better-result";
import { startSystemKeyWatcher } from "./commands/permissions";

type SystemKey =
	| "fn"
	| "leftControl"
	| "rightControl"
	| "leftOption"
	| "rightOption"
	| "leftShift"
	| "rightShift"
	| "leftCommand"
	| "rightCommand";

type SystemKeyPressedPayload = {
	key: SystemKey;
	pressedKeys: SystemKey[];
};

type SystemKeyReleasedPayload = {
	key: SystemKey;
	pressedKeys: SystemKey[];
};

type RegisteredHotkey = {
	hotkey: string;
	dispose: () => Promise<void>;
};

type GlobalShortcutEvent = {
	state: string;
};

type HotkeyHandler = () => void;
type HotkeyHandlers = {
	onPressed: HotkeyHandler;
	onReleased?: HotkeyHandler;
};

type HotkeyRecorderActivePayload = {
	active: boolean;
};

export const DEFAULT_HOTKEY = "LeftControl+LeftOption";
export const DEFAULT_HOLD_TO_SPEAK_HOTKEY = "RightCommand";

/**
 * How long an exact hotkey match must stay held unchanged before it fires.
 * Long enough to outlast synthetic modifier transients from key remappers
 * (Karabiner posts a hyperkey's modifiers within ~1ms of each other), short
 * enough to be imperceptible on a deliberate press.
 */
const HOTKEY_MATCH_STABILITY_MS = 60;

/**
 * Maps KeyboardEvent.code values to display-friendly modifier labels.
 * Used during recording to show the user what they're pressing.
 */
const CODE_TO_DISPLAY: Record<string, string> = {
	MetaLeft: "Left \u2318",
	MetaRight: "Right \u2318",
	ShiftLeft: "Left \u21E7",
	ShiftRight: "Right \u21E7",
	AltLeft: "Left \u2325",
	AltRight: "Right \u2325",
	ControlLeft: "Left \u2303",
	ControlRight: "Right \u2303",
	CapsLock: "\u21EA Caps Lock",
	Fn: "Fn",
};

/**
 * The set of KeyboardEvent.code values that correspond to modifier keys.
 */
const MODIFIER_CODES = new Set([
	"MetaLeft",
	"MetaRight",
	"ShiftLeft",
	"ShiftRight",
	"AltLeft",
	"AltRight",
	"ControlLeft",
	"ControlRight",
	"CapsLock",
	"Fn",
]);

const COMBO_MODIFIER_NAMES = new Set([
	"Command",
	"Cmd",
	"CommandOrControl",
	"CommandOrCtrl",
	"CmdOrCtrl",
	"CmdOrControl",
	"Control",
	"Ctrl",
	"Alt",
	"Option",
	"Shift",
	"CapsLock",
	"Fn",
]);

const SYSTEM_HOTKEY_TO_KEY: Record<string, SystemKey> = {
	Fn: "fn",
	LeftControl: "leftControl",
	RightControl: "rightControl",
	LeftOption: "leftOption",
	RightOption: "rightOption",
	LeftShift: "leftShift",
	RightShift: "rightShift",
	LeftCommand: "leftCommand",
	RightCommand: "rightCommand",
};

const SYSTEM_HOTKEY_ORDER = [
	"Fn",
	"LeftControl",
	"RightControl",
	"LeftOption",
	"RightOption",
	"LeftShift",
	"RightShift",
	"LeftCommand",
	"RightCommand",
];

const MODIFIER_CODE_TO_SYSTEM_HOTKEY: Record<string, string> = {
	Fn: "Fn",
	ControlLeft: "LeftControl",
	ControlRight: "RightControl",
	AltLeft: "LeftOption",
	AltRight: "RightOption",
	ShiftLeft: "LeftShift",
	ShiftRight: "RightShift",
	MetaLeft: "LeftCommand",
	MetaRight: "RightCommand",
};

let registeredDictationHotkey: RegisteredHotkey | null = null;
let registeredDictationHotkeys: RegisteredHotkey[] = [];

/**
 * Check if a KeyboardEvent.code is a modifier key.
 */
export function isModifierCode(code: string): boolean {
	return MODIFIER_CODES.has(code);
}

/**
 * Get the display string for a modifier KeyboardEvent.code during recording.
 * Returns undefined for non-modifier codes.
 */
export function modifierCodeToDisplay(code: string): string | undefined {
	return CODE_TO_DISPLAY[code];
}

/**
 * Convert a KeyboardEvent.code modifier into the system-only hotkey string
 * persisted in settings, when the native watcher supports it.
 */
export function modifierCodeToSystemHotkey(code: string): string | undefined {
	return MODIFIER_CODE_TO_SYSTEM_HOTKEY[code];
}

export function systemHotkeyFromKeys(keys: Iterable<SystemKey>): string {
	const keySet = new Set(keys);

	return SYSTEM_HOTKEY_ORDER.filter((part) => {
		const key = SYSTEM_HOTKEY_TO_KEY[part];
		return key !== undefined && keySet.has(key);
	}).join("+");
}

function systemKeysFromHotkey(hotkey: string): SystemKey[] | undefined {
	const parts = hotkey.split("+");
	if (parts.length === 0) return undefined;

	const keys: SystemKey[] = [];
	for (const part of parts) {
		const key = SYSTEM_HOTKEY_TO_KEY[part];
		if (!key) return undefined;
		keys.push(key);
	}

	return keys;
}

function systemKeySetsMatch(a: Iterable<SystemKey>, b: Iterable<SystemKey>): boolean {
	const aSet = new Set(a);
	const bSet = new Set(b);
	if (aSet.size !== bSet.size) return false;

	for (const key of aSet) {
		if (!bSet.has(key)) return false;
	}

	return true;
}

/**
 * Check if a hotkey should be handled by the native system key watcher.
 */
export function isSystemOnlyHotkey(hotkey: string): boolean {
	return systemKeysFromHotkey(hotkey) !== undefined;
}

/**
 * Check if a hotkey string can be registered by one of the supported backends.
 */
export function isValidHotkey(hotkey: string): boolean {
	if (isSystemOnlyHotkey(hotkey)) return true;

	const parts = hotkey
		.split("+")
		.map((part) => part.trim())
		.filter(Boolean);
	if (parts.length < 2) return false;
	if (parts.some((part) => part in SYSTEM_HOTKEY_TO_KEY)) return false;

	const key = parts[parts.length - 1];
	if (!key || COMBO_MODIFIER_NAMES.has(key)) return false;

	return parts.slice(0, -1).every((part) => COMBO_MODIFIER_NAMES.has(part));
}

async function registerSystemHotkey(
	hotkey: string,
	handlers: HotkeyHandlers
): Promise<RegisteredHotkey> {
	const expectedKeys = systemKeysFromHotkey(hotkey);
	if (!expectedKeys) {
		throw new Error(`Unsupported system hotkey: ${hotkey}`);
	}

	const watcherStarted = await startSystemKeyWatcher();
	if (Result.isError(watcherStarted)) {
		throw watcherStarted.error;
	}

	let isHeld = false;
	let isRecorderActive = false;
	let pendingPressTimer: ReturnType<typeof setTimeout> | null = null;

	const cancelPendingPress = () => {
		if (pendingPressTimer !== null) {
			clearTimeout(pendingPressTimer);
			pendingPressTimer = null;
		}
	};

	const unlistenRecorderActive = await listen<HotkeyRecorderActivePayload>(
		"hotkey-recorder-active",
		(event) => {
			isRecorderActive = event.payload.active;
			if (isRecorderActive) cancelPendingPress();
		}
	);

	const unlistenPressed = await listen<SystemKeyPressedPayload>("system-key-pressed", (event) => {
		if (!systemKeySetsMatch(event.payload.pressedKeys, expectedKeys)) {
			// Another key joined the chord — a pending match was a transient
			// state (e.g. Karabiner posting a hyperkey's modifiers one by one),
			// not a deliberate press of this hotkey.
			cancelPendingPress();
			return;
		}
		if (isHeld) return;
		if (isRecorderActive) return;

		// Remappers like Karabiner-Elements synthesize modifier sequences that
		// can pass through this exact combination for a millisecond on their
		// way to a larger chord. Only fire once the match survives a short
		// stability window.
		cancelPendingPress();
		pendingPressTimer = setTimeout(() => {
			pendingPressTimer = null;
			isHeld = true;
			handlers.onPressed();
		}, HOTKEY_MATCH_STABILITY_MS);
	});

	const unlistenReleased = await listen("system-keys-released", () => {
		cancelPendingPress();
		if (isHeld) handlers.onReleased?.();
		isHeld = false;
	});

	const unlistenKeyReleased = await listen<SystemKeyReleasedPayload>(
		"system-key-released",
		(event) => {
			if (systemKeySetsMatch(event.payload.pressedKeys, expectedKeys)) return;
			cancelPendingPress();
			if (isHeld) handlers.onReleased?.();
			isHeld = false;
		}
	);

	return {
		hotkey,
		dispose: async () => {
			cancelPendingPress();
			unlistenRecorderActive();
			unlistenPressed();
			unlistenKeyReleased();
			unlistenReleased();
		},
	};
}

async function registerGlobalHotkey(
	hotkey: string,
	handlers: HotkeyHandlers
): Promise<RegisteredHotkey> {
	await Result.tryPromise(() => unregister(hotkey));

	await register(hotkey, (event: GlobalShortcutEvent) => {
		if (event.state === "Pressed") {
			handlers.onPressed();
		} else if (event.state === "Released") {
			handlers.onReleased?.();
		}
	});

	return {
		hotkey,
		dispose: async () => {
			await Result.tryPromise(() => unregister(hotkey));
		},
	};
}

/**
 * Register the dictation hotkey using the best backend for the configured hotkey.
 * Combo hotkeys use Tauri global-shortcut; supported system-only hotkeys use
 * the native macOS system key watcher.
 */
export async function registerDictationHotkey(
	hotkey: string,
	handler: HotkeyHandler
): Promise<void> {
	await unregisterDictationHotkey();

	registeredDictationHotkey = isSystemOnlyHotkey(hotkey)
		? await registerSystemHotkey(hotkey, { onPressed: handler })
		: await registerGlobalHotkey(hotkey, { onPressed: handler });
}

export async function registerDictationHotkeys(
	hotkeys: { hotkey: string; onPressed: HotkeyHandler; onReleased?: HotkeyHandler }[]
): Promise<void> {
	await unregisterDictationHotkey();

	const registered: RegisteredHotkey[] = [];
	const result = await Result.tryPromise(async () => {
		for (const config of hotkeys) {
			registered.push(
				isSystemOnlyHotkey(config.hotkey)
					? await registerSystemHotkey(config.hotkey, config)
					: await registerGlobalHotkey(config.hotkey, config)
			);
		}
	});
	if (Result.isError(result)) {
		for (const registration of registered) {
			await registration.dispose();
		}
		throw result.error;
	}
	registeredDictationHotkeys = registered;
}

/**
 * Unregister the currently active dictation hotkey, regardless of backend.
 */
export async function unregisterDictationHotkey(): Promise<void> {
	const registered = registeredDictationHotkey;
	const registeredMany = registeredDictationHotkeys;
	if (!registered && registeredMany.length === 0) return;

	registeredDictationHotkey = null;
	registeredDictationHotkeys = [];
	if (registered) await registered.dispose();
	for (const registration of registeredMany) {
		await registration.dispose();
	}
}

/**
 * Get a user-friendly display name for a hotkey string.
 * Handles combo shortcuts like "Command+;" by mapping modifier names to symbols.
 */
export function hotkeyDisplayName(hotkey: string): string {
	return hotkey
		.replace(/Command/g, "\u2318")
		.replace(/Control/g, "\u2303")
		.replace(/Option/g, "\u2325")
		.replace(/Alt/g, "\u2325")
		.replace(/Shift/g, "\u21E7");
}

/**
 * Check if a hotkey is valid for hands-free (press) mode.
 * Returns null if valid, error message string if invalid.
 */
export function validateHandsFreeHotkey(hotkey: string): string | null {
	if (isValidHotkey(hotkey)) return null;
	return "Use modifier keys plus one regular key.";
}

/**
 * Check if a hotkey is valid for hold-to-speak mode.
 * FN alone is not allowed as it conflicts with system functionality.
 * Returns null if valid, error message string if invalid.
 */
export function validateHoldToSpeakHotkey(hotkey: string): string | null {
	if (hotkey === "Fn") {
		return "FN cannot be used as hold-to-speak hotkey";
	}
	return validateHandsFreeHotkey(hotkey);
}
