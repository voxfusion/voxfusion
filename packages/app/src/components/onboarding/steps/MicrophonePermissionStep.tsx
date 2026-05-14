import { AlertCircle, Check, Mic } from "lucide-solid";
import { Show, createSignal, onMount } from "solid-js";
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
			setIsGranted(false);
			props.onPermissionChange(false);
		}
	};

	onMount(() => {
		checkPermission();
	});

	const handleRequest = async () => {
		setIsRequesting(true);
		try {
			const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
			for (const track of stream.getTracks()) {
				track.stop();
			}
			setIsGranted(true);
			props.onPermissionChange(true);
		} catch {
			setIsGranted(false);
			props.onPermissionChange(false);
		}
		setIsRequesting(false);
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
					</Show>
				</div>

				<Show when={isGranted() !== true}>
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
				</Show>
			</div>
		</div>
	);
}
