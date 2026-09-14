import { Globe } from "lucide-solid";
import { Show, createMemo } from "solid-js";

interface SiteIconProps {
	domain: string | null;
	sizeClass: string;
}

export default function SiteIcon(props: SiteIconProps) {
	// Render text locally. Fetching even a favicon discloses the configured domain.
	const initial = createMemo(() => Array.from(props.domain?.trim() ?? "")[0]?.toUpperCase());

	return (
		<div
			aria-hidden="true"
			class={`${props.sizeClass} shrink-0 bg-th-input border border-border flex items-center justify-center overflow-hidden`}
		>
			<Show when={initial()} fallback={<Globe class="w-3.5 h-3.5 text-txt-muted" />}>
				<span class="text-xs font-medium text-txt-primary">{initial()}</span>
			</Show>
		</div>
	);
}
