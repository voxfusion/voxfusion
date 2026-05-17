import { emit, listen } from "@tauri-apps/api/event";
import { Result } from "better-result";
import { createEffect, createSignal, onCleanup } from "solid-js";
import {
	hotkeyDisplayName,
	isModifierCode,
	modifierCodeToDisplay,
	modifierCodeToSystemHotkey,
	systemHotkeyFromKeys,
} from "../lib/hotkeyUtils";
import { updateHotkey } from "../lib/settingsStore";

let activeRecordingId: string | null = null;

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

interface UseHotkeyRecorderOptions {
	onHotkeyRecorded?: (hotkey: string) => Promise<void>;
	validator?: (hotkey: string) => string | null;
	recorderId?: string;
}

export function useHotkeyRecorder(options: UseHotkeyRecorderOptions = {}) {
	const recorderId = options.recorderId || Math.random().toString(36);
	const [isRecording, setIsRecording] = createSignal(false);
	const [pendingHotkey, setPendingHotkey] = createSignal("");
	const [error, setError] = createSignal<string | null>(null);

	// Track all currently held modifier codes for combo shortcuts
	let heldModifierCodes = new Set<string>();
	let pendingSystemHotkey = "";

	const resetTracking = () => {
		heldModifierCodes = new Set<string>();
		pendingSystemHotkey = "";
	};

	const setRecorderActive = (active: boolean) => {
		void Result.tryPromise(() => emit("hotkey-recorder-active", { active }));
	};

	const saveRecordedHotkey = async (hotkey: string) => {
		const validationError = options.validator?.(hotkey);
		if (validationError) {
			setError(validationError);
			return false;
		}
		setError(null);
		const saved = await Result.tryPromise(async () => {
			await (options.onHotkeyRecorded?.(hotkey) ?? updateHotkey(hotkey));
		});
		if (Result.isError(saved)) {
			return false;
		}
		return true;
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
			// A modifier key was pressed — show current modifier state
			heldModifierCodes.add(code);
			setPendingHotkey(buildModifierDisplay(heldModifierCodes));
		} else {
			// A non-modifier key was pressed — build a standard combo shortcut string
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
			const systemHotkey = modifierCodeToSystemHotkey(code);
			if (systemHotkey && pendingSystemHotkey) {
				heldModifierCodes.delete(code);
				return;
			}

			if (heldModifierCodes.size === 1 && heldModifierCodes.has(code) && systemHotkey) {
				const success = await saveRecordedHotkey(systemHotkey);
				if (success) {
					setIsRecording(false);
					setPendingHotkey("");
					resetTracking();
					setRecorderActive(false);
				}
				return;
			}

			heldModifierCodes.delete(code);

			// When all modifiers are released without a combo, just reset display
			if (heldModifierCodes.size === 0) {
				setPendingHotkey("");
			} else {
				// Update display with remaining modifiers
				setPendingHotkey(buildModifierDisplay(heldModifierCodes));
			}
		} else {
			// A non-modifier key was released — check if we have a valid combo shortcut
			const pending = pendingHotkey();
			if (pending?.includes("+")) {
				const success = await saveRecordedHotkey(pending);
				if (success) {
					setIsRecording(false);
					setPendingHotkey("");
					resetTracking();
					setRecorderActive(false);
				}
			}
		}
	};

	const unlistenSystemPressedPromise = listen<SystemKeyPressedPayload>(
		"system-key-pressed",
		(event) => {
			if (!isRecording()) return;

			pendingSystemHotkey = systemHotkeyFromKeys(event.payload.pressedKeys);
			setPendingHotkey(hotkeyDisplayName(pendingSystemHotkey));
		}
	);

	const unlistenSystemReleasedPromise = listen("system-keys-released", () => {
		if (!isRecording()) return;
		if (!pendingSystemHotkey) return;

		const hotkey = pendingSystemHotkey;
		void Result.tryPromise(() =>
			saveRecordedHotkey(hotkey).then((success) => {
				if (success) {
					setIsRecording(false);
					setPendingHotkey("");
					resetTracking();
					setRecorderActive(false);
				}
			})
		);
	});

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
		if (activeRecordingId === recorderId) {
			activeRecordingId = null;
		}
		setRecorderActive(false);
		void Result.tryPromise(() => unlistenSystemPressedPromise.then((unlisten) => unlisten()));
		void Result.tryPromise(() => unlistenSystemReleasedPromise.then((unlisten) => unlisten()));
	});

	return {
		isRecording,
		pendingHotkey,
		error,
		startRecording: () => {
			if (activeRecordingId !== null && activeRecordingId !== recorderId) {
				setError("Another hotkey is being recorded. Please finish or cancel it first.");
				return;
			}
			activeRecordingId = recorderId;
			setIsRecording(true);
			setPendingHotkey("");
			setError(null);
			resetTracking();
			setRecorderActive(true);
		},
		stopRecording: () => {
			if (activeRecordingId === recorderId) {
				activeRecordingId = null;
			}
			setIsRecording(false);
			setPendingHotkey("");
			setError(null);
			resetTracking();
			setRecorderActive(false);
		},
		toggleRecording: () => {
			if (isRecording()) {
				if (activeRecordingId === recorderId) {
					activeRecordingId = null;
				}
				setIsRecording(false);
				setPendingHotkey("");
				setError(null);
				resetTracking();
				setRecorderActive(false);
			} else {
				if (activeRecordingId !== null && activeRecordingId !== recorderId) {
					setError("Another hotkey is being recorded. Please finish or cancel it first.");
					return;
				}
				activeRecordingId = recorderId;
				setIsRecording(true);
				setPendingHotkey("");
				setError(null);
				resetTracking();
				setRecorderActive(true);
			}
		},
	};
}
