import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { Result } from "better-result";
import { AlertCircle, Check, ExternalLink, Shield } from "lucide-solid";
import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { requestAccessibilityPermission } from "tauri-plugin-macos-permissions-api";
import { useI18n } from "../../../i18n";
import { checkAccessibilityProbe } from "../../../lib/commands/permissions";

interface AccessibilityPermissionStepProps {
	onPermissionChange: (granted: boolean) => void;
}

export default function AccessibilityPermissionStep(props: AccessibilityPermissionStepProps) {
	const [t] = useI18n();
	const [isGranted, setIsGranted] = createSignal<boolean | null>(null);
	const [isRequesting, setIsRequesting] = createSignal(false);
	let pollInterval: ReturnType<typeof setInterval> | undefined;
	let unlistenFn: UnlistenFn | undefined;
	let disposed = false;

	const markGranted = () => {
		setIsGranted(true);
		props.onPermissionChange(true);
		if (pollInterval) {
			clearInterval(pollInterval);
			pollInterval = undefined;
		}
	};

	const checkPermission = async () => {
		const granted = await checkAccessibilityProbe();
		if (Result.isError(granted)) {
			setIsGranted(false);
			props.onPermissionChange(false);
			return;
		}
		setIsGranted(granted.value);
		props.onPermissionChange(granted.value);
		if (granted.value && pollInterval) {
			clearInterval(pollInterval);
			pollInterval = undefined;
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
			// AXIsProcessTrusted can lag behind the Settings notification.
			// The notification itself is reliable — it fires when the user
			// toggles the switch. Trust it.
			markGranted();
		}
	};

	onMount(async () => {
		await checkPermission();
		if (disposed) return;

		if (!isGranted()) {
			// Poll as fallback
			pollInterval = setInterval(checkPermission, 1000);

			// Listen for macOS distributed notification when the user
			// toggles accessibility in System Settings. The component may
			// unmount while `listen` resolves, so guard with the disposed flag.
			const unlisten = await listen("accessibility-changed", () => {
				checkWithRetries();
			});
			if (disposed) {
				unlisten();
			} else {
				unlistenFn = unlisten;
			}
		}
	});

	onCleanup(() => {
		disposed = true;
		if (pollInterval) {
			clearInterval(pollInterval);
		}
		if (unlistenFn) {
			unlistenFn();
		}
	});

	const handleOpenSettings = async () => {
		setIsRequesting(true);
		await Result.tryPromise(() => requestAccessibilityPermission());
		if (!pollInterval) {
			pollInterval = setInterval(checkPermission, 1000);
		}
		setIsRequesting(false);
	};

	return (
		<div class="text-center max-w-md mx-auto">
			{/* Terminal-style header */}
			<div class="font-mono text-ac text-sm mb-8 tracking-wider">
				[STEP_02] &gt; ACCESSIBILITY_PERMISSION
			</div>

			{/* Card container */}
			<div class="border border-border bg-th-surface p-8">
				<div class="w-16 h-16 border border-border-strong flex items-center justify-center mx-auto mb-6">
					<Shield class="w-8 h-8 text-ac" />
				</div>

				<h2 class="font-mono text-xl uppercase tracking-wider text-txt-primary mb-3">
					{t("onboarding.accessibilityTitle")}
				</h2>

				<p class="font-mono text-sm text-txt-secondary mb-4">
					{t("onboarding.accessibilityDescription")}
				</p>

				<Show when={isGranted() === false}>
					<p class="font-mono text-xs text-txt-muted mb-6 border-l-2 border-border-strong pl-3 text-left">
						{t("onboarding.accessibilityInstructions")}
					</p>
				</Show>

				<div class="mb-6">
					<Show when={isGranted() === null}>
						<div class="flex items-center justify-center gap-2 font-mono text-sm text-txt-muted">
							<div class="w-4 h-4 border-2 border-txt-muted border-t-transparent rounded-full animate-spin" />
							<span>{t("onboarding.checkingPermission")}</span>
						</div>
					</Show>
					<Show when={isGranted() === true}>
						<div class="flex items-center justify-center gap-2 font-mono text-sm text-success">
							<Check class="w-5 h-5" />
							<span>{t("onboarding.accessibilityGranted")}</span>
						</div>
					</Show>
					<Show when={isGranted() === false}>
						<div class="flex items-center justify-center gap-2 font-mono text-sm text-ac">
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
						class="inline-flex items-center gap-2 px-6 py-3 bg-ac text-ac-on font-mono font-bold uppercase tracking-wider text-sm hover:bg-ac-hover transition-colors disabled:opacity-30"
					>
						<span>{t("onboarding.openSystemPreferences")}</span>
						<ExternalLink class="w-4 h-4" />
					</button>
				</Show>
			</div>
		</div>
	);
}
