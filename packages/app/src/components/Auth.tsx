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
		<div class="relative flex items-center justify-center min-h-full w-full bg-[#0a0a0a] overflow-hidden">
			{/* Grid overlay pattern */}
			<div
				class="absolute inset-0 opacity-[0.03]"
				style={{
					"background-image": `
						linear-gradient(to right, #ff3e00 1px, transparent 1px),
						linear-gradient(to bottom, #ff3e00 1px, transparent 1px)
					`,
					"background-size": "40px 40px",
				}}
			/>

			{/* Scanline effect */}
			<div
				class="absolute inset-0 pointer-events-none opacity-[0.02]"
				style={{
					"background-image": "repeating-linear-gradient(0deg, transparent, transparent 2px, #fff 2px, #fff 4px)",
				}}
			/>

			<div class="relative z-10 flex flex-col w-full max-w-md mx-4">
				{/* Terminal header */}
				<div class="border border-[#333] border-b-0 bg-[#0a0a0a] px-4 py-3">
					<div class="flex items-center gap-2">
						<span class="text-[#ff3e00] font-mono text-sm">[VOXFUSION]</span>
						<span class="text-[#666] font-mono text-sm">&gt;</span>
						<span class="text-[#e0e0e0] font-mono text-sm uppercase tracking-wider">AUTH_REQUIRED</span>
						<span class="ml-auto text-[#666] font-mono text-xs">v1.0.0</span>
					</div>
				</div>

				{/* Main container */}
				<div class="border border-[#333] bg-[#0a0a0a] p-8">
					{/* Status line */}
					<div class="mb-8 font-mono">
						<p class="text-[#888] text-xs uppercase tracking-wider mb-2">// STATUS</p>
						<p class="text-[#e0e0e0] text-sm">
							<span class="text-[#ff3e00]">&gt;</span> {t("auth.signInToContinue")}
						</p>
					</div>

					{/* Welcome message */}
					<div class="mb-8">
						<h1 class="text-[#e0e0e0] font-mono text-2xl uppercase tracking-wider mb-2">
							{t("auth.welcome")}
						</h1>
						<div class="h-[1px] bg-[#333] w-full" />
					</div>

					{/* Google login button */}
					<button
						type="button"
						onClick={handleGoogleLogin}
						disabled={isLoading()}
						class="group w-full flex items-center justify-center gap-3 px-6 py-4 bg-transparent border border-[#ff3e00] font-mono text-sm uppercase tracking-wider text-[#e0e0e0] hover:bg-[#ff3e00] hover:text-[#0a0a0a] transition-all duration-150 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:bg-transparent disabled:hover:text-[#e0e0e0]"
					>
						<Show
							when={!isLoading()}
							fallback={<Loader class="w-5 h-5 animate-spin" />}
						>
							<GoogleIcon class="w-5 h-5" />
						</Show>
						<span>{t("auth.continueWithGoogle")}</span>
					</button>

					{/* Terminal output style status */}
					<div class="mt-6 font-mono text-xs">
						<p class="text-[#666]">
							<span class="text-[#888]">[INFO]</span> Secure OAuth 2.0 authentication
						</p>
					</div>

					{/* Dev token section */}
					{import.meta.env.DEV && (
						<div class="mt-8 pt-6 border-t border-[#222]">
							<div class="mb-4 font-mono">
								<p class="text-[#888] text-xs uppercase tracking-wider mb-2">// DEV_MODE</p>
								<p class="text-[#666] text-xs">
									<span class="text-[#ff3e00]">&gt;</span> {t("auth.devPasteToken")}
								</p>
							</div>

							<div class="flex flex-col gap-3">
								<div class="relative">
									<span class="absolute left-3 top-1/2 -translate-y-1/2 text-[#ff3e00] font-mono text-sm">$</span>
									<input
										type="text"
										value={devToken()}
										onInput={(e) => setDevToken(e.currentTarget.value)}
										placeholder={t("auth.pasteTokenPlaceholder")}
										class="w-full pl-8 pr-4 py-3 bg-[#0a0a0a] border border-[#333] text-[#e0e0e0] font-mono text-sm placeholder:text-[#444] focus:outline-none focus:border-[#ff3e00] transition-colors"
									/>
								</div>
								<button
									type="button"
									onClick={handleDevTokenSubmit}
									class="w-full px-4 py-3 bg-[#ff3e00] text-[#0a0a0a] font-mono text-sm uppercase tracking-wider hover:bg-[#e03800] transition-colors"
								>
									{t("auth.authenticate")}
								</button>
							</div>

							{/* Dev status */}
							<div class="mt-4 font-mono text-xs">
								<p class="text-[#666]">
									<span class="text-[#888]">[WARN]</span> Development mode active
								</p>
							</div>
						</div>
					)}
				</div>

				{/* Footer */}
				<div class="border border-[#333] border-t-0 bg-[#0a0a0a] px-4 py-2">
					<p class="text-[#444] font-mono text-xs text-center uppercase tracking-wider">
						Press ENTER to continue_
					</p>
				</div>
			</div>
		</div>
	);
}

export default Auth;
