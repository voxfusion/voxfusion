import { createSignal, For, Show, onMount, onCleanup } from "solid-js";
import { Loader } from "lucide-solid";
import TranscriptionCard from "./TranscriptionCard";
import eden from "../lib/eden";

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

export default function TranscriptionList() {
	const [transcriptions, setTranscriptions] = createSignal<Transcription[]>([]);
	const [loading, setLoading] = createSignal(false);
	const [initialLoading, setInitialLoading] = createSignal(true);
	const [nextCursor, setNextCursor] = createSignal<string | null>(null);
	const [hasMore, setHasMore] = createSignal(true);
	const [error, setError] = createSignal<string | null>(null);

	let sentinelRef: HTMLDivElement | undefined;
	let observer: IntersectionObserver | null = null;

	const fetchTranscriptions = async (cursor?: string) => {
		if (loading()) return;

		setLoading(true);
		setError(null);

		try {
			const response = await eden.api.transcribe.get({
				query: cursor ? { cursor, limit: 20 } : { limit: 20 },
			});

			if (response.error) {
				throw new Error("Failed to fetch transcriptions");
			}

			const data = response.data;

			setTranscriptions((prev) =>
				cursor ? [...prev, ...data.transcriptions] : data.transcriptions,
			);
			setNextCursor(data.nextCursor);
			setHasMore(data.hasMore);
		} catch (err) {
			setError(err instanceof Error ? err.message : "An error occurred");
		} finally {
			setLoading(false);
			setInitialLoading(false);
		}
	};

	const handleRatingChange = (id: string, rating: "up" | "down" | null) => {
		setTranscriptions((prev) => prev.map((t) => (t.id === id ? { ...t, rating } : t)));
	};

	onMount(() => {
		fetchTranscriptions();

		observer = new IntersectionObserver(
			(entries) => {
				const entry = entries[0];
				if (entry?.isIntersecting && hasMore() && !loading() && nextCursor()) {
					fetchTranscriptions(nextCursor()!);
				}
			},
			{ rootMargin: "100px" },
		);

		if (sentinelRef) {
			observer.observe(sentinelRef);
		}
	});

	onCleanup(() => {
		observer?.disconnect();
	});

	return (
		<div class="space-y-4">
			<Show when={initialLoading()}>
				<div class="flex items-center justify-center py-12">
					<Loader class="w-6 h-6 animate-spin text-slate-400" />
				</div>
			</Show>

			<Show when={error()}>
				<div class="text-center py-8">
					<p class="text-red-500 mb-2">{error()}</p>
					<button
						type="button"
						onClick={() => fetchTranscriptions()}
						class="text-sm text-blue-500 hover:underline"
					>
						Try again
					</button>
				</div>
			</Show>

			<Show when={!initialLoading() && !error() && transcriptions().length === 0}>
				<div class="text-center py-12">
					<p class="text-slate-500 text-lg mb-2">No transcriptions yet</p>
					<p class="text-slate-400 text-sm">
						Use Command+; to start recording and create your first transcription
					</p>
				</div>
			</Show>

			<Show when={!initialLoading() && transcriptions().length > 0}>
				<For each={transcriptions()}>
					{(transcription) => (
						<TranscriptionCard
							transcription={transcription}
							onRatingChange={handleRatingChange}
						/>
					)}
				</For>

				<div ref={sentinelRef} class="h-4" />

				<Show when={loading() && transcriptions().length > 0}>
					<div class="flex items-center justify-center py-4">
						<Loader class="w-5 h-5 animate-spin text-slate-400" />
					</div>
				</Show>

				<Show when={!hasMore() && transcriptions().length > 0}>
					<p class="text-center text-slate-400 text-sm py-4">No more transcriptions</p>
				</Show>
			</Show>
		</div>
	);
}
