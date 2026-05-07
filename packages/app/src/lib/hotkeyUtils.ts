import { listen } from "@tauri-apps/api/event";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";

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

const SYSTEM_HOTKEY_TO_KEY = {
	Fn: "fn",
	LeftControl: "leftControl",
	RightControl: "rightControl",
	LeftOption: "leftOption",
	RightOption: "rightOption",
	LeftCommand: "leftCommand",
	RightCommand: "rightCommand",
} satisfies Record<string, SystemKey>;

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

	return SYSTEM_HOTKEY_ORDER.filter((part) => keySet.has(SYSTEM_HOTKEY_TO_KEY[part])).join("+");
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

async function registerSystemHotkey(hotkey: string, handler: HotkeyHandler): Promise<RegisteredHotkey> {
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
		},
	);

	const unlistenPressed = await listen<SystemKeyPressedPayload>("system-key-pressed", (event) => {
		if (!systemKeySetsMatch(event.payload.pressedKeys, expectedKeys)) return;
		if (isHeld) return;
		if (isRecorderActive) return;

		isHeld = true;
		handler();
	});

	const unlistenReleased = await listen("system-keys-released", () => {
		isHeld = false;
	});

	const unlistenKeyReleased = await listen<SystemKeyReleasedPayload>(
		"system-key-released",
		(event) => {
			if (systemKeySetsMatch(event.payload.pressedKeys, expectedKeys)) return;
			isHeld = false;
		},
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

async function registerGlobalHotkey(hotkey: string, handler: HotkeyHandler): Promise<RegisteredHotkey> {
	try {
		await unregister(hotkey);
	} catch {}

	await register(hotkey, (event: GlobalShortcutEvent) => {
		if (event.state !== "Pressed") return;
		handler();
	});

	return {
		hotkey,
		dispose: async () => {
			try {
				await unregister(hotkey);
			} catch {}
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
	handler: HotkeyHandler,
): Promise<void> {
	await unregisterDictationHotkey();

	registeredDictationHotkey = isSystemOnlyHotkey(hotkey)
		? await registerSystemHotkey(hotkey, handler)
		: await registerGlobalHotkey(hotkey, handler);
}

/**
 * Unregister the currently active dictation hotkey, regardless of backend.
 */
export async function unregisterDictationHotkey(): Promise<void> {
	const registered = registeredDictationHotkey;
	if (!registered) return;

	registeredDictationHotkey = null;
	await registered.dispose();
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
