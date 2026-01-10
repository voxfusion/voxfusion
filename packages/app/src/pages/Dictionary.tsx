import { For, Show, createSignal, onMount } from "solid-js";
import { useI18n } from "../i18n";
import { BookOpen, Loader, Pencil, Plus, Trash2, X, Check } from "lucide-solid";
import eden from "../lib/eden";

type DictionaryWord = {
	id: string;
	word: string;
	createdAt: string;
	updatedAt: string;
};

export default function Dictionary() {
	const [t] = useI18n();
	const [words, setWords] = createSignal<DictionaryWord[]>([]);
	const [loading, setLoading] = createSignal(true);
	const [newWord, setNewWord] = createSignal("");
	const [adding, setAdding] = createSignal(false);
	const [editingId, setEditingId] = createSignal<string | null>(null);
	const [editingWord, setEditingWord] = createSignal("");

	const fetchWords = async () => {
		setLoading(true);
		try {
			const response = await eden.api.dictionary.get();
			if (!response.error && response.data) {
				const data =
					response.data instanceof Response ? await response.data.json() : response.data;
				setWords(data.words ?? []);
			}
		} finally {
			setLoading(false);
		}
	};

	onMount(() => {
		fetchWords();
	});

	const handleAdd = async () => {
		const word = newWord().trim();
		if (!word || adding()) return;

		setAdding(true);
		try {
			const response = await eden.api.dictionary.post({ word });
			if (!response.error) {
				await fetchWords();
				setNewWord("");
			}
		} finally {
			setAdding(false);
		}
	};

	const handleKeyDown = (e: KeyboardEvent) => {
		if (e.key === "Enter") {
			handleAdd();
		}
	};

	const handleDelete = async (id: string) => {
		// Optimistic update
		setWords(words().filter((w) => w.id !== id));
		await eden.api.dictionary({ id }).delete();
	};

	const startEdit = (word: DictionaryWord) => {
		setEditingId(word.id);
		setEditingWord(word.word);
	};

	const cancelEdit = () => {
		setEditingId(null);
		setEditingWord("");
	};

	const handleEdit = async (id: string) => {
		const word = editingWord().trim();
		if (!word) return;

		// Optimistic update
		setWords(words().map((w) => (w.id === id ? { ...w, word } : w)));
		setEditingId(null);
		setEditingWord("");

		await eden.api.dictionary({ id }).patch({ word });
	};

	const handleEditKeyDown = (e: KeyboardEvent, id: string) => {
		if (e.key === "Enter") {
			handleEdit(id);
		} else if (e.key === "Escape") {
			cancelEdit();
		}
	};

	return (
		<div class="min-h-screen px-6 py-8">
			<div class="max-w-2xl mx-auto">
				<div class="mb-8 flex items-center justify-between">
					<div>
						<h1 class="text-2xl font-bold text-slate-800">{t("dictionary.title")}</h1>
						<p class="text-slate-500 text-sm mt-1">{t("dictionary.description")}</p>
					</div>
					<Show when={words().length > 0}>
						<span class="text-sm text-slate-400">
							{t("dictionary.wordCount").replace("{count}", String(words().length))}
						</span>
					</Show>
				</div>

				{/* Add Word Form */}
				<div class="bg-white rounded-xl p-4 shadow-sm border border-slate-200 mb-6">
					<div class="flex gap-3">
						<input
							type="text"
							value={newWord()}
							onInput={(e) => setNewWord(e.currentTarget.value)}
							onKeyDown={handleKeyDown}
							placeholder={t("dictionary.wordPlaceholder")}
							class="flex-1 px-4 py-2 border border-slate-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-primary-500 focus:border-transparent"
						/>
						<button
							type="button"
							onClick={handleAdd}
							disabled={!newWord().trim() || adding()}
							class="flex items-center gap-2 px-4 py-2 bg-primary-500 text-white rounded-lg hover:bg-primary-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
						>
							<Show when={adding()} fallback={<Plus class="w-4 h-4" />}>
								<Loader class="w-4 h-4 animate-spin" />
							</Show>
							{t("dictionary.addWord")}
						</button>
					</div>
				</div>

				{/* Loading State */}
				<Show when={loading()}>
					<div class="flex justify-center py-12">
						<Loader class="w-6 h-6 animate-spin text-slate-400" />
					</div>
				</Show>

				{/* Empty State */}
				<Show when={!loading() && words().length === 0}>
					<div class="bg-white rounded-xl p-12 shadow-sm border border-slate-200 flex flex-col items-center justify-center text-center">
						<div class="w-16 h-16 bg-slate-100 rounded-full flex items-center justify-center mb-4">
							<BookOpen class="w-8 h-8 text-slate-400" />
						</div>
						<h2 class="text-lg font-semibold text-slate-700 mb-2">
							{t("dictionary.emptyState")}
						</h2>
						<p class="text-slate-500 text-sm max-w-sm">
							{t("dictionary.emptyStateDescription")}
						</p>
					</div>
				</Show>

				{/* Word List */}
				<Show when={!loading() && words().length > 0}>
					<div class="space-y-2">
						<For each={words()}>
							{(word) => (
								<div class="bg-white rounded-lg px-4 py-3 shadow-sm border border-slate-200 flex items-center justify-between group">
									<Show
										when={editingId() === word.id}
										fallback={
											<span class="text-slate-800 font-medium">{word.word}</span>
										}
									>
										<input
											type="text"
											value={editingWord()}
											onInput={(e) => setEditingWord(e.currentTarget.value)}
											onKeyDown={(e) => handleEditKeyDown(e, word.id)}
											class="flex-1 px-2 py-1 border border-slate-200 rounded focus:outline-none focus:ring-2 focus:ring-primary-500 focus:border-transparent"
											autofocus
										/>
									</Show>

									<div class="flex items-center gap-1">
										<Show
											when={editingId() === word.id}
											fallback={
												<>
													<button
														type="button"
														onClick={() => startEdit(word)}
														class="p-2 text-slate-400 hover:text-slate-600 opacity-0 group-hover:opacity-100 transition-opacity"
														title={t("dictionary.edit")}
													>
														<Pencil class="w-4 h-4" />
													</button>
													<button
														type="button"
														onClick={() => handleDelete(word.id)}
														class="p-2 text-slate-400 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-opacity"
														title={t("dictionary.delete")}
													>
														<Trash2 class="w-4 h-4" />
													</button>
												</>
											}
										>
											<button
												type="button"
												onClick={() => handleEdit(word.id)}
												class="p-2 text-green-500 hover:text-green-600"
												title={t("dictionary.save")}
											>
												<Check class="w-4 h-4" />
											</button>
											<button
												type="button"
												onClick={cancelEdit}
												class="p-2 text-slate-400 hover:text-slate-600"
												title={t("dictionary.cancel")}
											>
												<X class="w-4 h-4" />
											</button>
										</Show>
									</div>
								</div>
							)}
						</For>
					</div>
				</Show>
			</div>
		</div>
	);
}
