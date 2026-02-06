import { createEffect, createSignal, onCleanup } from "solid-js";
import { isModifierCode, modifierCodeToDisplay, modifierCodeToName } from "../lib/hotkeyUtils";
import { updateHotkey } from "../lib/settingsStore";

/**
 * Maps KeyboardEvent.code to the modifier prefix used in combo shortcuts
 * (e.g., "Command+K"). These are the generic modifier names without left/right
 * distinction, used when building combo hotkey strings.
 */
function codeToComboModifier(code: string): string | undefined {
	switch (code) {
		case "MetaLeft":
		case "MetaRight":
			return "Command";
		case "ShiftLeft":
		case "ShiftRight":
			return "Shift";
		case "AltLeft":
		case "AltRight":
			return "Alt";
		case "ControlLeft":
		case "ControlRight":
			return "Control";
		default:
			return undefined;
	}
}

/**
 * Consistent ordering for modifier keys when building shortcut strings.
 */
const MOD_ORDER = [
	"MetaLeft",
	"MetaRight",
	"ControlLeft",
	"ControlRight",
	"AltLeft",
	"AltRight",
	"ShiftLeft",
	"ShiftRight",
];

export function useHotkeyRecorder() {
	const [isRecording, setIsRecording] = createSignal(false);
	const [pendingHotkey, setPendingHotkey] = createSignal("");

	// Track if a non-modifier key was pressed (contaminates modifier-only detection)
	let nonModifierPressed = false;
	// Track all currently held modifier codes for combo shortcuts
	let heldModifierCodes = new Set<string>();
	// Track the peak set of modifiers held simultaneously (for multi-modifier shortcuts)
	let peakModifierCodes = new Set<string>();

	const resetTracking = () => {
		nonModifierPressed = false;
		heldModifierCodes = new Set<string>();
		peakModifierCodes = new Set<string>();
	};

	/**
	 * Build the modifier-only shortcut string from the peak set of held modifiers.
	 * For a single modifier: "RightCommand", "LeftShift", etc.
	 * For multiple modifiers: "Control+Shift" (using generic names, consistent order).
	 */
	const buildModifierOnlyString = (codes: Set<string>): string | null => {
		if (codes.size === 0) return null;

		if (codes.size === 1) {
			// Single modifier — use the specific left/right name
			const first = codes.values().next();
			if (first.done) return null;
			return modifierCodeToName(first.value) ?? null;
		}

		// Multiple modifiers — use generic names in consistent order, deduped
		const modifiers: string[] = [];
		const added = new Set<string>();
		for (const mc of MOD_ORDER) {
			if (codes.has(mc)) {
				const generic = codeToComboModifier(mc);
				if (generic && !added.has(generic)) {
					modifiers.push(generic);
					added.add(generic);
				}
			}
		}
		return modifiers.length >= 2 ? modifiers.join("+") : null;
	};

	/**
	 * Build a display string for currently held modifiers.
	 */
	const buildModifierDisplay = (codes: Set<string>): string => {
		if (codes.size === 1) {
			const first = codes.values().next();
			if (first.done) return "";
			return modifierCodeToDisplay(first.value) ?? first.value;
		}
		const parts: string[] = [];
		for (const mc of MOD_ORDER) {
			if (codes.has(mc)) {
				const display = modifierCodeToDisplay(mc);
				if (display) parts.push(display);
			}
		}
		return parts.join(" + ");
	};

	const handleKeyDown = (e: KeyboardEvent) => {
		if (!isRecording()) return;

		e.preventDefault();
		e.stopPropagation();

		const code = e.code;

		if (isModifierCode(code)) {
			// A modifier key was pressed
			heldModifierCodes.add(code);
			// Update peak: track maximum simultaneous modifiers
			if (heldModifierCodes.size > peakModifierCodes.size) {
				peakModifierCodes = new Set(heldModifierCodes);
			}

			if (!nonModifierPressed) {
				// Show current modifier state
				setPendingHotkey(buildModifierDisplay(heldModifierCodes));
			}
		} else {
			// A non-modifier key was pressed
			nonModifierPressed = true;

			// Build a standard combo shortcut string using the held modifiers
			const modifiers: string[] = [];
			const addedModifiers = new Set<string>();

			for (const mc of MOD_ORDER) {
				if (heldModifierCodes.has(mc)) {
					const modName = codeToComboModifier(mc);
					if (modName && !addedModifiers.has(modName)) {
						modifiers.push(modName);
						addedModifiers.add(modName);
					}
				}
			}

			const key = e.key;
			const keyPart = key.length === 1 ? key.toUpperCase() : key;
			const hotkeyString = [...modifiers, keyPart].join("+");
			setPendingHotkey(hotkeyString);
		}
	};

	const handleKeyUp = async (e: KeyboardEvent) => {
		if (!isRecording()) return;

		const code = e.code;

		if (isModifierCode(code)) {
			heldModifierCodes.delete(code);

			// When all modifiers are released and no non-modifier key was pressed,
			// save as modifier-only shortcut (single or multi)
			if (heldModifierCodes.size === 0 && !nonModifierPressed && peakModifierCodes.size > 0) {
				const modString = buildModifierOnlyString(peakModifierCodes);
				if (modString) {
					await updateHotkey(modString);
					setIsRecording(false);
					setPendingHotkey("");
					resetTracking();
					return;
				}
			}

			// If all modifiers released without a valid shortcut, reset for next attempt
			if (heldModifierCodes.size === 0) {
				resetTracking();
				setPendingHotkey("");
			}
		} else {
			// A non-modifier key was released — check if we have a valid combo shortcut
			const pending = pendingHotkey();
			if (pending?.includes("+")) {
				await updateHotkey(pending);
				setIsRecording(false);
				setPendingHotkey("");
				resetTracking();
			}
		}
	};

	createEffect(() => {
		if (isRecording()) {
			resetTracking();
			window.addEventListener("keydown", handleKeyDown);
			window.addEventListener("keyup", handleKeyUp);
		} else {
			window.removeEventListener("keydown", handleKeyDown);
			window.removeEventListener("keyup", handleKeyUp);
		}
	});

	onCleanup(() => {
		window.removeEventListener("keydown", handleKeyDown);
		window.removeEventListener("keyup", handleKeyUp);
	});

	return {
		isRecording,
		pendingHotkey,
		startRecording: () => {
			setIsRecording(true);
			setPendingHotkey("");
			resetTracking();
		},
		stopRecording: () => {
			setIsRecording(false);
			setPendingHotkey("");
			resetTracking();
		},
		toggleRecording: () => {
			if (isRecording()) {
				setIsRecording(false);
				setPendingHotkey("");
				resetTracking();
			} else {
				setIsRecording(true);
				setPendingHotkey("");
				resetTracking();
			}
		},
	};
}
