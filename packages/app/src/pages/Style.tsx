import { ChevronDown, Search } from "lucide-solid";
import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { useI18n } from "../i18n";
import {
	type AppInstruction,
	type AppStyle,
	type InstalledApp,
	STYLE_LIST,
	deleteAppInstruction,
	listAppInstructions,
	listInstalledApps,
	setAppInstruction,
} from "../lib/commands/apps";
import { capture } from "../lib/posthog";
import { updateDefaultStyle, useSettings } from "../lib/settingsStore";

const SKELETON_ROWS = Array.from({ length: 4 });

export default function Style() {
	const [t] = useI18n();
	const settings = useSettings();
	const [installedApps, setInstalledApps] = createSignal<InstalledApp[]>([]);
	const [instructions, setInstructions] = createSignal<AppInstruction[]>([]);
	const [searchQuery, setSearchQuery] = createSignal("");
	const [searchOpen, setSearchOpen] = createSignal(false);
	const [highlightedIndex, setHighlightedIndex] = createSignal(0);
	const [loading, setLoading] = createSignal(true);
	let searchContainerRef: HTMLDivElement | undefined;
	const optionRefs = new Map<number, HTMLButtonElement>();

	const fetchInstructions = async () => {
		const result = await listAppInstructions();
		setInstructions(result);
	};

	onMount(async () => {
		capture("$pageview", { $current_url: "/style" });
		setLoading(true);
		try {
			const [apps, instr] = await Promise.all([listInstalledApps(), listAppInstructions()]);
			setInstalledApps(apps);
			setInstructions(instr);
		} finally {
			setLoading(false);
		}
	});

	const handleClickOutside = (e: MouseEvent) => {
		if (searchContainerRef && !searchContainerRef.contains(e.target as Node)) {
			setSearchOpen(false);
		}
	};

	createEffect(() => {
		if (searchOpen()) {
			document.addEventListener("click", handleClickOutside);
		} else {
			document.removeEventListener("click", handleClickOutside);
		}
	});

	onCleanup(() => {
		document.removeEventListener("click", handleClickOutside);
	});

	const configuredBundleIds = createMemo(
		() => new Set(instructions().map((i) => i.bundle_id))
	);

	const iconByBundleId = createMemo(() => {
		const map = new Map<string, string>();
		for (const app of installedApps()) {
			if (app.icon_data_url) map.set(app.bundle_id, app.icon_data_url);
		}
		return map;
	});

	const filteredApps = createMemo(() => {
		const query = searchQuery().trim().toLowerCase();
		const configured = configuredBundleIds();
		return installedApps()
			.filter((app) => !configured.has(app.bundle_id))
			.filter((app) => {
				if (!query) return true;
				return (
					app.name.toLowerCase().includes(query) ||
					app.bundle_id.toLowerCase().includes(query)
				);
			})
			.slice(0, 50);
	});

	createEffect(() => {
		filteredApps();
		searchQuery();
		setHighlightedIndex(0);
	});

	createEffect(() => {
		if (!searchOpen()) return;
		const el = optionRefs.get(highlightedIndex());
		el?.scrollIntoView({ block: "nearest" });
	});

	const handleSearchKeyDown = (e: KeyboardEvent) => {
		const items = filteredApps();
		if (e.key === "ArrowDown") {
			e.preventDefault();
			setSearchOpen(true);
			if (items.length === 0) return;
			setHighlightedIndex((i) => Math.min(items.length - 1, i + 1));
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			if (items.length === 0) return;
			setHighlightedIndex((i) => Math.max(0, i - 1));
		} else if (e.key === "Enter") {
			if (!searchOpen() || items.length === 0) return;
			e.preventDefault();
			const idx = Math.min(highlightedIndex(), items.length - 1);
			const target = items[idx];
			if (target) handleAddApp(target);
		} else if (e.key === "Escape") {
			if (!searchOpen()) return;
			e.preventDefault();
			setSearchOpen(false);
		}
	};

	const handleAddApp = async (app: InstalledApp) => {
		await setAppInstruction(app.bundle_id, app.name, "default");
		capture("app_instruction_added", { style: "default" });
		setSearchQuery("");
		setSearchOpen(false);
		await fetchInstructions();
	};

	const handleChangeStyle = async (instruction: AppInstruction, style: AppStyle) => {
		setInstructions(
			instructions().map((i) => (i.id === instruction.id ? { ...i, style } : i))
		);
		await setAppInstruction(instruction.bundle_id, instruction.app_name, style);
		capture("app_instruction_style_changed", { style });
	};

	const handleDelete = async (id: string) => {
		capture("app_instruction_deleted");
		setInstructions(instructions().filter((i) => i.id !== id));
		await deleteAppInstruction(id);
	};

	const handleSelectDefaultStyle = async (style: AppStyle) => {
		await updateDefaultStyle(style);
		capture("default_style_changed", { style });
	};

	const styleLabel = (style: AppStyle) => {
		switch (style) {
			case "professional":
				return t("appInstructions.styles.professional");
			case "casual":
				return t("appInstructions.styles.casual");
			case "agents":
				return t("appInstructions.styles.agents");
			case "default":
				return t("appInstructions.styles.default");
		}
	};

	const styleDescription = (style: AppStyle) => {
		switch (style) {
			case "professional":
				return t("style.descriptions.professional");
			case "casual":
				return t("style.descriptions.casual");
			case "agents":
				return t("style.descriptions.agents");
			case "default":
				return t("style.descriptions.default");
		}
	};

	return (
		<div class="min-h-screen bg-th-base px-6 py-8">
			<div class="max-w-2xl mx-auto">
				<div class="mb-8">
					<h1 class="font-mono text-txt-primary text-sm">
						<span class="text-ac">[STYLE]</span>
						<span class="text-txt-muted"> {">"} </span>
						<span class="text-txt-secondary">TRANSCRIPTION_STYLE</span>
					</h1>
				</div>

				<section class="mb-10">
					<div class="mb-3">
						<h2 class="font-mono text-txt-primary text-xs uppercase tracking-wider">
							{t("style.defaultStyleTitle")}
						</h2>
						<p class="text-txt-muted font-mono text-xs mt-1">
							{t("style.defaultStyleDescription")}
						</p>
					</div>

					<div class="bg-th-surface border border-border">
						<div class="flex border-b border-border">
							<For each={STYLE_LIST}>
								{(style) => {
									const isActive = () => settings().defaultStyle === style;
									return (
										<button
											type="button"
											onClick={() => handleSelectDefaultStyle(style)}
											class={`flex-1 px-4 py-2.5 font-mono text-xs uppercase tracking-wider transition-colors border-b-2 ${
												isActive()
													? "text-ac border-ac bg-th-hover"
													: "text-txt-secondary border-transparent hover:text-txt-primary hover:bg-th-hover"
											}`}
										>
											{styleLabel(style)}
										</button>
									);
								}}
							</For>
						</div>
						<p class="px-4 py-4 text-txt-secondary font-mono text-xs leading-relaxed">
							{styleDescription(settings().defaultStyle)}
						</p>
					</div>
				</section>

				<section>
					<div class="mb-3 flex items-center justify-between">
						<div>
							<h2 class="font-mono text-txt-primary text-xs uppercase tracking-wider">
								{t("style.perAppTitle")}
							</h2>
							<p class="text-txt-muted font-mono text-xs mt-1">
								{t("appInstructions.description")}
							</p>
						</div>
						<Show when={instructions().length > 0}>
							<span class="text-txt-muted font-mono text-xs uppercase">
								{t("appInstructions.appCount").replace(
									"{count}",
									String(instructions().length)
								)}
							</span>
						</Show>
					</div>

					<div ref={searchContainerRef} class="relative mb-6">
						<div class="bg-th-surface border border-border p-4">
							<div class="flex items-center gap-3">
								<Search class="w-4 h-4 text-txt-muted shrink-0" />
								<input
									type="text"
									value={searchQuery()}
									onInput={(e) => {
										setSearchQuery(e.currentTarget.value);
										setSearchOpen(true);
									}}
									onFocus={() => setSearchOpen(true)}
									onKeyDown={handleSearchKeyDown}
									placeholder={t("appInstructions.searchPlaceholder")}
									class="flex-1 bg-transparent text-txt-primary font-mono placeholder-txt-muted focus:outline-none"
								/>
							</div>
						</div>

						<Show when={searchOpen()}>
							<div class="absolute z-50 w-full mt-1 bg-th-surface border border-border-strong max-h-72 overflow-auto">
								<Show
									when={!loading()}
									fallback={
										<For each={SKELETON_ROWS}>{() => <AppRowSkeleton />}</For>
									}
								>
									<Show
										when={filteredApps().length > 0}
										fallback={
											<div class="px-4 py-3 font-mono text-xs text-txt-muted uppercase tracking-wide">
												{installedApps().length === 0
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
													onClick={() => handleAddApp(app)}
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

					<Show when={loading()}>
						<div class="space-y-1">
							<For each={SKELETON_ROWS}>{() => <AppRowSkeleton bordered />}</For>
						</div>
					</Show>

					<Show when={!loading() && instructions().length === 0}>
						<div class="bg-th-surface border border-border p-12 flex flex-col items-center justify-center text-center">
							<div class="font-mono text-txt-muted text-sm mb-4">
								<span class="text-ac">[INFO]</span> NO_APPS_CONFIGURED
							</div>
							<p class="text-txt-secondary font-mono text-xs uppercase tracking-wide">
								{t("appInstructions.emptyState")}
							</p>
							<p class="text-txt-muted font-mono text-xs mt-2 max-w-sm">
								{t("appInstructions.emptyStateDescription")}
							</p>
						</div>
					</Show>

					<Show when={!loading() && instructions().length > 0}>
						<div class="space-y-1">
							<For each={instructions()}>
								{(instruction) => (
									<div class="bg-th-surface border border-border px-4 py-3 flex items-center justify-between gap-3 group hover:border-border-strong transition-colors">
										<AppIcon
											src={iconByBundleId().get(instruction.bundle_id) ?? null}
											alt={instruction.app_name}
										/>
										<div class="flex flex-col min-w-0 flex-1">
											<span class="text-txt-primary font-mono truncate">
												{instruction.app_name}
											</span>
											<span class="text-txt-muted font-mono text-xs truncate">
												{instruction.bundle_id}
											</span>
										</div>
										<StyleSelect
											value={instruction.style}
											labelFor={styleLabel}
											onChange={(style) => handleChangeStyle(instruction, style)}
										/>
										<button
											type="button"
											onClick={() => handleDelete(instruction.id)}
											class="text-txt-muted hover:text-ac opacity-0 group-hover:opacity-100 transition-all font-mono text-xs uppercase tracking-wider"
											title={t("appInstructions.delete")}
										>
											[DEL]
										</button>
									</div>
								)}
							</For>
						</div>
					</Show>
				</section>
			</div>
		</div>
	);
}

interface AppRowSkeletonProps {
	bordered?: boolean;
}

function AppRowSkeleton(props: AppRowSkeletonProps) {
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
				<div class="w-40 h-6 bg-th-input shrink-0" />
			</Show>
		</div>
	);
}

interface AppIconProps {
	src: string | null;
	alt: string;
}

function AppIcon(props: AppIconProps) {
	return (
		<Show
			when={props.src}
			fallback={
				<div class="w-8 h-8 shrink-0 bg-th-input border border-border flex items-center justify-center text-txt-muted font-mono text-xs">
					{props.alt.charAt(0).toUpperCase()}
				</div>
			}
		>
			{(src) => (
				<img src={src()} alt={props.alt} class="w-8 h-8 shrink-0" draggable={false} />
			)}
		</Show>
	);
}

interface StyleSelectProps {
	value: AppStyle;
	labelFor: (style: AppStyle) => string;
	onChange: (style: AppStyle) => void;
}

function StyleSelect(props: StyleSelectProps) {
	const [open, setOpen] = createSignal(false);
	const [highlighted, setHighlighted] = createSignal(0);
	let containerRef: HTMLDivElement | undefined;
	let buttonRef: HTMLButtonElement | undefined;

	const handleClickOutside = (e: MouseEvent) => {
		if (containerRef && !containerRef.contains(e.target as Node)) {
			setOpen(false);
		}
	};

	createEffect(() => {
		if (open()) {
			document.addEventListener("click", handleClickOutside);
			setHighlighted(Math.max(0, STYLE_LIST.indexOf(props.value)));
		} else {
			document.removeEventListener("click", handleClickOutside);
		}
	});

	onCleanup(() => {
		document.removeEventListener("click", handleClickOutside);
	});

	const handleKeyDown = (e: KeyboardEvent) => {
		if (e.key === "ArrowDown") {
			e.preventDefault();
			if (!open()) {
				setOpen(true);
				return;
			}
			setHighlighted((i) => Math.min(STYLE_LIST.length - 1, i + 1));
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			if (!open()) {
				setOpen(true);
				return;
			}
			setHighlighted((i) => Math.max(0, i - 1));
		} else if (e.key === "Enter" || e.key === " ") {
			if (!open()) {
				e.preventDefault();
				setOpen(true);
				return;
			}
			e.preventDefault();
			const choice = STYLE_LIST[highlighted()];
			if (choice) {
				props.onChange(choice);
				setOpen(false);
				buttonRef?.focus();
			}
		} else if (e.key === "Escape") {
			if (!open()) return;
			e.preventDefault();
			setOpen(false);
			buttonRef?.focus();
		}
	};

	return (
		<div ref={containerRef} class="relative w-40 shrink-0">
			<button
				type="button"
				ref={buttonRef}
				onClick={() => setOpen(!open())}
				onKeyDown={handleKeyDown}
				class="flex items-center justify-between w-full px-3 py-1.5 bg-th-input border border-border-strong text-txt-primary hover:border-ac focus:outline-none focus:border-ac transition-colors font-mono text-xs uppercase tracking-wider"
			>
				<span class="truncate">{props.labelFor(props.value)}</span>
				<ChevronDown
					class={`w-3 h-3 ml-2 text-txt-muted transition-transform ${open() ? "rotate-180" : ""}`}
				/>
			</button>
			<Show when={open()}>
				<div class="absolute z-40 right-0 w-full mt-1 bg-th-surface border border-border-strong">
					<For each={STYLE_LIST}>
						{(style, index) => (
							<button
								type="button"
								onClick={() => {
									props.onChange(style);
									setOpen(false);
									buttonRef?.focus();
								}}
								onMouseEnter={() => setHighlighted(index())}
								class={`flex items-center justify-between w-full px-3 py-1.5 text-left transition-colors font-mono text-xs uppercase tracking-wider ${
									highlighted() === index() ? "bg-th-hover" : ""
								} ${style === props.value ? "text-ac" : "text-txt-primary"}`}
							>
								<span class="truncate">{props.labelFor(style)}</span>
								<Show when={style === props.value}>
									<span class="text-ac">[*]</span>
								</Show>
							</button>
						)}
					</For>
				</div>
			</Show>
		</div>
	);
}
