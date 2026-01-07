import { onMount, type ParentProps } from "solid-js";
import { getAllWebviewWindows } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, primaryMonitor } from "@tauri-apps/api/window";
import { useStore } from "@nanostores/solid";
import { authClient } from "./lib/authClient";
import Auth from "./components/Auth";
import { Show } from "solid-js";

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
		<div class="flex flex-col min-h-screen h-full w-full bg-slate-100">
			<div class="h-6" data-tauri-drag-region />
			<div class="grow">
				<Show when={!session() || !session().data?.user} fallback={props.children}>
					<Auth />
				</Show>
			</div>
		</div>
	);
}

export default App;
