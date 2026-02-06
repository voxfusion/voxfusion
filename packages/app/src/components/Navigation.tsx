import { A, useLocation } from "@solidjs/router";
import { emit, listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { BookOpen, Home, LogOut, Settings, Shield, User } from "lucide-solid";
import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { useI18n } from "../i18n";
import { authClient } from "../lib/authClient";
import eden from "../lib/eden";
import { tokenManager } from "../lib/tokenManager";

interface SidebarProps {
	onOpenSettings: () => void;
}

export default function Sidebar(props: SidebarProps) {
	const [t] = useI18n();
	const location = useLocation();
	const [isUserMenuOpen, setIsUserMenuOpen] = createSignal(false);
	const [wordsUsed, setWordsUsed] = createSignal(0);
	const [wordLimit, setWordLimit] = createSignal(10_000);

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
		await authClient.signOut();
		await tokenManager.deleteToken();
		await emit("auth-changed");
		setIsUserMenuOpen(false);
	};

	return (
		<aside class="w-56 bg-[#0a0a0a] border-r border-[#222] flex flex-col h-full">
			<nav class="flex-1 p-3 pt-9 space-y-1">
				<A
					href="/"
					class={`flex items-center gap-3 px-3 py-2 font-mono uppercase tracking-wider text-xs transition-colors ${
						isActive("/")
							? "text-[#ff3e00] border-l-2 border-[#ff3e00] bg-[#111]"
							: "text-[#888] hover:text-[#e0e0e0] hover:bg-[#111] border-l-2 border-transparent"
					}`}
				>
					<Home class="w-4 h-4" />
					<span>01 {t("sidebar.home")}</span>
				</A>
				<A
					href="/dictionary"
					class={`flex items-center gap-3 px-3 py-2 font-mono uppercase tracking-wider text-xs transition-colors ${
						isActive("/dictionary")
							? "text-[#ff3e00] border-l-2 border-[#ff3e00] bg-[#111]"
							: "text-[#888] hover:text-[#e0e0e0] hover:bg-[#111] border-l-2 border-transparent"
					}`}
				>
					<BookOpen class="w-4 h-4" />
					<span>02 {t("sidebar.dictionary")}</span>
				</A>
			</nav>

			<div class="px-3 pb-3">
				<div class="px-3 py-2">
					<div class="flex items-center justify-between mb-1.5">
						<span class="font-mono uppercase tracking-wider text-[10px] text-[#666]">
							{t("sidebar.wordsUsed")}
						</span>
					</div>
					<div class="font-mono text-xs tabular-nums mb-1.5">
						<Show
							when={!isLimitReached()}
							fallback={<span class="text-[#ff3e00]">{t("sidebar.limitReached")}</span>}
						>
							<span class="text-[#e0e0e0]">{wordsUsed().toLocaleString()}</span>
							<span class="text-[#666]"> / {wordLimit().toLocaleString()}</span>
						</Show>
					</div>
					<div class="w-full h-1 bg-[#222] overflow-hidden">
						<div
							class="h-full transition-all duration-300"
							style={{
								width: `${usagePercent()}%`,
								"background-color": isLimitReached() ? "#ff3e00" : "#ff3e00",
								opacity: isLimitReached() ? 1 : 0.7,
							}}
						/>
					</div>
				</div>
			</div>

			<div class="p-3 border-t border-[#222] relative">
				<button
					type="button"
					onClick={() => setIsUserMenuOpen(!isUserMenuOpen())}
					class="flex items-center gap-3 w-full px-3 py-2 font-mono uppercase tracking-wider text-xs text-[#888] hover:text-[#e0e0e0] hover:bg-[#111] transition-colors"
				>
					<div class="w-8 h-8 bg-[#222] border border-[#333] flex items-center justify-center">
						<User class="w-4 h-4 text-[#888]" />
					</div>
					<span>{t("sidebar.account")}</span>
				</button>

				<Show when={isUserMenuOpen()}>
					<div class="absolute bottom-full left-3 right-3 mb-2 bg-[#111] border border-[#222] py-1 z-50">
						<button
							type="button"
							onClick={() => {
								setIsUserMenuOpen(false);
								props.onOpenSettings();
							}}
							class="flex items-center gap-3 w-full px-4 py-2 font-mono uppercase tracking-wider text-xs text-[#888] hover:text-[#e0e0e0] hover:bg-[#0a0a0a] transition-colors"
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
							class="flex items-center gap-3 w-full px-4 py-2 font-mono uppercase tracking-wider text-xs text-[#888] hover:text-[#e0e0e0] hover:bg-[#0a0a0a] transition-colors"
						>
							<Shield class="w-4 h-4" />
							{t("sidebar.privacy")}
						</button>
						<div class="border-t border-[#222] my-1" />
						<button
							type="button"
							onClick={handleLogout}
							class="flex items-center gap-3 w-full px-4 py-2 font-mono uppercase tracking-wider text-xs text-[#ff3e00] hover:bg-[#0a0a0a] transition-colors"
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
