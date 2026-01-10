import { createSignal, onMount, Show } from "solid-js";
import { Mic, Check, AlertCircle } from "lucide-solid";
import { useI18n } from "../../../i18n";

interface MicrophonePermissionStepProps {
	onPermissionChange: (granted: boolean) => void;
}

export default function MicrophonePermissionStep(props: MicrophonePermissionStepProps) {
	const [t] = useI18n();
	const [isGranted, setIsGranted] = createSignal<boolean | null>(null);
	const [isRequesting, setIsRequesting] = createSignal(false);

	const checkPermission = async () => {
		try {
			const result = await navigator.permissions.query({
				name: "microphone" as PermissionName,
			});
			const granted = result.state === "granted";
			setIsGranted(granted);
			props.onPermissionChange(granted);
		} catch {
			// Fallback: try to get user media to check
			try {
				const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
				stream.getTracks().forEach((track) => track.stop());
				setIsGranted(true);
				props.onPermissionChange(true);
			} catch {
				setIsGranted(false);
				props.onPermissionChange(false);
			}
		}
	};

	onMount(() => {
		checkPermission();
	});

	const handleRequest = async () => {
		setIsRequesting(true);
		try {
			// Request microphone access - this triggers the native macOS permission dialog
			const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
			// Stop the stream immediately, we just needed to trigger the permission
			stream.getTracks().forEach((track) => track.stop());
			setIsGranted(true);
			props.onPermissionChange(true);
		} catch {
			// User denied or error occurred
			setIsGranted(false);
			props.onPermissionChange(false);
		}
		setIsRequesting(false);
	};

	return (
		<div class="text-center max-w-md mx-auto">
			<div class="w-20 h-20 bg-primary-100 dark:bg-primary-900/30 rounded-full flex items-center justify-center mx-auto mb-6">
				<Mic class="w-10 h-10 text-primary-600 dark:text-primary-400" />
			</div>

			<h2 class="text-2xl font-bold text-slate-900 dark:text-white mb-3">
				{t("onboarding.micPermissionTitle")}
			</h2>

			<p class="text-slate-600 dark:text-slate-400 mb-8">
				{t("onboarding.micPermissionDescription")}
			</p>

			{/* Status indicator */}
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
						<span>{t("onboarding.micPermissionGranted")}</span>
					</div>
				</Show>
				<Show when={isGranted() === false}>
					<div class="flex items-center justify-center gap-2 text-amber-600 dark:text-amber-400">
						<AlertCircle class="w-5 h-5" />
						<span>{t("onboarding.micPermissionNotGranted")}</span>
					</div>
				</Show>
			</div>

			{/* Action button */}
			<Show when={isGranted() !== true}>
				<button
					type="button"
					onClick={handleRequest}
					disabled={isRequesting() || isGranted() === null}
					class="px-6 py-3 bg-primary-500 text-white rounded-xl font-medium hover:bg-primary-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
				>
					{isRequesting() ? t("onboarding.checkingPermission") : t("onboarding.grantMicPermission")}
				</button>
			</Show>
		</div>
	);
}
