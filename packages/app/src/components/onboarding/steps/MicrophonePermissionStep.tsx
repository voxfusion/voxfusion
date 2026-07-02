import { openUrl } from "@tauri-apps/plugin-opener";
import { Result } from "better-result";
import { AlertCircle, Check, ExternalLink, Mic } from "lucide-solid";
import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { useI18n } from "../../../i18n";

const MICROPHONE_SETTINGS_URL =
	"x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";
const PERMISSION_POLL_INTERVAL_MS = 1000;

interface MicrophonePermissionStepProps {
	onPermissionChange: (granted: boolean) => void;
}

export default function MicrophonePermissionStep(props: MicrophonePermissionStepProps) {
	const [t] = useI18n();
	const [isGranted, setIsGranted] = createSignal<boolean | null>(null);
	const [isRequesting, setIsRequesting] = createSignal(false);
	const [isDenied, setIsDenied] = createSignal(false);
	let pollInterval: ReturnType<typeof setInterval> | undefined;
	let disposed = false;

	const stopPolling = () => {
		if (pollInterval) {
			clearInterval(pollInterval);
			pollInterval = undefined;
		}
	};

	const startPolling = () => {
		if (disposed || pollInterval) return;
		pollInterval = setInterval(checkPermission, PERMISSION_POLL_INTERVAL_MS);
	};

	const checkPermission = async () => {
		const result = await Result.tryPromise(() =>
			navigator.permissions.query({
				name: "microphone" as PermissionName,
			})
		);
		if (Result.isError(result)) {
			setIsGranted(false);
			props.onPermissionChange(false);
			return;
		}
		const granted = result.value.state === "granted";
		if (result.value.state === "denied") {
			setIsDenied(true);
		}
		if (granted) {
			setIsDenied(false);
			stopPolling();
		}
		setIsGranted(granted);
		props.onPermissionChange(granted);
	};

	onMount(async () => {
		await checkPermission();
		// Keep polling so the step advances by itself once access is granted
		// in System Settings.
		if (!disposed && isGranted() !== true) {
			startPolling();
		}
	});

	onCleanup(() => {
		disposed = true;
		stopPolling();
	});

	const handleRequest = async () => {
		setIsRequesting(true);
		const stream = await Result.tryPromise(() =>
			navigator.mediaDevices.getUserMedia({ audio: true })
		);
		if (Result.isOk(stream)) {
			for (const track of stream.value.getTracks()) {
				track.stop();
			}
			setIsDenied(false);
			setIsGranted(true);
			props.onPermissionChange(true);
			stopPolling();
		} else {
			// A NotAllowedError is a permanent denial on macOS: re-requesting can
			// never succeed, the user has to flip the switch in System Settings.
			if (stream.error instanceof DOMException && stream.error.name === "NotAllowedError") {
				setIsDenied(true);
			}
			setIsGranted(false);
			props.onPermissionChange(false);
			startPolling();
		}
		setIsRequesting(false);
	};

	const handleOpenSystemSettings = async () => {
		await Result.tryPromise(() => openUrl(MICROPHONE_SETTINGS_URL));
		startPolling();
	};

	return (
		<div class="text-center max-w-md mx-auto">
			{/* Terminal-style header */}
			<div class="font-mono text-ac text-sm mb-8 tracking-wider">[STEP_01] &gt; MIC_PERMISSION</div>

			{/* Card container */}
			<div class="border border-border bg-th-surface p-8">
				<div class="w-16 h-16 border border-border-strong flex items-center justify-center mx-auto mb-6">
					<Mic class="w-8 h-8 text-ac" />
				</div>

				<h2 class="font-mono text-xl uppercase tracking-wider text-txt-primary mb-3">
					{t("onboarding.micPermissionTitle")}
				</h2>

				<p class="font-mono text-sm text-txt-secondary mb-8">
					{t("onboarding.micPermissionDescription")}
				</p>

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
							<span>{t("onboarding.micPermissionGranted")}</span>
						</div>
					</Show>
					<Show when={isGranted() === false}>
						<div class="flex items-center justify-center gap-2 font-mono text-sm text-ac">
							<AlertCircle class="w-5 h-5" />
							<span>{t("onboarding.micPermissionNotGranted")}</span>
						</div>
						<Show when={isDenied()}>
							<p class="mt-4 font-mono text-xs text-txt-muted border-l-2 border-border-strong pl-3 text-left">
								{t("onboarding.micPermissionDenied")}
							</p>
						</Show>
					</Show>
				</div>

				<Show when={isGranted() !== true}>
					<Show
						when={isDenied()}
						fallback={
							<button
								type="button"
								onClick={handleRequest}
								disabled={isRequesting() || isGranted() === null}
								class="px-6 py-3 bg-ac text-ac-on font-mono font-bold uppercase tracking-wider text-sm hover:bg-ac-hover transition-colors disabled:opacity-30"
							>
								{isRequesting()
									? t("onboarding.checkingPermission")
									: t("onboarding.grantMicPermission")}
							</button>
						}
					>
						<button
							type="button"
							onClick={handleOpenSystemSettings}
							class="inline-flex items-center gap-2 px-6 py-3 bg-ac text-ac-on font-mono font-bold uppercase tracking-wider text-sm hover:bg-ac-hover transition-colors"
						>
							<span>{t("onboarding.openSystemPreferences")}</span>
							<ExternalLink class="w-4 h-4" />
						</button>
					</Show>
				</Show>
			</div>
		</div>
	);
}
