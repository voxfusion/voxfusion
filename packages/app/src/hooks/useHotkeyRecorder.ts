import { createSignal, createEffect, onCleanup } from "solid-js";
import { updateHotkey } from "../lib/settingsStore";

export function useHotkeyRecorder() {
	const [isRecording, setIsRecording] = createSignal(false);
	const [pendingHotkey, setPendingHotkey] = createSignal("");

	const handleKeyDown = (e: KeyboardEvent) => {
		if (!isRecording()) return;

		e.preventDefault();
		e.stopPropagation();

		const modifiers: string[] = [];
		if (e.metaKey) modifiers.push("Command");
		if (e.ctrlKey) modifiers.push("Control");
		if (e.altKey) modifiers.push("Alt");
		if (e.shiftKey) modifiers.push("Shift");

		const key = e.key;
		// Ignore pure modifier keys
		if (["Meta", "Control", "Alt", "Shift"].includes(key)) {
			return;
		}

		const hotkeyString = [...modifiers, key.length === 1 ? key.toUpperCase() : key].join("+");
		setPendingHotkey(hotkeyString);
	};

	const handleKeyUp = async () => {
		if (!isRecording()) return;

		const pending = pendingHotkey();
		if (pending && pending.includes("+")) {
			await updateHotkey(pending);
			setIsRecording(false);
			setPendingHotkey("");
		}
	};

	createEffect(() => {
		if (isRecording()) {
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
		},
		stopRecording: () => {
			setIsRecording(false);
			setPendingHotkey("");
		},
		toggleRecording: () => {
			if (isRecording()) {
				setIsRecording(false);
				setPendingHotkey("");
			} else {
				setIsRecording(true);
				setPendingHotkey("");
			}
		},
	};
}
