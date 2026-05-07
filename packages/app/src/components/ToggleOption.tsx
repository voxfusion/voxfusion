interface ToggleOptionProps {
	label: string;
	description: string;
	isEnabled: boolean;
	onChange: (enabled: boolean) => void;
}

export default function ToggleOption(props: ToggleOptionProps) {
	return (
		<div class="flex items-center justify-between gap-4">
			<div class="flex-1">
				<div class="font-mono text-txt-muted text-xs uppercase tracking-wider mb-1">
					{props.label}
				</div>
				<p class="font-mono text-xs text-txt-faint">{props.description}</p>
			</div>
			<button
				type="button"
				role="switch"
				aria-checked={props.isEnabled}
				onClick={() => props.onChange(!props.isEnabled)}
				class={`relative inline-flex items-center h-6 w-11 border transition-colors ${
					props.isEnabled ? "bg-ac border-ac" : "bg-th-surface border-border-strong"
				}`}
			>
				<span
					class={`inline-block w-4 h-4 transition-transform ${
						props.isEnabled ? "translate-x-6 bg-ac-on" : "translate-x-1 bg-txt-muted"
					}`}
				/>
			</button>
		</div>
	);
}
