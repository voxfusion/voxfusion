import { createSignal, onCleanup, onMount, type ParentProps, Show } from "solid-js";
import { getAllWebviewWindows } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, primaryMonitor } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "@solidjs/router";
import { useStore } from "@nanostores/solid";
import { authClient } from "./lib/authClient";
import { initSettings, useSettings, markOnboardingComplete, updateMicrophone } from "./lib/settingsStore";
import tauriconf from "../src-tauri/tauri.conf.json";
import Auth from "./components/Auth";
import Sidebar from "./components/Navigation";
import SettingsModal from "./components/SettingsModal";
import OnboardingWizard from "./components/onboarding/OnboardingWizard";
import UpdateNotification from "./components/UpdateNotification";

const FORCE_SHOW_ONBOARDING =
	import.meta.env.DEV && import.meta.env.VITE_FORCE_ONBOARDING === "true";

function App(props: ParentProps) {
	const session = useStore(authClient.useSession);
	const settings = useSettings();
	const navigate = useNavigate();
	const [isSettingsOpen, setIsSettingsOpen] = createSignal(false);
	console.log("session", session());

	const shouldShowOnboarding = () => {
		if (FORCE_SHOW_ONBOARDING) return true;
		return !settings().onboardingComplete;
	};

	onMount(async () => {
		// Initialize settings from store
		await initSettings();

		// Listen for tray menu events
		const unlistenNavigate = await listen<string>("navigate", (event) => {
			navigate(event.payload);
		});
		onCleanup(() => unlistenNavigate());

		const unlistenMicrophone = await listen<string>("select-microphone", async (event) => {
			const deviceName = event.payload;
			await updateMicrophone(deviceName || null);
		});
		onCleanup(() => unlistenMicrophone());

		// Register Cmd+, shortcut to open settings
		const handleKeyDown = (e: KeyboardEvent) => {
			if (e.metaKey && e.key === ",") {
				e.preventDefault();
				setIsSettingsOpen(true);
			}
		};
		window.addEventListener("keydown", handleKeyDown);
		onCleanup(() => window.removeEventListener("keydown", handleKeyDown));

		const voicecontrolWindow = tauriconf.app.windows.find(w => w.label === "voice-control");
		if (!voicecontrolWindow) {
			console.error("Voice control window not found");
			throw new Error("Voice control window not found");
		}

		const windowWidth = voicecontrolWindow.width;
		const windowHeight = voicecontrolWindow.height;
		const bottomPadding = 20

		const monitor = await primaryMonitor();
		if (!monitor) {
			console.error("No primary monitor found");
			return;
		}

		const allWindows = await getAllWebviewWindows();
		const voiceWindow = allWindows.find((w) => w.label === "voice-control");
		if (!voiceWindow) {
			console.error("Voice control window not found");
			return;
		}

		const position = monitor.position.toLogical(monitor.scaleFactor);
		const size = monitor.size.toLogical(monitor.scaleFactor);

		const x = position.x + (size.width - windowWidth) / 2;
		const y = position.y + size.height - windowHeight - bottomPadding;

		await voiceWindow.setPosition(new LogicalPosition(x, y));

		await voiceWindow.show();
	});

	return (
		<div class="relative min-h-screen h-full w-full bg-slate-100 dark:bg-midnight-900 transition-colors">
			<div class="absolute top-0 left-0 right-0 h-6 z-50" data-tauri-drag-region />
			<Show when={session()?.isPending}>
				<div class="h-full flex flex-col items-center justify-center">
					<div class="w-16 h-16 bg-slate-300 rounded-2xl mb-8" />
					<div class="w-48 h-1 bg-slate-200 rounded-full overflow-hidden">
						<div class="w-1/4 h-full bg-slate-400 rounded-full animate-slide" />
					</div>
				</div>
			</Show>
			<Show when={!session()?.isPending}>
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
						fallback={<OnboardingWizard onComplete={() => markOnboardingComplete()} />}
					>
						<div class="flex h-full">
							<Sidebar onOpenSettings={() => setIsSettingsOpen(true)} />
							<main class="flex-1 overflow-auto pt-6">{props.children}</main>
						</div>
						<SettingsModal isOpen={isSettingsOpen()} onClose={() => setIsSettingsOpen(false)} />
						<UpdateNotification />
					</Show>
				</Show>
			</Show>
		</div>
	);
}

export default App;
