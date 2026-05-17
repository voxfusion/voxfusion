import { listen } from "@tauri-apps/api/event";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { Result } from "better-result";

type SystemKey =
	| "fn"
	| "leftControl"
	| "rightControl"
	| "leftOption"
	| "rightOption"
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

const COMBO_MODIFIER_NAMES = new Set(["Command", "Control", "Alt", "Shift", "CapsLock", "Fn"]);

const SYSTEM_HOTKEY_TO_KEY: Record<string, SystemKey> = {
	Fn: "fn",
	LeftControl: "leftControl",
	RightControl: "rightControl",
	LeftOption: "leftOption",
	RightOption: "rightOption",
	LeftCommand: "leftCommand",
	RightCommand: "rightCommand",
};

const SYSTEM_HOTKEY_ORDER = [
	"Fn",
	"LeftControl",
	"RightControl",
	"LeftOption",
	"RightOption",
	"LeftCommand",
	"RightCommand",
];

const MODIFIER_CODE_TO_SYSTEM_HOTKEY: Record<string, string> = {
	Fn: "Fn",
	ControlLeft: "LeftControl",
	ControlRight: "RightControl",
	AltLeft: "LeftOption",
	AltRight: "RightOption",
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
	if (!hotkey.includes("+")) return false;

	const parts = hotkey.split("+");
	if (parts.some((part) => part in SYSTEM_HOTKEY_TO_KEY)) return false;

	return parts.some((part) => !COMBO_MODIFIER_NAMES.has(part));
}

async function registerSystemHotkey(
	hotkey: string,
	handlers: HotkeyHandlers
): Promise<RegisteredHotkey> {
	const expectedKeys = systemKeysFromHotkey(hotkey);
	if (!expectedKeys) {
		throw new Error(`Unsupported system hotkey: ${hotkey}`);
	}

	let isHeld = false;
	let isRecorderActive = false;

	const unlistenRecorderActive = await listen<HotkeyRecorderActivePayload>(
		"hotkey-recorder-active",
		(event) => {
			isRecorderActive = event.payload.active;
		}
	);

	const unlistenPressed = await listen<SystemKeyPressedPayload>("system-key-pressed", (event) => {
		if (!systemKeySetsMatch(event.payload.pressedKeys, expectedKeys)) return;
		if (isHeld) return;
		if (isRecorderActive) return;

		isHeld = true;
		handlers.onPressed();
	});

	const unlistenReleased = await listen("system-keys-released", () => {
		if (isHeld) handlers.onReleased?.();
		isHeld = false;
	});

	const unlistenKeyReleased = await listen<SystemKeyReleasedPayload>(
		"system-key-released",
		(event) => {
			if (systemKeySetsMatch(event.payload.pressedKeys, expectedKeys)) return;
			if (isHeld) handlers.onReleased?.();
			isHeld = false;
		}
	);

	return {
		hotkey,
		dispose: async () => {
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
export function validateHandsFreeHotkey(_hotkey: string): string | null {
	return null;
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
	return null;
}
