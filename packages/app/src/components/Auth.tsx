import { openUrl } from "@tauri-apps/plugin-opener";
import { createSignal, Show } from "solid-js";
import { authClient } from "../lib/authClient";

function Auth() {
	const [showTokenInput, setShowTokenInput] = createSignal(false);
	const [token, setToken] = createSignal("");
	const [error, setError] = createSignal("");

	const handleGoogleLogin = async () => {
		const data = await authClient.signIn.social({
			provider: "google",
			callbackURL: "http://localhost:3000/api/deeplink",
			disableRedirect: true,
		});

		console.log(data.data?.url);
		if (data.data?.url) {
			await openUrl(data.data.url);
		}
	};

	const handleTokenSubmit = async () => {
		const tokenValue = token().trim();
		if (!tokenValue) {
			setError("Please enter a token");
			return;
		}

		try {
			// Set the auth token cookie
			document.cookie = `better-auth.session_token=${tokenValue}; path=/`;
			// Refetch session to update auth state
			await authClient.getSession();
			setError("");
		} catch (e) {
			setError("Invalid token");
			console.error("Token auth failed:", e);
		}
	};

	return (
		<div class="flex items-center justify-center min-h-full w-full">
			<div class="flex flex-col items-center gap-8 p-12 bg-white rounded-2xl shadow-xl border border-slate-200">
				<div class="flex flex-col items-center gap-2">
					<h1 class="text-3xl font-bold text-slate-800 tracking-tight">Welcome to VoxFusion</h1>
					<p class="text-slate-500 text-sm">Sign in to continue</p>
				</div>

				<button
					type="button"
					onClick={handleGoogleLogin}
					class="group flex items-center gap-3 px-6 py-3.5 bg-white border-2 border-slate-200 rounded-xl hover:border-blue-500 hover:shadow-lg hover:shadow-blue-100 transition-all duration-200 cursor-pointer"
				>
					<svg
						class="w-5 h-5"
						viewBox="0 0 24 24"
						xmlns="http://www.w3.org/2000/svg"
						aria-labelledby="google-icon-title"
					>
						<title id="google-icon-title">Google logo</title>
						<path
							d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"
							fill="#4285F4"
						/>
						<path
							d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"
							fill="#34A853"
						/>
						<path
							d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"
							fill="#FBBC05"
						/>
						<path
							d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"
							fill="#EA4335"
						/>
					</svg>
					<span class="text-slate-700 font-medium group-hover:text-blue-600 transition-colors">
						Continue with Google
					</span>
				</button>

				<Show when={showTokenInput()}>
					<div class="w-full flex flex-col gap-3">
						<input
							type="text"
							value={token()}
							onInput={(e) => setToken(e.currentTarget.value)}
							placeholder="Paste token here..."
							class="w-full px-4 py-2.5 border-2 border-slate-200 rounded-lg text-sm font-mono focus:border-blue-500 focus:outline-none"
						/>
						<Show when={error()}>
							<p class="text-red-500 text-sm">{error()}</p>
						</Show>
						<button
							type="button"
							onClick={handleTokenSubmit}
							class="px-4 py-2.5 bg-blue-500 text-white rounded-lg font-medium hover:bg-blue-600 transition-colors cursor-pointer"
						>
							Submit Token
						</button>
					</div>
				</Show>

				<button
					type="button"
					onClick={() => setShowTokenInput(!showTokenInput())}
					class="text-slate-400 text-xs hover:text-slate-600 transition-colors cursor-pointer"
				>
					{showTokenInput() ? "Hide token input" : "Dev: Paste token manually"}
				</button>
			</div>
		</div>
	);
}

export default Auth;
