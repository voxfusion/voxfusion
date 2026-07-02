import { Show } from "solid-js";

interface AppIconProps {
	src: string | null;
	alt: string;
}

export default function AppIcon(props: AppIconProps) {
	return (
		<Show
			when={props.src}
			fallback={
				<div class="w-8 h-8 shrink-0 bg-th-input border border-border flex items-center justify-center text-txt-muted font-mono text-xs">
					{props.alt.charAt(0).toUpperCase()}
				</div>
			}
		>
			{(src) => <img src={src()} alt={props.alt} class="w-8 h-8 shrink-0" draggable={false} />}
		</Show>
	);
}
