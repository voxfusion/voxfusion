import { createSignal, onMount, onCleanup, Show } from "solid-js";
import { Shield, Check, AlertCircle, ExternalLink } from "lucide-solid";
import { useI18n } from "../../../i18n";
import { invoke } from "@tauri-apps/api/core";
import { requestAccessibilityPermission } from "tauri-plugin-macos-permissions-api";

interface AccessibilityPermissionStepProps {
	onPermissionChange: (granted: boolean) => void;
}

export default function AccessibilityPermissionStep(props: AccessibilityPermissionStepProps) {
	const [t] = useI18n();
	const [isGranted, setIsGranted] = createSignal<boolean | null>(null);
	const [isRequesting, setIsRequesting] = createSignal(false);
	let pollInterval: ReturnType<typeof setInterval> | undefined;

	const checkPermission = async () => {
		try {
			const granted = await invoke<boolean>("check_accessibility_probe");
			console.log(granted);
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

	onMount(async () => {
		await checkPermission();
		if (!isGranted() && !pollInterval) {
			pollInterval = setInterval(checkPermission, 1000);
		}
	});

	onCleanup(() => {
		if (pollInterval) {
			clearInterval(pollInterval);
		}
	});

	const handleOpenSettings = async () => {
		setIsRequesting(true);
		try {
			await requestAccessibilityPermission();
			pollInterval = setInterval(checkPermission, 1000);
		} catch {
			pollInterval = setInterval(checkPermission, 1000);
		}
		setIsRequesting(false);
	};

	return (
		<div class="text-center max-w-md mx-auto">
			<div class="w-20 h-20 bg-primary-100 dark:bg-primary-900/30 rounded-full flex items-center justify-center mx-auto mb-6">
				<Shield class="w-10 h-10 text-primary-600 dark:text-primary-400" />
			</div>

			<h2 class="text-2xl font-bold text-slate-900 dark:text-white mb-3">
				{t("onboarding.accessibilityTitle")}
			</h2>

			<p class="text-slate-600 dark:text-slate-400 mb-4">
				{t("onboarding.accessibilityDescription")}
			</p>

			<Show when={isGranted() === false}>
				<p class="text-sm text-slate-500 dark:text-slate-500 mb-6 italic">
					{t("onboarding.accessibilityInstructions")}
				</p>
			</Show>

			<div class="mb-6">
				<Show when={isGranted() === null}>
					<div class="flex items-center justify-center gap-2 text-slate-500 dark:text-slate-400">
						<div class="w-4 h-4 border-2 border-slate-400 dark:border-slate-500 border-t-transparent rounded-full animate-spin" />
						<span>{t("onboarding.checkingPermission")}</span>
					</div>
				</Show>
				<Show when={isGranted() === true}>
					<div class="flex items-center justify-center gap-2 text-green-600 dark:text-green-400">
						<Check class="w-5 h-5" />
						<span>{t("onboarding.accessibilityGranted")}</span>
					</div>
				</Show>
				<Show when={isGranted() === false}>
					<div class="flex items-center justify-center gap-2 text-amber-600 dark:text-amber-400">
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
					class="inline-flex items-center gap-2 px-6 py-3 bg-primary-500 text-white rounded-xl font-medium hover:bg-primary-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
				>
					<span>{t("onboarding.openSystemPreferences")}</span>
					<ExternalLink class="w-4 h-4" />
				</button>
			</Show>
		</div>
	);
}
