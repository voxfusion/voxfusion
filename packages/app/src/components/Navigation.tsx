import { createSignal, Show } from "solid-js";
import { A, useLocation } from "@solidjs/router";
import { Home, BookOpen, User, Settings, Shield, LogOut } from "lucide-solid";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useI18n } from "../i18n";
import { authClient } from "../lib/authClient";
import { tokenManager } from "../lib/tokenManager";

export default function Sidebar() {
	const [t] = useI18n();
	const location = useLocation();
	const [isUserMenuOpen, setIsUserMenuOpen] = createSignal(false);

	const isActive = (path: string) => location.pathname === path;

	const handleLogout = async () => {
		await authClient.signOut();
		await tokenManager.deleteToken();
		setIsUserMenuOpen(false);
	};

	return (
		<aside class="w-56 bg-white border-r border-slate-200 flex flex-col h-full">
			<nav class="flex-1 p-3 pt-9 space-y-1">
				<A
					href="/"
					class={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
						isActive("/")
							? "bg-primary-50 text-primary-700"
							: "text-slate-600 hover:bg-slate-50 hover:text-slate-900"
					}`}
				>
					<Home class="w-5 h-5" />
					{t("sidebar.home")}
				</A>
				<A
					href="/dictionary"
					class={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
						isActive("/dictionary")
							? "bg-primary-50 text-primary-700"
							: "text-slate-600 hover:bg-slate-50 hover:text-slate-900"
					}`}
				>
					<BookOpen class="w-5 h-5" />
					{t("sidebar.dictionary")}
				</A>
			</nav>

			<div class="p-3 border-t border-slate-200 relative">
				<button
					type="button"
					onClick={() => setIsUserMenuOpen(!isUserMenuOpen())}
					class="flex items-center gap-3 w-full px-3 py-2 rounded-lg text-sm font-medium text-slate-600 hover:bg-slate-50 hover:text-slate-900 transition-colors"
				>
					<div class="w-8 h-8 bg-slate-200 rounded-full flex items-center justify-center">
						<User class="w-4 h-4 text-slate-600" />
					</div>
					<span>{t("sidebar.account")}</span>
				</button>

				<Show when={isUserMenuOpen()}>
					<div class="absolute bottom-full left-3 right-3 mb-2 bg-white rounded-lg shadow-lg border border-slate-200 py-1 z-50">
						<A
							href="/settings"
							onClick={() => setIsUserMenuOpen(false)}
							class="flex items-center gap-3 px-4 py-2 text-sm text-slate-600 hover:bg-slate-50 hover:text-slate-900 transition-colors"
						>
							<Settings class="w-5 h-5" />
							{t("sidebar.settings")}
						</A>
						<button
							type="button"
							onClick={() => {
								openUrl("https://voxfusion.io/privacy");
								setIsUserMenuOpen(false);
							}}
							class="flex items-center gap-3 w-full px-4 py-2 text-sm text-slate-600 hover:bg-slate-50 hover:text-slate-900 transition-colors"
						>
							<Shield class="w-5 h-5" />
							{t("sidebar.privacy")}
						</button>
						<div class="border-t border-slate-100 my-1" />
						<button
							type="button"
							onClick={handleLogout}
							class="flex items-center gap-3 w-full px-4 py-2 text-sm text-red-600 hover:bg-red-50 transition-colors"
						>
							<LogOut class="w-5 h-5" />
							{t("sidebar.logout")}
						</button>
					</div>
				</Show>
			</div>
		</aside>
	);
}
