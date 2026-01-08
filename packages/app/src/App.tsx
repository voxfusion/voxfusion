import { onMount, type ParentProps, Show } from "solid-js";
import { getAllWebviewWindows } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, primaryMonitor } from "@tauri-apps/api/window";
import { useStore } from "@nanostores/solid";
import { authClient } from "./lib/authClient";
import Auth from "./components/Auth";
import Sidebar from "./components/Navigation";

function App(props: ParentProps) {
	const session = useStore(authClient.useSession);
	console.log("session", session());

	onMount(async () => {
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
		<div class="relative min-h-screen h-full w-full bg-slate-100">
			<div class="absolute top-0 left-0 right-0 h-6 z-50" data-tauri-drag-region />
			<Show
				when={session()?.data?.user}
				fallback={
					<div class="h-full pt-6">
						<Auth />
					</div>
				}
			>
				<div class="flex h-full">
					<Sidebar />
					<main class="flex-1 overflow-auto pt-6">{props.children}</main>
				</div>
			</Show>
		</div>
	);
}

export default App;
