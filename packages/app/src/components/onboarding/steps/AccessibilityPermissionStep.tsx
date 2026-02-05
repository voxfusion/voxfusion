import { createSignal, onMount, onCleanup, Show } from "solid-js";
import { Shield, Check, AlertCircle, ExternalLink } from "lucide-solid";
import { useI18n } from "../../../i18n";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { requestAccessibilityPermission } from "tauri-plugin-macos-permissions-api";

interface AccessibilityPermissionStepProps {
	onPermissionChange: (granted: boolean) => void;
}

export default function AccessibilityPermissionStep(props: AccessibilityPermissionStepProps) {
	const [t] = useI18n();
	const [isGranted, setIsGranted] = createSignal<boolean | null>(null);
	const [isRequesting, setIsRequesting] = createSignal(false);
	let pollInterval: ReturnType<typeof setInterval> | undefined;
	let unlistenFn: UnlistenFn | undefined;

	const markGranted = () => {
		setIsGranted(true);
		props.onPermissionChange(true);
		if (pollInterval) {
			clearInterval(pollInterval);
			pollInterval = undefined;
		}
	};

	const checkPermission = async () => {
		try {
			const granted = await invoke<boolean>("check_accessibility_probe");
			setIsGranted(granted);
			props.onPermissionChange(granted);
			if (granted && pollInterval) {
				clearInterval(pollInterval);
				pollInterval = undefined;
			}
		} catch {
			setIsGranted(false);
			props.onPermissionChange(false);
		}
	};

	const checkWithRetries = async () => {
		// Try immediately
		await checkPermission();
		if (isGranted()) return;

		// Retry after 200ms (let TCC settle)
		await new Promise((r) => setTimeout(r, 200));
		await checkPermission();
		if (isGranted()) return;

		// Retry after 500ms more
		await new Promise((r) => setTimeout(r, 500));
		await checkPermission();
		if (isGranted()) return;

		// Final retry after 1s more — if probes are fully cached,
		// trust the notification since it only fires on an actual toggle
		await new Promise((r) => setTimeout(r, 1000));
		await checkPermission();
		if (!isGranted()) {
			// Both CGEventTapCreate and AXIsProcessTrusted are cached.
			// The notification itself is reliable — it fires when the user
			// toggles the switch. Trust it.
			markGranted();
		}
	};

	onMount(async () => {
		await checkPermission();

		if (!isGranted()) {
			// Poll as fallback
			pollInterval = setInterval(checkPermission, 1000);

			// Listen for macOS distributed notification when the user
			// toggles accessibility in System Settings
			unlistenFn = await listen("accessibility-changed", () => {
				checkWithRetries();
			});
		}
	});

	onCleanup(() => {
		if (pollInterval) {
			clearInterval(pollInterval);
		}
		if (unlistenFn) {
			unlistenFn();
		}
	});

	const handleOpenSettings = async () => {
		setIsRequesting(true);
		try {
			await requestAccessibilityPermission();
		} catch {
			// ignore - settings may still open
		}
		if (!pollInterval) {
			pollInterval = setInterval(checkPermission, 1000);
		}
		setIsRequesting(false);
	};

	return (
		<div class="text-center max-w-md mx-auto">
			{/* Terminal-style header */}
			<div class="font-mono text-[#ff3e00] text-sm mb-8 tracking-wider">
				[STEP_03] &gt; ACCESSIBILITY_PERMISSION
			</div>

			{/* Card container */}
			<div class="border border-[#222] bg-[#111] p-8">
				<div class="w-16 h-16 border border-[#333] flex items-center justify-center mx-auto mb-6">
					<Shield class="w-8 h-8 text-[#ff3e00]" />
				</div>

				<h2 class="font-mono text-xl uppercase tracking-wider text-[#e0e0e0] mb-3">
					{t("onboarding.accessibilityTitle")}
				</h2>

				<p class="font-mono text-sm text-[#888] mb-4">
					{t("onboarding.accessibilityDescription")}
				</p>

				<Show when={isGranted() === false}>
					<p class="font-mono text-xs text-[#666] mb-6 border-l-2 border-[#333] pl-3 text-left">
						{t("onboarding.accessibilityInstructions")}
					</p>
				</Show>

				<div class="mb-6">
					<Show when={isGranted() === null}>
						<div class="flex items-center justify-center gap-2 font-mono text-sm text-[#666]">
							<div class="w-4 h-4 border-2 border-[#666] border-t-transparent rounded-full animate-spin" />
							<span>{t("onboarding.checkingPermission")}</span>
						</div>
					</Show>
					<Show when={isGranted() === true}>
						<div class="flex items-center justify-center gap-2 font-mono text-sm text-[#00ff88]">
							<Check class="w-5 h-5" />
							<span>{t("onboarding.accessibilityGranted")}</span>
						</div>
					</Show>
					<Show when={isGranted() === false}>
						<div class="flex items-center justify-center gap-2 font-mono text-sm text-[#ff3e00]">
							<AlertCircle class="w-5 h-5" />
							<span>{t("onboarding.accessibilityNotGranted")}</span>
						</div>
					</Show>
				</div>

				<Show when={isGranted() !== true}>
					<button
						type="button"
						onClick={handleOpenSettings}
						disabled={isRequesting() || isGranted() === null}
						class="inline-flex items-center gap-2 px-6 py-3 bg-[#ff3e00] text-black font-mono font-bold uppercase tracking-wider text-sm hover:bg-[#ff5722] transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
					>
						<span>{t("onboarding.openSystemPreferences")}</span>
						<ExternalLink class="w-4 h-4" />
					</button>
				</Show>
			</div>
		</div>
	);
}
