import { invoke } from "@tauri-apps/api/core";
import { Check, Keyboard, Mic } from "lucide-solid";
import { Show, createSignal, onMount } from "solid-js";
import { getPlatformInfo } from "../../../lib/platform";

export default function LinuxSetupStep(props: {
	kind: "microphone" | "desktop";
	onPermissionChange: (ready: boolean) => void;
}) {
	const [ready, setReady] = createSignal(false);
	const [checking, setChecking] = createSignal(true);
	const [error, setError] = createSignal("");
	const check = async () => {
		setChecking(true);
		setError("");
		try {
			if (props.kind === "microphone") await invoke("check_microphone");
			else {
				const platform = await getPlatformInfo();
				if (platform.wayland && !platform.hyprland)
					throw new Error(
						"Automatic shortcuts require Hyprland. On other Wayland desktops, configure compositor shortcuts using the Linux guide."
					);
				if (!platform.textInsertionAvailable)
					throw new Error(
						"Install wtype to insert transcribed text: sudo pacman -S wtype. Then restart VoxFusion."
					);
			}
			setReady(true);
			props.onPermissionChange(true);
		} catch (cause) {
			setReady(false);
			props.onPermissionChange(false);
			setError(String(cause));
		} finally {
			setChecking(false);
		}
	};
	onMount(() => {
		void check();
	});
	return (
		<div class="text-center max-w-lg mx-auto">
			<div class="font-mono text-ac text-sm mb-8 tracking-wider">
				{props.kind === "microphone" ? "[STEP_01] > LINUX_AUDIO" : "[STEP_02] > LINUX_DESKTOP"}
			</div>
			<div class="border border-border bg-th-surface p-8">
				<div class="w-16 h-16 border border-border-strong flex items-center justify-center mx-auto mb-6">
					<Show when={props.kind === "microphone"} fallback={<Keyboard class="w-8 h-8 text-ac" />}>
						<Mic class="w-8 h-8 text-ac" />
					</Show>
				</div>
				<h2 class="font-mono text-xl uppercase tracking-wider text-txt-primary mb-3">
					{props.kind === "microphone" ? "Microphone on Linux" : "Ready for Omarchy"}
				</h2>
				<p class="font-mono text-sm text-txt-secondary mb-6">
					{props.kind === "microphone"
						? "VoxFusion records from your system audio input. Choose your microphone in the next step. Speech is transcribed locally on your computer."
						: "VoxFusion uses Hyprland shortcuts and wtype to insert text into the focused app. Your shortcuts work while the app is in the tray."}
				</p>
				<Show when={ready()}>
					<div class="flex items-center justify-center gap-2 font-mono text-sm text-success">
						<Check class="w-5 h-5" />
						{props.kind === "microphone" ? "Microphone available" : "Desktop integration available"}
					</div>
				</Show>
				<Show when={error()}>
					<p role="alert" class="font-mono text-sm text-ac mb-5">
						{error()}
					</p>
				</Show>
				<Show when={!ready()}>
					<button
						type="button"
						onClick={check}
						disabled={checking()}
						class="px-6 py-3 bg-ac text-ac-on font-mono uppercase text-sm disabled:opacity-30"
					>
						{checking() ? "Checking…" : "Check again"}
					</button>
				</Show>
				<Show when={props.kind === "desktop"}>
					<p class="mt-6 font-mono text-xs text-txt-muted">
						Default shortcuts: Ctrl + Alt + V to toggle dictation, Ctrl + Alt + B to hold and speak.
						Change them during setup or in Settings.
					</p>
				</Show>
			</div>
		</div>
	);
}
