import { A, useLocation } from "@solidjs/router";
import { emit, listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { BookOpen, Home, LogOut, Settings, Shield, User } from "lucide-solid";
import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { useI18n } from "../i18n";
import { authClient } from "../lib/authClient";
import eden from "../lib/eden";
import { capture, resetUser } from "../lib/posthog";
import { tokenManager } from "../lib/tokenManager";

interface SidebarProps {
	onOpenSettings: () => void;
}

export default function Sidebar(props: SidebarProps) {
	const [t] = useI18n();
	const location = useLocation();
	const [isUserMenuOpen, setIsUserMenuOpen] = createSignal(false);
	const [wordsUsed, setWordsUsed] = createSignal(0);
	const [wordLimit, setWordLimit] = createSignal(1_000);

	const isActive = (path: string) => location.pathname === path;

	const fetchUsage = async () => {
		try {
			const res = await eden.api.transcribe.usage.get();
			const data = res.data as { wordsUsed: number; wordLimit: number } | null;
			if (data) {
				setWordsUsed(data.wordsUsed);
				setWordLimit(data.wordLimit);
			}
		} catch {}
	};

	onMount(async () => {
		await fetchUsage();
		const unlisten = await listen("transcription-created", fetchUsage);
		onCleanup(unlisten);
	});

	const usagePercent = () => Math.min(100, (wordsUsed() / wordLimit()) * 100);
	const isLimitReached = () => wordsUsed() >= wordLimit();

	const handleLogout = async () => {
		capture("logout");
		await authClient.signOut();
		await tokenManager.deleteToken();
		resetUser();
		await emit("auth-changed");
		setIsUserMenuOpen(false);
	};

	return (
		<aside class="w-56 bg-th-base border-r border-border flex flex-col h-full">
			<nav class="flex-1 p-3 pt-9 space-y-1">
				<A
					href="/"
					class={`flex items-center gap-3 px-3 py-2 font-mono uppercase tracking-wider text-xs transition-colors ${
						isActive("/")
							? "text-ac border-l-2 border-ac bg-th-surface"
							: "text-txt-secondary hover:text-txt-primary hover:bg-th-surface border-l-2 border-transparent"
					}`}
				>
					<Home class="w-4 h-4" />
					<span>01 {t("sidebar.home")}</span>
				</A>
				<A
					href="/dictionary"
					class={`flex items-center gap-3 px-3 py-2 font-mono uppercase tracking-wider text-xs transition-colors ${
						isActive("/dictionary")
							? "text-ac border-l-2 border-ac bg-th-surface"
							: "text-txt-secondary hover:text-txt-primary hover:bg-th-surface border-l-2 border-transparent"
					}`}
				>
					<BookOpen class="w-4 h-4" />
					<span>02 {t("sidebar.dictionary")}</span>
				</A>
			</nav>

			<div class="px-3 pb-3">
				<div class="px-3 py-2">
					<div class="flex items-center justify-between mb-1.5">
						<span class="font-mono uppercase tracking-wider text-[10px] text-txt-muted">
							{t("sidebar.wordsUsed")}
						</span>
					</div>
					<div class="font-mono text-xs tabular-nums mb-1.5">
						<Show
							when={!isLimitReached()}
							fallback={<span class="text-ac">{t("sidebar.limitReached")}</span>}
						>
							<span class="text-txt-primary">{wordsUsed().toLocaleString()}</span>
							<span class="text-txt-muted"> / {wordLimit().toLocaleString()}</span>
						</Show>
					</div>
					<div class="w-full h-1 bg-border overflow-hidden">
						<div
							class="h-full transition-all duration-300"
							style={{
								width: `${usagePercent()}%`,
								"background-color": "var(--color-accent)",
								opacity: isLimitReached() ? 1 : 0.7,
							}}
						/>
					</div>
				</div>
			</div>

			<div class="p-3 border-t border-border relative">
				<button
					type="button"
					onClick={() => setIsUserMenuOpen(!isUserMenuOpen())}
					class="flex items-center gap-3 w-full px-3 py-2 font-mono uppercase tracking-wider text-xs text-txt-secondary hover:text-txt-primary hover:bg-th-surface transition-colors"
				>
					<div class="w-8 h-8 bg-border border border-border-strong flex items-center justify-center">
						<User class="w-4 h-4 text-txt-secondary" />
					</div>
					<span>{t("sidebar.account")}</span>
				</button>

				<Show when={isUserMenuOpen()}>
					<div class="absolute bottom-full left-3 right-3 mb-2 bg-th-surface border border-border py-1 z-50">
						<button
							type="button"
							onClick={() => {
								setIsUserMenuOpen(false);
								props.onOpenSettings();
							}}
							class="flex items-center gap-3 w-full px-4 py-2 font-mono uppercase tracking-wider text-xs text-txt-secondary hover:text-txt-primary hover:bg-th-base transition-colors"
						>
							<Settings class="w-4 h-4" />
							{t("sidebar.settings")}
						</button>
						<button
							type="button"
							onClick={() => {
								openUrl("https://voxfusion.io/privacy");
								setIsUserMenuOpen(false);
							}}
							class="flex items-center gap-3 w-full px-4 py-2 font-mono uppercase tracking-wider text-xs text-txt-secondary hover:text-txt-primary hover:bg-th-base transition-colors"
						>
							<Shield class="w-4 h-4" />
							{t("sidebar.privacy")}
						</button>
						<div class="border-t border-border my-1" />
						<button
							type="button"
							onClick={handleLogout}
							class="flex items-center gap-3 w-full px-4 py-2 font-mono uppercase tracking-wider text-xs text-ac hover:bg-th-base transition-colors"
						>
							<LogOut class="w-4 h-4" />
							{t("sidebar.logout")}
						</button>
					</div>
				</Show>
			</div>
		</aside>
	);
}
