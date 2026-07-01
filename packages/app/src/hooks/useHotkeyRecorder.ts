import { emit, listen } from "@tauri-apps/api/event";
import { Result } from "better-result";
import { createEffect, createSignal, onCleanup } from "solid-js";
import { startSystemKeyWatcher } from "../lib/commands/permissions";
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

const CODE_TO_HOTKEY_KEY: Record<string, string> = {
	Backquote: "`",
	Backslash: "\\",
	BracketLeft: "[",
	BracketRight: "]",
	Comma: ",",
	Equal: "=",
	Minus: "-",
	Period: ".",
	Quote: "'",
	Semicolon: ";",
	Slash: "/",
	Space: "Space",
	Tab: "Tab",
	Enter: "Enter",
	Escape: "Escape",
	Backspace: "Backspace",
	Delete: "Delete",
	CapsLock: "CapsLock",
	ArrowDown: "ArrowDown",
	ArrowLeft: "ArrowLeft",
	ArrowRight: "ArrowRight",
	ArrowUp: "ArrowUp",
};

interface UseHotkeyRecorderOptions {
	onHotkeyRecorded?: (hotkey: string) => Promise<void>;
	validator?: (hotkey: string) => string | null;
	recorderId?: string;
}

function keyEventToHotkeyKey(e: KeyboardEvent): string {
	if (/^Key[A-Z]$/.test(e.code)) return e.code.slice(3);
	if (/^Digit[0-9]$/.test(e.code)) return e.code.slice(5);
	if (/^Numpad/.test(e.code)) return e.code;
	if (/^F[0-9]{1,2}$/.test(e.code)) return e.code;
	return CODE_TO_HOTKEY_KEY[e.code] ?? (e.key.length === 1 ? e.key.toUpperCase() : e.key);
}

export function useHotkeyRecorder(options: UseHotkeyRecorderOptions = {}) {
	const recorderId = options.recorderId || Math.random().toString(36);
	const [isRecording, setIsRecording] = createSignal(false);
	const [pendingHotkey, setPendingHotkey] = createSignal("");
	const [error, setError] = createSignal<string | null>(null);

	// Track all currently held modifier codes for combo shortcuts
	let heldModifierCodes = new Set<string>();
	let pendingSystemHotkey = "";
	let isStartingWatcher = false;

	const resetTracking = () => {
		heldModifierCodes = new Set<string>();
		pendingSystemHotkey = "";
	};

	const setRecorderActive = (active: boolean) => {
		void Result.tryPromise(() => emit("hotkey-recorder-active", { active }));
	};

	/**
	 * Tear down the recording session. Releases the shared `activeRecordingId`
	 * lock so another recorder can start — this must run on every completion
	 * path (successful save *and* cancel), otherwise the lock stays held and
	 * the next recorder is wrongly told "another hotkey is being recorded".
	 */
	const finishRecording = () => {
		if (activeRecordingId === recorderId) {
			activeRecordingId = null;
		}
		setIsRecording(false);
		setPendingHotkey("");
		resetTracking();
		setRecorderActive(false);
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

			const keyPart = keyEventToHotkeyKey(e);
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
					finishRecording();
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
					finishRecording();
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
					finishRecording();
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

	const startRecording = async () => {
		if (activeRecordingId !== null && activeRecordingId !== recorderId) {
			setError("Another hotkey is being recorded. Please finish or cancel it first.");
			return;
		}
		if (isStartingWatcher) return;
		isStartingWatcher = true;
		const watcherStarted = await startSystemKeyWatcher();
		isStartingWatcher = false;
		if (Result.isError(watcherStarted)) {
			setError(watcherStarted.error.message);
			return;
		}
		activeRecordingId = recorderId;
		setIsRecording(true);
		setPendingHotkey("");
		setError(null);
		resetTracking();
		setRecorderActive(true);
	};

	const stopRecording = () => {
		finishRecording();
		setError(null);
	};

	return {
		isRecording,
		pendingHotkey,
		error,
		startRecording,
		stopRecording,
		toggleRecording: () => {
			if (isRecording()) {
				stopRecording();
			} else {
				void startRecording();
			}
		},
	};
}
