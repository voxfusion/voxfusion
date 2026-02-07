import { openUrl } from "@tauri-apps/plugin-opener";
import { Loader } from "lucide-solid";
import { Show, createSignal } from "solid-js";
import { useI18n } from "../i18n";
import { GoogleIcon } from "../icons/GoogleIcon";
import { API_BASE_URL, authClient } from "../lib/authClient";
import { capture } from "../lib/posthog";
import { tokenManager } from "../lib/tokenManager";

function Auth() {
	const [t] = useI18n();
	const [devToken, setDevToken] = createSignal("");
	const [isLoading, setIsLoading] = createSignal(false);

	const handleGoogleLogin = async () => {
		setIsLoading(true);
		capture("login_started", { provider: "google" });
		try {
			const data = await authClient.signIn.social({
				provider: "google",
				callbackURL: `${API_BASE_URL}/api/deeplink`,
				disableRedirect: true,
			});

			if (data.error) {
				console.error("Auth error:", data.error);
				return;
			}

			if (data.data?.url) {
				await openUrl(data.data.url);
			} else {
				console.error("No URL returned from auth server");
			}
		} catch (error) {
			console.error("Google login failed:", error);
		} finally {
			setIsLoading(false);
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
		<div class="relative flex items-center justify-center min-h-full w-full bg-th-base overflow-hidden">
			{/* Grid overlay pattern */}
			<div
				class="absolute inset-0 opacity-[0.03]"
				style={{
					"background-image": `
						linear-gradient(to right, var(--color-accent) 1px, transparent 1px),
						linear-gradient(to bottom, var(--color-accent) 1px, transparent 1px)
					`,
					"background-size": "40px 40px",
				}}
			/>

			{/* Scanline effect */}
			<div
				class="absolute inset-0 pointer-events-none opacity-[0.02]"
				style={{
					"background-image":
						"repeating-linear-gradient(0deg, transparent, transparent 2px, currentColor 2px, currentColor 4px)",
					color: "var(--color-text-primary)",
				}}
			/>

			<div class="relative z-10 flex flex-col w-full max-w-md mx-4">
				{/* Terminal header */}
				<div class="border border-border-strong border-b-0 bg-th-base px-4 py-3">
					<div class="flex items-center gap-2">
						<span class="text-ac font-mono text-sm">[VOXFUSION]</span>
						<span class="text-txt-muted font-mono text-sm">&gt;</span>
						<span class="text-txt-primary font-mono text-sm uppercase tracking-wider">
							AUTH_REQUIRED
						</span>
						<span class="ml-auto text-txt-muted font-mono text-xs">v0.1.0</span>
					</div>
				</div>

				{/* Main container */}
				<div class="border border-border-strong bg-th-base p-8">
					{/* Status line */}
					<div class="mb-8 font-mono">
						<p class="text-txt-secondary text-xs uppercase tracking-wider mb-2">// STATUS</p>
						<p class="text-txt-primary text-sm">
							<span class="text-ac">&gt;</span> {t("auth.signInToContinue")}
						</p>
					</div>

					{/* Welcome message */}
					<div class="mb-8">
						<h1 class="text-txt-primary font-mono text-2xl uppercase tracking-wider mb-2">
							{t("auth.welcome")}
						</h1>
						<div class="h-[1px] bg-border-strong w-full" />
					</div>

					{/* Google login button */}
					<button
						type="button"
						onClick={handleGoogleLogin}
						disabled={isLoading()}
						class="group w-full flex items-center justify-center gap-3 px-6 py-4 bg-transparent border border-ac font-mono text-sm uppercase tracking-wider text-txt-primary hover:bg-ac hover:text-ac-on transition-all duration-150 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:bg-transparent disabled:hover:text-txt-primary"
					>
						<Show when={!isLoading()} fallback={<Loader class="w-5 h-5 animate-spin" />}>
							<GoogleIcon class="w-5 h-5" />
						</Show>
						<span>{t("auth.continueWithGoogle")}</span>
					</button>

					{/* Terminal output style status */}
					<div class="mt-6 font-mono text-xs">
						<p class="text-txt-muted">
							<span class="text-txt-secondary">[INFO]</span> Secure OAuth 2.0 authentication
						</p>
					</div>

					{/* Dev token section */}
					{import.meta.env.DEV && (
						<div class="mt-8 pt-6 border-t border-border">
							<div class="mb-4 font-mono">
								<p class="text-txt-secondary text-xs uppercase tracking-wider mb-2">// DEV_MODE</p>
								<p class="text-txt-muted text-xs">
									<span class="text-ac">&gt;</span> {t("auth.devPasteToken")}
								</p>
							</div>

							<div class="flex flex-col gap-3">
								<div class="relative">
									<span class="absolute left-3 top-1/2 -translate-y-1/2 text-ac font-mono text-sm">
										$
									</span>
									<input
										type="text"
										value={devToken()}
										onInput={(e) => setDevToken(e.currentTarget.value)}
										placeholder={t("auth.pasteTokenPlaceholder")}
										class="w-full pl-8 pr-4 py-3 bg-th-input border border-border-strong text-txt-primary font-mono text-sm placeholder:text-txt-faint focus:outline-none focus:border-ac transition-colors"
									/>
								</div>
								<button
									type="button"
									onClick={handleDevTokenSubmit}
									class="w-full px-4 py-3 bg-ac text-ac-on font-mono text-sm uppercase tracking-wider hover:bg-ac-hover transition-colors"
								>
									{t("auth.authenticate")}
								</button>
							</div>

							{/* Dev status */}
							<div class="mt-4 font-mono text-xs">
								<p class="text-txt-muted">
									<span class="text-txt-secondary">[WARN]</span> Development mode active
								</p>
							</div>
						</div>
					)}
				</div>

				{/* Footer */}
				<div class="border border-border-strong border-t-0 bg-th-base px-4 py-2">
					<p class="text-txt-faint font-mono text-xs text-center uppercase tracking-wider">
						Press ENTER to continue_
					</p>
				</div>
			</div>
		</div>
	);
}

export default Auth;
