import { useNavigate } from "@solidjs/router";
import { listen } from "@tauri-apps/api/event";
import { type ParentProps, Show, createSignal, onCleanup, onMount } from "solid-js";
import appIcon from "../src-tauri/icons/icon.svg";
import Sidebar from "./components/Navigation";
import SettingsModal from "./components/SettingsModal";
import OnboardingWizard from "./components/onboarding/OnboardingWizard";
import { checkModelStatus } from "./lib/commands/model";
import { MODEL_DOWNLOAD_STEP } from "./lib/onboarding";
import { capture } from "./lib/posthog";
import {
	initSettings,
	markOnboardingComplete,
	resumeOnboardingAt,
	updateMicrophone,
	useSettings,
} from "./lib/settingsStore";

const FORCE_SHOW_ONBOARDING =
	import.meta.env.DEV && import.meta.env.VITE_FORCE_ONBOARDING === "true";

type TauriWindow = Window & {
	__TAURI_INTERNALS__?: unknown;
};

function waitForTauriIPC(): Promise<void> {
	if ((window as TauriWindow).__TAURI_INTERNALS__) return Promise.resolve();

	return new Promise((resolve, reject) => {
		const timeout = setTimeout(() => {
			clearInterval(interval);
			reject(new Error("Tauri IPC bridge not ready after 5s"));
		}, 5000);

		const interval = setInterval(() => {
			if ((window as TauriWindow).__TAURI_INTERNALS__) {
				clearInterval(interval);
				clearTimeout(timeout);
				resolve();
			}
		}, 50);
	});
}

function App(props: ParentProps) {
	const settings = useSettings();
	const navigate = useNavigate();
	const [isSettingsOpen, setIsSettingsOpen] = createSignal(false);
	const [isReady, setIsReady] = createSignal(false);

	const shouldShowOnboarding = () => {
		if (FORCE_SHOW_ONBOARDING) return true;
		return !settings().onboardingComplete;
	};

	onMount(async () => {
		capture("app_opened");
		await waitForTauriIPC();
		await initSettings();

		try {
			const modelReady = await checkModelStatus();
			if (!modelReady && settings().onboardingComplete) {
				await resumeOnboardingAt(MODEL_DOWNLOAD_STEP);
			}
		} catch (err) {
			console.error("Failed to verify Whisper model state:", err);
		}

		setIsReady(true);

		const unlistenNavigate = await listen<string>("navigate", (event) => {
			navigate(event.payload);
		});
		onCleanup(() => unlistenNavigate());

		const unlistenMicrophone = await listen<string>("select-microphone", async (event) => {
			const deviceName = event.payload;
			await updateMicrophone(deviceName || null);
		});
		onCleanup(() => unlistenMicrophone());

		const unlistenAccessibility = await listen("accessibility-permission-needed", () => {
			setIsSettingsOpen(true);
		});
		onCleanup(() => unlistenAccessibility());

		const handleKeyDown = (e: KeyboardEvent) => {
			if (e.metaKey && e.key === ",") {
				e.preventDefault();
				setIsSettingsOpen(true);
			}
		};
		window.addEventListener("keydown", handleKeyDown);
		onCleanup(() => window.removeEventListener("keydown", handleKeyDown));
	});

	return (
		<div class="relative min-h-screen h-full w-full bg-th-base transition-colors">
			<div
				class="pointer-events-none absolute inset-0 z-0"
				style={{
					"background-image":
						"linear-gradient(var(--color-grid-line) 1px, transparent 1px), linear-gradient(90deg, var(--color-grid-line) 1px, transparent 1px)",
					"background-size": "40px 40px",
				}}
			/>
			<div class="absolute top-0 left-0 right-0 h-6 z-50" data-tauri-drag-region />
			<Show when={!isReady()}>
				<div class="h-full flex flex-col items-center justify-center">
					<img src={appIcon} alt="VoxFusion" class="w-16 h-16 mb-8" />
					<div class="w-48 h-1 bg-border overflow-hidden">
						<div class="w-1/4 h-full bg-ac animate-slide" />
					</div>
				</div>
			</Show>
			<Show when={isReady()}>
				<Show
					when={!shouldShowOnboarding()}
					fallback={
						<OnboardingWizard
							initialStep={settings().onboardingStep}
							onComplete={() => {
								capture("onboarding_completed");
								markOnboardingComplete();
							}}
						/>
					}
				>
					<div class="flex h-full">
						<Sidebar onOpenSettings={() => setIsSettingsOpen(true)} />
						<main class="flex-1 overflow-auto pt-6">{props.children}</main>
					</div>
					<SettingsModal isOpen={isSettingsOpen()} onClose={() => setIsSettingsOpen(false)} />
				</Show>
			</Show>
		</div>
	);
}

export default App;
