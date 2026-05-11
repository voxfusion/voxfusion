import { Show } from "solid-js";
import type { Theme } from "../../lib/settingsStore";

interface ThemeOptionProps {
	value: Theme;
	label: string;
	isSelected: boolean;
	onClick: () => void;
}

export default function ThemeOption(props: ThemeOptionProps) {
	return (
		<button
			type="button"
			onClick={props.onClick}
			class={`relative p-4 border transition-all ${
				props.isSelected
					? "border-ac bg-ac-bg"
					: "border-border-strong bg-th-surface hover:border-txt-faint"
			}`}
		>
			<div class="mb-3">
				<Show when={props.value === "light"}>
					<div class="w-full h-12 bg-[#e0e0e0] border border-[#ccc] flex items-center justify-center">
						<div class="w-8 h-1.5 bg-[#999]" />
					</div>
				</Show>
				<Show when={props.value === "dark"}>
					<div class="w-full h-12 bg-[#1a1a1a] border border-[#333] flex items-center justify-center">
						<div class="w-8 h-1.5 bg-[#444]" />
					</div>
				</Show>
				<Show when={props.value === "system"}>
					<div class="w-full h-12 overflow-hidden flex">
						<div class="w-1/2 bg-[#e0e0e0] border-l border-t border-b border-[#ccc] flex items-center justify-center">
							<div class="w-4 h-1.5 bg-[#999]" />
						</div>
						<div class="w-1/2 bg-[#1a1a1a] border-r border-t border-b border-[#333] flex items-center justify-center">
							<div class="w-4 h-1.5 bg-[#444]" />
						</div>
					</div>
				</Show>
			</div>
			<span
				class={`font-mono text-xs uppercase tracking-wider ${props.isSelected ? "text-ac" : "text-txt-secondary"}`}
			>
				{props.label}
			</span>
			<Show when={props.isSelected}>
				<div class="absolute top-2 right-2 font-mono text-ac text-xs">[*]</div>
			</Show>
		</button>
	);
}
