import type { ParentProps } from "solid-js";
import { onMount, onCleanup } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { onOpenUrl, getCurrent } from "@tauri-apps/plugin-deep-link";
import { getAllWebviewWindows } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, primaryMonitor } from "@tauri-apps/api/window";
import { tokenManager } from "./lib/tokenManager";
import { authClient } from "./lib/authClient";

function App(props: ParentProps) {
	onMount(async () => {
		const handleDeepLink = async (url: string) => {
			try {
				const urlObj = new URL(url);
				const token = urlObj.searchParams.get("token");
				if (token) {
					await tokenManager.storeToken(token);
					document.cookie = `better-auth.session_token=${token}; path=/`;

					const session = await authClient.getSession();
					console.log("Session:", session);
				}
			} catch (e) {
				console.error("Error:", e);
			}
		};

		try {
			const urls = await getCurrent();
			if (urls && urls.length > 0) {
				for (const url of urls) {
					await handleDeepLink(url);
				}
			} else {
			}
		} catch (e) {
			console.error("getCurrent error:", e);
		}

		const unlistenPlugin = await onOpenUrl((urls) => {
			for (const url of urls) {
				handleDeepLink(url);
			}
		});

		const unlistenEvent = await listen<string>("deep-link", (event) => {
			handleDeepLink(event.payload);
		});

		onCleanup(() => {
			unlistenPlugin();
			unlistenEvent();
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
				{/* <Show when={!session.isPending}> */}
				{/* <Show when={session.data?.user} fallback={<Auth />}> */}
				{props.children}
				{/* </Show>
				</Show> */}
			</div>
		</div>
	);
}

export default App;
