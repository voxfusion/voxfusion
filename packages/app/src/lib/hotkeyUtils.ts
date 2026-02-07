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
 * Get a user-friendly display name for a hotkey string.
 * Handles combo shortcuts like "Command+;" by mapping modifier names to symbols.
 */
export function hotkeyDisplayName(hotkey: string): string {
	return hotkey
		.replace(/Command/g, "\u2318")
		.replace(/Control/g, "\u2303")
		.replace(/Alt/g, "\u2325")
		.replace(/Shift/g, "\u21E7");
}
