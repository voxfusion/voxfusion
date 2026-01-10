import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { Loader } from "lucide-solid";
import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { useI18n } from "../i18n";
import eden from "../lib/eden";
import TranscriptionCard from "./TranscriptionCard";

type Transcription = {
	id: string;
	text: string;
	fileUrl: string;
	processingTimeMs: number;
	audioDurationMs: number | null;
	rating: string | null;
	createdAt: Date;
	userId: string;
	provider: string;
	model: string;
};

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

	const getDateLabel = (date: Date): string => {
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
			const label = getDateLabel(transcription.createdAt);
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

		try {
			const response = await eden.api.transcribe.get({
				query: cursor ? { cursor, limit: 20 } : { limit: 20 },
			});

			if (response.error) {
				throw new Error(t("transcriptionList.failedToFetch"));
			}

			let data: { transcriptions: Transcription[]; nextCursor: string | null; hasMore: boolean };
			if (response.data instanceof Response) {
				data = await response.data.json();
			} else {
				data = response.data as typeof data;
			}

			const items = (data.transcriptions ?? []).map((item) => ({
				...item,
				createdAt: new Date(item.createdAt),
			}));

			setTranscriptions((prev) => (cursor ? [...prev, ...items] : items));
			setNextCursor(data.nextCursor ?? null);
			setHasMore(data.hasMore ?? false);
		} catch (err) {
			console.error("Fetch error:", err);
			setError(err instanceof Error ? err.message : t("transcriptionList.errorOccurred"));
		} finally {
			setLoading(false);
			setInitialLoading(false);
		}
	};

	onMount(async () => {
		fetchTranscriptions();

		unlisten = await listen("transcription-created", () => {
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
					<Loader class="w-6 h-6 animate-spin text-slate-400 dark:text-slate-500" />
				</div>
			</Show>

			<Show when={error()}>
				<div class="text-center py-8">
					<p class="text-red-500 dark:text-red-400 mb-2">{error()}</p>
					<button
						type="button"
						onClick={() => fetchTranscriptions()}
						class="text-sm text-blue-500 dark:text-blue-400 hover:underline"
					>
						{t("transcriptionList.tryAgain")}
					</button>
				</div>
			</Show>

			<Show when={!initialLoading() && !error() && transcriptions().length === 0}>
				<div class="text-center py-12">
					<p class="text-slate-500 dark:text-slate-400 text-lg mb-2">{t("transcriptionList.noTranscriptions")}</p>
					<p class="text-slate-400 dark:text-slate-500 text-sm">{t("transcriptionList.useCommandToRecord")}</p>
				</div>
			</Show>

			<Show when={!initialLoading() && transcriptions().length > 0}>
				<For each={groupedTranscriptions()}>
					{(group) => (
						<div>
							<h3 class="text-sm font-medium text-slate-500 dark:text-slate-400 mb-2 px-1">{group.label}</h3>
							<div class="bg-white dark:bg-midnight-800 rounded-lg divide-y divide-slate-100 dark:divide-midnight-700">
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
						<Loader class="w-5 h-5 animate-spin text-slate-400 dark:text-slate-500" />
					</div>
				</Show>

				<Show when={!hasMore() && transcriptions().length > 0}>
					<p class="text-center text-slate-400 dark:text-slate-500 text-sm py-4">{t("transcriptionList.noMore")}</p>
				</Show>
			</Show>
		</div>
	);
}
