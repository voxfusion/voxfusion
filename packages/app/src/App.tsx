import type { ParentProps } from "solid-js";
import { Show, onMount, onCleanup, createSignal, createMemo } from "solid-js";
import { useStore } from "@nanostores/solid";
import { useLocation } from "@solidjs/router";
import { listen } from "@tauri-apps/api/event";
import { onOpenUrl, getCurrent } from "@tauri-apps/plugin-deep-link";
import { authClient } from "./lib/authClient";
import Auth from "./components/Auth";

// Debug signal to show deep link status
const [debugInfo, setDebugInfo] = createSignal<string[]>([]);

function App(props: ParentProps) {
	const session = useStore(authClient.useSession)();
	const location = useLocation();

	// Check if this is the voice control window
	const isVoiceControl = createMemo(() => location.pathname === "/voice-control");

	onMount(async () => {
		const addDebug = (msg: string) => {
			console.log("[DeepLink]", msg);
			setDebugInfo((prev) => [...prev.slice(-4), msg]);
		};

		addDebug("App mounted, setting up deep link handlers...");

		// Handle deep link URLs
		const handleDeepLink = async (url: string) => {
			addDebug(`Received: ${url.substring(0, 50)}...`);
			try {
				const urlObj = new URL(url);
				const token = urlObj.searchParams.get("token");
				if (token) {
					addDebug("Token found, authenticating...");
					console.log("Token found, authenticating...", token);
					// Set the auth token/cookie
					document.cookie = `better-auth.session_token=${token}; path=/`;
					// Refetch session to update auth state
					const session = await authClient.getSession();
					console.log("Session:", session);
					addDebug("Authentication complete!");
				} else {
					addDebug("No token in URL");
				}
			} catch (e) {
				addDebug(`Error: ${e}`);
			}
		};

		// Check if app was opened with a deep link (handles cold start)
		try {
			addDebug("Checking for pending URLs...");
			const urls = await getCurrent();
			if (urls && urls.length > 0) {
				addDebug(`Found ${urls.length} pending URL(s)`);
				for (const url of urls) {
					await handleDeepLink(url);
				}
			} else {
				addDebug("No pending URLs");
			}
		} catch (e) {
			addDebug(`getCurrent error: ${e}`);
		}

		// Listen for deep links via plugin (handles when app is already running)
		addDebug("Setting up onOpenUrl listener...");
		const unlistenPlugin = await onOpenUrl((urls) => {
			addDebug(`onOpenUrl triggered with ${urls.length} URL(s)`);
			for (const url of urls) {
				handleDeepLink(url);
			}
		});

		// Also listen for deep links via custom event (from single-instance)
		const unlistenEvent = await listen<string>("deep-link", (event) => {
			addDebug("deep-link event received");
			handleDeepLink(event.payload);
		});

		addDebug("Deep link setup complete");

		onCleanup(() => {
			unlistenPlugin();
			unlistenEvent();
		});
	});

	// Voice control window has a minimal transparent layout
	if (isVoiceControl()) {
		return <div class="bg-transparent">{props.children}</div>;
	}

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
