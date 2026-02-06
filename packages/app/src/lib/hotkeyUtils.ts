/**
 * Modifier-only shortcut names that map to specific physical modifier keys.
 * These are used when the user records a single modifier key as their shortcut.
 */
const MODIFIER_ONLY_SHORTCUTS = new Set([
	"LeftCommand",
	"RightCommand",
	"LeftShift",
	"RightShift",
	"LeftOption",
	"RightOption",
	"LeftControl",
	"RightControl",
	"CapsLock",
	"Fn",
]);

/**
 * Maps KeyboardEvent.code values for modifier keys to our internal shortcut names.
 */
const CODE_TO_MODIFIER_NAME: Record<string, string> = {
	MetaLeft: "LeftCommand",
	MetaRight: "RightCommand",
	ShiftLeft: "LeftShift",
	ShiftRight: "RightShift",
	AltLeft: "LeftOption",
	AltRight: "RightOption",
	ControlLeft: "LeftControl",
	ControlRight: "RightControl",
	CapsLock: "CapsLock",
	Fn: "Fn",
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
 * Maps stored hotkey names to display-friendly labels for the settings UI.
 */
const HOTKEY_DISPLAY_NAMES: Record<string, string> = {
	LeftCommand: "Left \u2318",
	RightCommand: "Right \u2318",
	LeftShift: "Left \u21E7",
	RightShift: "Right \u21E7",
	LeftOption: "Left \u2325",
	RightOption: "Right \u2325",
	LeftControl: "Left \u2303",
	RightControl: "Right \u2303",
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

/**
 * Generic modifier names used in multi-modifier combos like "Control+Shift".
 */
const GENERIC_MODIFIER_NAMES = new Set(["Command", "Control", "Alt", "Shift", "CapsLock", "Fn"]);

/**
 * Check if a hotkey string represents a modifier-only shortcut.
 * This includes:
 * - Single modifier shortcuts: "RightCommand", "LeftShift", etc.
 * - Multi-modifier combos: "Control+Shift", "Command+Alt", etc.
 */
export function isModifierOnlyShortcut(hotkey: string): boolean {
	// Single modifier with left/right distinction
	if (MODIFIER_ONLY_SHORTCUTS.has(hotkey)) return true;

	// Multi-modifier combo: every part separated by "+" must be a generic modifier name
	if (hotkey.includes("+")) {
		const parts = hotkey.split("+");
		return parts.length >= 2 && parts.every((part) => GENERIC_MODIFIER_NAMES.has(part));
	}

	return false;
}

/**
 * Check if a KeyboardEvent.code is a modifier key.
 */
export function isModifierCode(code: string): boolean {
	return MODIFIER_CODES.has(code);
}

/**
 * Convert a KeyboardEvent.code for a modifier key to our internal shortcut name.
 * Returns undefined for non-modifier codes.
 */
export function modifierCodeToName(code: string): string | undefined {
	return CODE_TO_MODIFIER_NAME[code];
}

/**
 * Get the display string for a modifier KeyboardEvent.code during recording.
 * Returns undefined for non-modifier codes.
 */
export function modifierCodeToDisplay(code: string): string | undefined {
	return CODE_TO_DISPLAY[code];
}

/**
 * Get a user-friendly display name for a hotkey string.
 * Handles both modifier-only shortcuts and regular combo shortcuts.
 *
 * For regular shortcuts like "Command+;", maps modifier names to symbols.
 * For modifier-only shortcuts like "RightCommand", returns "Right Cmd".
 */
export function hotkeyDisplayName(hotkey: string): string {
	// Check modifier-only shortcuts first
	if (HOTKEY_DISPLAY_NAMES[hotkey]) {
		return HOTKEY_DISPLAY_NAMES[hotkey];
	}

	// For regular shortcuts, replace modifier names with symbols
	return hotkey
		.replace(/Command/g, "\u2318")
		.replace(/Control/g, "\u2303")
		.replace(/Alt/g, "\u2325")
		.replace(/Shift/g, "\u21E7");
}
