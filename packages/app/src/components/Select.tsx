import { ChevronDown } from "lucide-solid";
import { For, Show, createEffect, createSignal, onCleanup } from "solid-js";

export interface SelectOption {
	value: string;
	label: string;
}

interface SelectProps {
	value: string;
	options: SelectOption[];
	onChange: (value: string) => void;
	placeholder?: string;
}

export default function Select(props: SelectProps) {
	const [isOpen, setIsOpen] = createSignal(false);
	let containerRef: HTMLDivElement | undefined;

	const selectedLabel = () => {
		const option = props.options.find((o) => o.value === props.value);
		return option?.label ?? props.placeholder ?? "";
	};

	const handleClickOutside = (e: MouseEvent) => {
		if (containerRef && !containerRef.contains(e.target as Node)) {
			setIsOpen(false);
		}
	};

	createEffect(() => {
		if (isOpen()) {
			document.addEventListener("click", handleClickOutside);
		} else {
			document.removeEventListener("click", handleClickOutside);
		}
	});

	onCleanup(() => {
		document.removeEventListener("click", handleClickOutside);
	});

	return (
		<div ref={containerRef} class="relative">
			<button
				type="button"
				onClick={() => setIsOpen(!isOpen())}
				class="flex items-center justify-between w-full px-4 py-2.5 bg-th-surface border border-border-strong text-txt-primary hover:border-ac focus:outline-none focus:border-ac transition-colors font-mono text-sm"
				role="combobox"
				aria-expanded={isOpen()}
				aria-haspopup="listbox"
			>
				<span class="truncate">{selectedLabel()}</span>
				<ChevronDown
					class={`w-4 h-4 ml-2 text-txt-muted transition-transform ${isOpen() ? "rotate-180" : ""}`}
				/>
			</button>

			<Show when={isOpen()}>
				<div
					class="absolute z-50 w-full mt-1 bg-th-surface border border-border-strong max-h-60 overflow-auto"
					role="listbox"
				>
					<For each={props.options}>
						{(option) => (
							<button
								type="button"
								onClick={() => {
									props.onChange(option.value);
									setIsOpen(false);
								}}
								class={`flex items-center justify-between w-full px-4 py-2.5 text-left hover:bg-th-hover transition-colors font-mono text-sm ${
									option.value === props.value ? "text-ac bg-th-hover" : "text-txt-primary"
								}`}
								role="option"
								aria-selected={option.value === props.value}
							>
								<span class="truncate">{option.label}</span>
								<Show when={option.value === props.value}>
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
