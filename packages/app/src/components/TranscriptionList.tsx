import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { Result } from "better-result";
import { Loader } from "lucide-solid";
import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { useI18n } from "../i18n";
import { type Transcription, listTranscriptions } from "../lib/commands/transcriptions";
import { capture } from "../lib/posthog";
import TranscriptionCard from "./TranscriptionCard";

type GroupedTranscriptions = {
	label: string;
	transcriptions: Transcription[];
};

export default function TranscriptionList() {
	const [t, { locale }] = useI18n();
	const [transcriptions, setTranscriptions] = createSignal<Transcription[]>([]);
	const [loading, setLoading] = createSignal(false);
	const [initialLoading, setInitialLoading] = createSignal(true);
	const [nextCursor, setNextCursor] = createSignal<string | null>(null);
	const [hasMore, setHasMore] = createSignal(true);
	const [error, setError] = createSignal<string | null>(null);

	const [sentinelRef, setSentinelRef] = createSignal<HTMLDivElement | null>(null);
	let observer: IntersectionObserver | null = null;
	let unlisten: UnlistenFn | null = null;

	const getDateLabel = (dateStr: string): string => {
		const date = new Date(dateStr);
		const today = new Date();
		const yesterday = new Date(today);
		yesterday.setDate(yesterday.getDate() - 1);

		const isToday =
			date.getDate() === today.getDate() &&
			date.getMonth() === today.getMonth() &&
			date.getFullYear() === today.getFullYear();

		const isYesterday =
			date.getDate() === yesterday.getDate() &&
			date.getMonth() === yesterday.getMonth() &&
			date.getFullYear() === yesterday.getFullYear();

		if (isToday) {
			return t("transcriptionList.today");
		}
		if (isYesterday) {
			return t("transcriptionList.yesterday");
		}

		return date.toLocaleDateString(locale(), {
			weekday: "long",
			month: "long",
			day: "numeric",
		});
	};

	const groupedTranscriptions = createMemo((): GroupedTranscriptions[] => {
		const groups: Map<string, Transcription[]> = new Map();

		for (const transcription of transcriptions()) {
			const label = getDateLabel(transcription.created_at);
			const existing = groups.get(label) ?? [];
			groups.set(label, [...existing, transcription]);
		}

		return Array.from(groups.entries()).map(([label, items]) => ({
			label,
			transcriptions: items,
		}));
	});

	const fetchTranscriptions = async (cursor?: string) => {
		if (loading()) return;

		setLoading(true);
		setError(null);

		const result = await listTranscriptions(20, cursor || null);
		if (Result.isError(result)) {
			setError(result.error.message || t("transcriptionList.errorOccurred"));
			setLoading(false);
			setInitialLoading(false);
			return;
		}

		const items = result.value.transcriptions ?? [];
		const lastItem = items[items.length - 1];

		setTranscriptions((prev) => (cursor ? [...prev, ...items] : items));
		setNextCursor(lastItem?.created_at ?? null);
		setHasMore(result.value.has_more ?? false);
		setLoading(false);
		setInitialLoading(false);
	};

	onMount(async () => {
		fetchTranscriptions();

		unlisten = await listen("transcription-created", () => {
			capture("transcription_created");
			fetchTranscriptions();
		});
	});

	createEffect(() => {
		const sentinel = sentinelRef();

		if (observer) {
			observer.disconnect();
			observer = null;
		}

		if (!sentinel) return;

		observer = new IntersectionObserver(
			(entries) => {
				const entry = entries[0];
				if (entry?.isIntersecting && hasMore() && !loading() && nextCursor()) {
					fetchTranscriptions(nextCursor()!);
				}
			},
			{ rootMargin: "100px" }
		);

		observer.observe(sentinel);
	});

	onCleanup(() => {
		observer?.disconnect();
		unlisten?.();
	});

	return (
		<div class="space-y-6">
			<Show when={initialLoading()}>
				<div class="flex items-center justify-center py-12">
					<Loader class="w-6 h-6 animate-spin text-ac" />
				</div>
			</Show>

			<Show when={error()}>
				<div class="text-center py-8 font-mono">
					<p class="text-ac mb-2">[ERROR] {error()}</p>
					<button
						type="button"
						onClick={() => fetchTranscriptions()}
						class="text-ac hover:underline"
					>
						[RETRY]
					</button>
				</div>
			</Show>

			<Show when={!initialLoading() && !error() && transcriptions().length === 0}>
				<div class="text-center py-12 font-mono">
					<p class="text-txt-secondary mb-2">[INFO] NO_TRANSCRIPTIONS</p>
					<p class="text-txt-muted text-sm">{t("transcriptionList.useCommandToRecord")}</p>
				</div>
			</Show>

			<Show when={!initialLoading() && transcriptions().length > 0}>
				<For each={groupedTranscriptions()}>
					{(group) => (
						<div>
							<h3 class="text-ac font-mono text-xs uppercase tracking-wider mb-2 px-1">
								{group.label}
							</h3>
							<div class="bg-th-surface border border-border divide-y divide-border">
								<For each={group.transcriptions}>
									{(transcription) => <TranscriptionCard transcription={transcription} />}
								</For>
							</div>
						</div>
					)}
				</For>

				<div ref={setSentinelRef} class="h-4" />

				<Show when={loading() && transcriptions().length > 0}>
					<div class="flex items-center justify-center py-4">
						<Loader class="w-5 h-5 animate-spin text-ac" />
					</div>
				</Show>

				<Show when={!hasMore() && transcriptions().length > 0}>
					<p class="text-center text-txt-muted font-mono text-xs py-4">
						{t("transcriptionList.noMore")}
					</p>
				</Show>
			</Show>
		</div>
	);
}
