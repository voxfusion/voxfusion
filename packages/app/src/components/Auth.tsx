import { createSignal, Show } from "solid-js";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Loader } from "lucide-solid";
import { GoogleIcon } from "../icons/GoogleIcon";
import { authClient, API_BASE_URL } from "../lib/authClient";
import { tokenManager } from "../lib/tokenManager";
import { useI18n } from "../i18n";

function Auth() {
	const [t] = useI18n();
	const [devToken, setDevToken] = createSignal("");
	const [isLoading, setIsLoading] = createSignal(false);

	const handleGoogleLogin = async () => {
		setIsLoading(true);
		try {
			console.log("Starting Google login...");
			const data = await authClient.signIn.social({
				provider: "google",
				callbackURL: `${API_BASE_URL}/api/deeplink`,
				disableRedirect: true,
			});

			console.log("Auth response:", data);

			if (data.error) {
				console.error("Auth error:", data.error);
				return;
			}

			if (data.data?.url) {
				console.log("Opening URL:", data.data.url);
				await openUrl(data.data.url);
			} else {
				console.error("No URL returned from auth server");
			}
		} catch (error) {
			console.error("Google login failed:", error);
		}
	};

	const handleDevTokenSubmit = async () => {
		const token = devToken().trim();
		if (token) {
			await tokenManager.storeToken(token);
			window.location.reload();
		}
	};

	return (
		<div class="flex items-center justify-center min-h-full w-full">
			<div class="flex flex-col items-center gap-8 p-12 bg-white dark:bg-midnight-800 rounded-2xl shadow-xl border border-slate-200 dark:border-midnight-700 transition-colors">
				<div class="flex flex-col items-center gap-2">
					<h1 class="text-3xl font-bold text-slate-800 dark:text-white tracking-tight">{t("auth.welcome")}</h1>
					<p class="text-slate-500 dark:text-slate-400 text-sm">{t("auth.signInToContinue")}</p>
				</div>

				<button
					type="button"
					onClick={handleGoogleLogin}
					disabled={isLoading()}
					class="group flex items-center gap-3 px-6 py-3.5 bg-white dark:bg-midnight-700 border-2 border-slate-200 dark:border-midnight-600 rounded-xl hover:border-primary-500 dark:hover:border-primary-500 hover:shadow-lg hover:shadow-primary-100 dark:hover:shadow-primary-900/20 transition-all duration-200 cursor-pointer disabled:opacity-60 disabled:cursor-not-allowed disabled:hover:border-slate-200 disabled:dark:hover:border-midnight-600 disabled:hover:shadow-none"
				>
					<Show
						when={!isLoading()}
						fallback={<Loader class="w-5 h-5 animate-spin text-slate-500 dark:text-slate-400" />}
					>
						<GoogleIcon class="w-5 h-5" />
					</Show>
					<span class="text-slate-700 dark:text-slate-200 font-medium group-hover:text-primary-600 dark:group-hover:text-primary-400 transition-colors">
						{t("auth.continueWithGoogle")}
					</span>
				</button>

				{import.meta.env.DEV && (
					<div class="flex flex-col gap-3 w-full mt-4 pt-6 border-t border-slate-200 dark:border-midnight-600">
						<p class="text-xs text-slate-500 dark:text-slate-400 text-center">
							{t("auth.devPasteToken")}
						</p>
						<input
							type="text"
							value={devToken()}
							onInput={(e) => setDevToken(e.currentTarget.value)}
							placeholder={t("auth.pasteTokenPlaceholder")}
							class="px-4 py-2.5 border-2 border-slate-200 dark:border-midnight-600 bg-white dark:bg-midnight-700 text-slate-900 dark:text-slate-100 rounded-xl focus:outline-none focus:border-primary-500 transition-colors text-sm font-mono"
						/>
						<button
							type="button"
							onClick={handleDevTokenSubmit}
							class="px-4 py-2.5 bg-primary-500 text-white rounded-xl hover:bg-primary-600 transition-colors text-sm font-medium"
						>
							{t("auth.authenticate")}
						</button>
					</div>
				)}
			</div>
		</div>
	);
}

export default Auth;
