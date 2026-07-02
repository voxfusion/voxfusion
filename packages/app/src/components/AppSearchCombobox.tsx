import { Search } from "lucide-solid";
import { For, Show, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { useI18n } from "../i18n";
import type { InstalledApp } from "../lib/commands/apps";
import AppIcon from "./AppIcon";

export const SKELETON_ROWS = Array.from({ length: 4 });

interface AppSearchComboboxProps {
	apps: InstalledApp[];
	excludedIds: Set<string>;
	loading: boolean;
	searchQuery: string;
	searchOpen: boolean;
	onSearchQueryChange: (value: string) => void;
	onSearchOpenChange: (open: boolean) => void;
	onSelect: (app: InstalledApp) => void;
}

export default function AppSearchCombobox(props: AppSearchComboboxProps) {
	const [t] = useI18n();
	const [highlightedIndex, setHighlightedIndex] = createSignal(0);
	let searchContainerRef: HTMLDivElement | undefined;
	const optionRefs = new Map<number, HTMLButtonElement>();

	const filteredApps = createMemo(() => {
		const query = props.searchQuery.trim().toLowerCase();
		const excluded = props.excludedIds;
		return props.apps
			.filter((app) => !excluded.has(app.bundle_id))
			.filter((app) => {
				if (!query) return true;
				return (
					app.name.toLowerCase().includes(query) || app.bundle_id.toLowerCase().includes(query)
				);
			})
			.slice(0, 50);
	});

	createEffect(() => {
		filteredApps();
		setHighlightedIndex(0);
	});

	createEffect(() => {
		if (!props.searchOpen) return;
		const el = optionRefs.get(highlightedIndex());
		el?.scrollIntoView({ block: "nearest" });
	});

	const handleClickOutside = (e: MouseEvent) => {
		if (searchContainerRef && !searchContainerRef.contains(e.target as Node)) {
			props.onSearchOpenChange(false);
		}
	};

	createEffect(() => {
		if (props.searchOpen) {
			document.addEventListener("click", handleClickOutside);
		} else {
			document.removeEventListener("click", handleClickOutside);
		}
	});

	onCleanup(() => {
		document.removeEventListener("click", handleClickOutside);
	});

	const handleSearchKeyDown = (e: KeyboardEvent) => {
		const items = filteredApps();
		if (e.key === "ArrowDown") {
			e.preventDefault();
			props.onSearchOpenChange(true);
			if (items.length === 0) return;
			setHighlightedIndex((i) => Math.min(items.length - 1, i + 1));
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			if (items.length === 0) return;
			setHighlightedIndex((i) => Math.max(0, i - 1));
		} else if (e.key === "Enter") {
			if (!props.searchOpen || items.length === 0) return;
			e.preventDefault();
			const idx = Math.min(highlightedIndex(), items.length - 1);
			const target = items[idx];
			if (target) props.onSelect(target);
		} else if (e.key === "Escape") {
			if (!props.searchOpen) return;
			e.preventDefault();
			props.onSearchOpenChange(false);
		}
	};

	return (
		<div ref={searchContainerRef} class="relative mb-6">
			<div class="bg-th-surface border border-border p-4">
				<div class="flex items-center gap-3">
					<Search class="w-4 h-4 text-txt-muted shrink-0" />
					<input
						type="text"
						value={props.searchQuery}
						onInput={(e) => {
							props.onSearchQueryChange(e.currentTarget.value);
							props.onSearchOpenChange(true);
						}}
						onFocus={() => props.onSearchOpenChange(true)}
						onKeyDown={handleSearchKeyDown}
						placeholder={t("appInstructions.searchPlaceholder")}
						class="flex-1 bg-transparent text-txt-primary font-mono placeholder-txt-muted focus:outline-none"
					/>
				</div>
			</div>

			<Show when={props.searchOpen}>
				<div class="absolute z-50 w-full mt-1 bg-th-surface border border-border-strong max-h-72 overflow-auto">
					<Show
						when={!props.loading}
						fallback={<For each={SKELETON_ROWS}>{() => <AppRowSkeleton />}</For>}
					>
						<Show
							when={filteredApps().length > 0}
							fallback={
								<div class="px-4 py-3 font-mono text-xs text-txt-muted uppercase tracking-wide">
									{props.apps.length === 0
										? t("appInstructions.noAppsDetected")
										: t("appInstructions.noMatches")}
								</div>
							}
						>
							<For each={filteredApps()}>
								{(app, index) => (
									<button
										type="button"
										ref={(el) => optionRefs.set(index(), el)}
										onClick={() => props.onSelect(app)}
										onMouseEnter={() => setHighlightedIndex(index())}
										class={`w-full px-4 py-2.5 text-left transition-colors font-mono text-sm flex items-center gap-3 ${
											highlightedIndex() === index() ? "bg-th-hover" : ""
										}`}
									>
										<AppIcon src={app.icon_data_url} alt={app.name} />
										<div class="flex flex-col gap-0.5 min-w-0 flex-1">
											<span class="text-txt-primary truncate">{app.name}</span>
											<span class="text-txt-muted text-xs truncate">{app.bundle_id}</span>
										</div>
									</button>
								)}
							</For>
						</Show>
					</Show>
				</div>
			</Show>
		</div>
	);
}

interface AppRowSkeletonProps {
	bordered?: boolean;
	trailingClass?: string;
}

export function AppRowSkeleton(props: AppRowSkeletonProps) {
	return (
		<div
			class={`px-4 py-2.5 flex items-center gap-3 animate-pulse ${
				props.bordered ? "bg-th-surface border border-border" : ""
			}`}
		>
			<div class="w-8 h-8 shrink-0 bg-th-input" />
			<div class="flex flex-col gap-1.5 min-w-0 flex-1">
				<div class="h-3 w-32 bg-th-input" />
				<div class="h-2.5 w-48 bg-th-input opacity-60" />
			</div>
			<Show when={props.bordered}>
				<div class={`${props.trailingClass ?? ""} bg-th-input shrink-0`} />
			</Show>
		</div>
	);
}
