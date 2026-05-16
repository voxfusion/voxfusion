import { A, useLocation } from "@solidjs/router";
import { BookOpen, Home, Settings, Wand2 } from "lucide-solid";
import { useI18n } from "../i18n";
import UpdateNotification from "./UpdateNotification";

interface SidebarProps {
	onOpenSettings: () => void;
}

export default function Sidebar(props: SidebarProps) {
	const [t] = useI18n();
	const location = useLocation();

	const isActive = (path: string) => location.pathname === path;

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
				<A
					href="/style"
					class={`flex items-center gap-3 px-3 py-2 font-mono uppercase tracking-wider text-xs transition-colors ${
						isActive("/style")
							? "text-ac border-l-2 border-ac bg-th-surface"
							: "text-txt-secondary hover:text-txt-primary hover:bg-th-surface border-l-2 border-transparent"
					}`}
				>
					<Wand2 class="w-4 h-4" />
					<span>03 {t("sidebar.style")}</span>
				</A>
			</nav>

			<UpdateNotification />

			<div class="p-3 border-t border-border">
				<button
					type="button"
					onClick={() => props.onOpenSettings()}
					class="flex items-center gap-3 w-full px-3 py-2 font-mono uppercase tracking-wider text-xs text-txt-secondary hover:text-txt-primary hover:bg-th-surface transition-colors"
				>
					<Settings class="w-4 h-4 text-txt-secondary" />
					<span>{t("sidebar.settings")}</span>
				</button>
			</div>
		</aside>
	);
}
