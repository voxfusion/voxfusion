import { useStore } from "@nanostores/solid";
import { useNavigate } from "@solidjs/router";
import { emit, listen } from "@tauri-apps/api/event";
import { getAllWebviewWindows } from "@tauri-apps/api/webviewWindow";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { type ParentProps, Show, createEffect, createSignal, onCleanup, onMount } from "solid-js";
import appIcon from "../src-tauri/icons/icon.svg";
import Auth from "./components/Auth";
import Sidebar from "./components/Navigation";
import SettingsModal from "./components/SettingsModal";
import OnboardingWizard from "./components/onboarding/OnboardingWizard";
import { authClient } from "./lib/authClient";
import { capture, identifyUser } from "./lib/posthog";
import {
	initSettings,
	markOnboardingComplete,
	updateMicrophone,
	useSettings,
} from "./lib/settingsStore";
import { tokenManager } from "./lib/tokenManager";

const FORCE_SHOW_ONBOARDING =
	import.meta.env.DEV && import.meta.env.VITE_FORCE_ONBOARDING === "true";

function waitForTauriIPC(): Promise<void> {
	if ((window as any).__TAURI_INTERNALS__) return Promise.resolve();

	return new Promise((resolve, reject) => {
		const timeout = setTimeout(() => {
			clearInterval(interval);
			reject(new Error("Tauri IPC bridge not ready after 5s"));
		}, 5000);

		const interval = setInterval(() => {
			if ((window as any).__TAURI_INTERNALS__) {
				clearInterval(interval);
				clearTimeout(timeout);
				resolve();
			}
		}, 50);
	});
}

const handleDeepLinkUrls = async (urls: string[]) => {
	await tokenManager.init();

	for (const urlString of urls) {
		try {
			const url = new URL(urlString);
			const token = url.searchParams.get("token");

			if (token) {
				await tokenManager.storeToken(token);
				await authClient.useSession.get().refetch();

				break;
			}
		} catch {
			// Deep link handling failed silently
		}
	}
};

function App(props: ParentProps) {
	const session = useStore(authClient.useSession);
	const settings = useSettings();
	const navigate = useNavigate();
	const [isSettingsOpen, setIsSettingsOpen] = createSignal(false);
	const [isSessionChecked, setIsSessionChecked] = createSignal(false);

	createEffect(() => {
		const s = session();
		if (s?.data?.user) {
			identifyUser(s.data.user.id, {
				email: s.data.user.email,
				name: s.data.user.name,
			});
		}
	});

	const shouldShowOnboarding = () => {
		if (FORCE_SHOW_ONBOARDING) return true;
		return !settings().onboardingComplete;
	};

	onMount(async () => {
		capture("app_opened");

		await waitForTauriIPC();
		await tokenManager.init();
		await initSettings();

		try {
			const storedToken = await tokenManager.getToken();
			if (storedToken) {
				await authClient.useSession.get().refetch();
			}
		} catch {
			// Session restoration failed silently
		}
		setIsSessionChecked(true);

		// Notify other windows of current auth state after init completes.
		// Also respond to future auth-request events from windows that
		// start after this emit.
		await emit("auth-changed");
		const unlistenAuthRequest = await listen("auth-request", async () => {
			await emit("auth-changed");
		});
		onCleanup(() => unlistenAuthRequest());

		const initialUrls = await getCurrent();
		if (initialUrls) {
			await handleDeepLinkUrls(initialUrls);
		}

		const unlistenDeepLink = await onOpenUrl((urls) => {
			handleDeepLinkUrls(urls).catch(() => {});
		});
		onCleanup(() => unlistenDeepLink());

		const unlistenNavigate = await listen<string>("navigate", (event) => {
			navigate(event.payload);
		});
		onCleanup(() => unlistenNavigate());

		const unlistenMicrophone = await listen<string>("select-microphone", async (event) => {
			const deviceName = event.payload;
			await updateMicrophone(deviceName || null);
		});
		onCleanup(() => unlistenMicrophone());

		// If the voice-control window reports that a modifier-only shortcut
		// could not be registered because Accessibility permission is missing,
		// open the settings modal so the user sees the relevant section.
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

		const allWindows = await getAllWebviewWindows();
		const voiceWindow = allWindows.find((w) => w.label === "voice-control");
		if (voiceWindow) {
			await voiceWindow.show();
		}
	});

	return (
		<div class="relative min-h-screen h-full w-full bg-th-base transition-colors">
			{/* Grid overlay pattern */}
			<div
				class="pointer-events-none absolute inset-0 z-0"
				style={{
					"background-image": `linear-gradient(var(--color-grid-line) 1px, transparent 1px), linear-gradient(90deg, var(--color-grid-line) 1px, transparent 1px)`,
					"background-size": "40px 40px",
				}}
			/>
			<div class="absolute top-0 left-0 right-0 h-6 z-50" data-tauri-drag-region />
			<Show when={!isSessionChecked() || session()?.isPending}>
				<div class="h-full flex flex-col items-center justify-center">
					<img src={appIcon} alt="VoxFusion" class="w-16 h-16 mb-8" />
					<div class="w-48 h-1 bg-border overflow-hidden">
						<div class="w-1/4 h-full bg-ac animate-slide" />
					</div>
				</div>
			</Show>
			<Show when={isSessionChecked() && !session()?.isPending}>
				<Show
					when={session()?.data?.user}
					fallback={
						<div class="h-full pt-6">
							<Auth />
						</div>
					}
				>
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
			</Show>
		</div>
	);
}

export default App;
