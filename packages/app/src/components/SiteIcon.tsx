import { Globe } from "lucide-solid";
import { Show, createEffect, createMemo, createSignal } from "solid-js";
import { getFaviconUrl } from "../lib/favicons";

interface SiteIconProps {
	domain: string | null;
	sizeClass: string;
}

export default function SiteIcon(props: SiteIconProps) {
	const [failed, setFailed] = createSignal(false);
	const activeDomain = createMemo(() => (failed() ? null : props.domain));

	createEffect(() => {
		props.domain;
		setFailed(false);
	});

	return (
		<div
			class={`${props.sizeClass} shrink-0 bg-th-input border border-border flex items-center justify-center overflow-hidden`}
		>
			<Show when={activeDomain()} fallback={<Globe class="w-3.5 h-3.5 text-txt-muted" />}>
				{(domain) => (
					<img
						src={getFaviconUrl(domain())}
						alt={domain()}
						class="w-full h-full"
						draggable={false}
						onError={() => setFailed(true)}
					/>
				)}
			</Show>
		</div>
	);
}
