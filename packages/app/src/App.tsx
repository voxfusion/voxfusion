import { onMount } from "solid-js";
import { onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { getAllWebviewWindows } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, primaryMonitor } from "@tauri-apps/api/window";
import { tokenManager } from "./lib/tokenManager";
import { authClient } from "./lib/authClient";
import Auth from "./components/Auth";
import { appLocalDataDir } from "@tauri-apps/api/path";

function App() {
	onMount(async () => {
		console.log("mounded");
		const token = await tokenManager.getToken();
		console.log("path", await appLocalDataDir());
		console.log("token", token);
		if (token) {
			document.cookie = `better-auth.session_token=${token}; path=/`;
			const session = await authClient.getSession();
			console.log("session", session);
		}

		onOpenUrl(async (url) => {
			console.log("url", url);
		});
	});

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
				<Auth />
				{/* <Show when={!session.isPending}> */}
				{/* <Show when={session.data?.user} fallback={<Auth />}> */}
				{/* {props.children} */}
				{/* </Show>
				</Show> */}
			</div>
		</div>
	);
}

export default App;
