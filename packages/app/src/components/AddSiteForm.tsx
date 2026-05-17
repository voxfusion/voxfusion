import { Show, createEffect, createSignal, onCleanup } from "solid-js";
import { useI18n } from "../i18n";
import { preloadFavicons } from "../lib/favicons";
import SiteIcon from "./SiteIcon";

const FAVICON_DEBOUNCE_MS = 400;

export function normalizeDomain(input: string): string | null {
	const trimmed = input.trim();
	if (!trimmed) return null;
	const withoutScheme = trimmed.includes("://") ? trimmed.split("://")[1] ?? "" : trimmed;
	const hostWithPort = withoutScheme
		.split("/")[0]
		?.split("?")[0]
		?.split("#")[0]
		?.split("@")
		.pop();
	if (!hostWithPort) return null;
	const host = hostWithPort.split(":")[0]?.trim() ?? "";
	if (!host) return null;
	const lowered = host.toLowerCase().replace(/^\.+|\.+$/g, "");
	if (!lowered) return null;
	const stripped = lowered.startsWith("www.") ? lowered.slice(4) : lowered;
	return stripped || null;
}

interface AddSiteFormProps {
	onAdd: (domain: string) => void | Promise<void>;
}

export default function AddSiteForm(props: AddSiteFormProps) {
	const [t] = useI18n();
	const [input, setInput] = createSignal("");
	const [error, setError] = createSignal<string | null>(null);
	const [debouncedDomain, setDebouncedDomain] = createSignal<string | null>(null);

	createEffect(() => {
		const value = input();
		const normalized = normalizeDomain(value);
		if (!normalized) {
			setDebouncedDomain(null);
			return;
		}
		const timer = setTimeout(() => {
			setDebouncedDomain(normalized);
			preloadFavicons([normalized]);
		}, FAVICON_DEBOUNCE_MS);
		onCleanup(() => clearTimeout(timer));
	});

	const handleAdd = async () => {
		const normalized = normalizeDomain(input());
		if (!normalized) {
			setError(t("dictionary.sitesInvalidDomain"));
			return;
		}
		setError(null);
		setInput("");
		await props.onAdd(normalized);
	};

	const handleKeyDown = (e: KeyboardEvent) => {
		if (e.key === "Enter") handleAdd();
	};

	return (
		<div class="mb-6 bg-th-surface border border-border p-4 space-y-3">
			<div class="flex items-center gap-3">
				<SiteIcon domain={debouncedDomain()} sizeClass="w-5 h-5" />
				<input
					type="text"
					value={input()}
					onInput={(e) => {
						setInput(e.currentTarget.value);
						if (error()) setError(null);
					}}
					onKeyDown={handleKeyDown}
					placeholder={t("dictionary.sitesDomainPlaceholder")}
					class="flex-1 bg-transparent text-txt-primary font-mono placeholder-txt-muted focus:outline-none"
				/>
				<button
					type="button"
					onClick={handleAdd}
					disabled={!input().trim()}
					class="flex items-center gap-1 px-3 py-1.5 bg-ac text-ac-on font-mono uppercase tracking-wider text-xs hover:bg-ac-hover disabled:opacity-50 transition-colors"
				>
					<span>+</span>
					{t("dictionary.sitesAddSite")}
				</button>
			</div>
			<Show when={error()}>
				<div class="flex justify-end">
					<span class="text-ac font-mono text-xs">{error()}</span>
				</div>
			</Show>
		</div>
	);
}
