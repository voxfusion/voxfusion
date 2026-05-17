import { ChevronDown } from "lucide-solid";
import { For, Show, createEffect, createSignal, onCleanup } from "solid-js";
import { type AppStyle, STYLE_LIST } from "../lib/commands/apps";

interface StyleSelectProps {
	value: AppStyle;
	labelFor: (style: AppStyle) => string;
	onChange: (style: AppStyle) => void;
}

export default function StyleSelect(props: StyleSelectProps) {
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
