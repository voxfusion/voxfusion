import { createSignal, onCleanup, onMount, type ParentProps, Show } from "solid-js";
import { getAllWebviewWindows } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, primaryMonitor } from "@tauri-apps/api/window";
import { useStore } from "@nanostores/solid";
import { authClient } from "./lib/authClient";
import { initSettings } from "./lib/settingsStore";
import Auth from "./components/Auth";
import Sidebar from "./components/Navigation";
import SettingsModal from "./components/SettingsModal";

function App(props: ParentProps) {
	const session = useStore(authClient.useSession);
	const [isSettingsOpen, setIsSettingsOpen] = createSignal(false);
	console.log("session", session());

	onMount(async () => {
		// Initialize settings from store
		await initSettings();

		// Register Cmd+, shortcut to open settings
		const handleKeyDown = (e: KeyboardEvent) => {
			if (e.metaKey && e.key === ",") {
				e.preventDefault();
				setIsSettingsOpen(true);
			}
		};
		window.addEventListener("keydown", handleKeyDown);
		onCleanup(() => window.removeEventListener("keydown", handleKeyDown));
		const windowWidth = 140;
		const windowHeight = 40;
		const bottomPadding = 0;

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
