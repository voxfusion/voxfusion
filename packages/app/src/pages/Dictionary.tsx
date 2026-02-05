import { For, Show, createSignal, onMount } from "solid-js";
import { useI18n } from "../i18n";
import { Loader } from "lucide-solid";
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
		<div class="min-h-screen bg-black px-6 py-8">
			<div class="max-w-2xl mx-auto">
				{/* Header */}
				<div class="mb-8">
					<div class="flex items-center justify-between">
						<h1 class="font-mono text-[#e0e0e0] text-lg">
							<span class="text-[#ff3e00]">[DICTIONARY]</span>
							<span class="text-[#666]"> {">"} </span>
							<span class="text-[#888]">CUSTOM_TERMS</span>
						</h1>
						<Show when={words().length > 0}>
							<span class="text-[#666] font-mono text-xs uppercase">
								{t("dictionary.wordCount").replace("{count}", String(words().length))}
							</span>
						</Show>
					</div>
					<p class="text-[#666] font-mono text-xs mt-2 uppercase tracking-wide">
						{t("dictionary.description")}
					</p>
				</div>

				{/* Add Word Input */}
				<div class="bg-[#111] border border-[#222] p-4 mb-6">
					<div class="flex gap-3">
						<input
							type="text"
							value={newWord()}
							onInput={(e) => setNewWord(e.currentTarget.value)}
							onKeyDown={handleKeyDown}
							placeholder={t("dictionary.wordPlaceholder")}
							class="flex-1 px-4 py-2 bg-[#0a0a0a] border border-[#333] text-[#e0e0e0] font-mono placeholder-[#666] focus:outline-none focus:border-[#ff3e00] transition-colors"
						/>
						<button
							type="button"
							onClick={handleAdd}
							disabled={!newWord().trim() || adding()}
							class="flex items-center gap-2 px-4 py-2 bg-[#ff3e00] text-black font-mono uppercase tracking-wider text-sm hover:bg-[#ff5722] disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
						>
							<Show when={adding()} fallback={<span>+</span>}>
								<Loader class="w-4 h-4 animate-spin" />
							</Show>
							{t("dictionary.addWord")}
						</button>
					</div>
				</div>

				{/* Loading State */}
				<Show when={loading()}>
					<div class="bg-[#111] border border-[#222] p-12 flex justify-center">
						<Loader class="w-6 h-6 animate-spin text-[#ff3e00]" />
					</div>
				</Show>

				{/* Empty State */}
				<Show when={!loading() && words().length === 0}>
					<div class="bg-[#111] border border-[#222] p-12 flex flex-col items-center justify-center text-center">
						<div class="font-mono text-[#666] text-sm mb-4">
							<span class="text-[#ff3e00]">[INFO]</span> NO_TERMS_FOUND
						</div>
						<p class="text-[#888] font-mono text-xs uppercase tracking-wide">
							{t("dictionary.emptyState")}
						</p>
						<p class="text-[#666] font-mono text-xs mt-2 max-w-sm">
							{t("dictionary.emptyStateDescription")}
						</p>
					</div>
				</Show>

				{/* Word List */}
				<Show when={!loading() && words().length > 0}>
					<div class="space-y-1">
						<For each={words()}>
							{(word) => (
								<div class="bg-[#111] border border-[#222] px-4 py-3 flex items-center justify-between group hover:border-[#333] transition-colors">
									<Show
										when={editingId() === word.id}
										fallback={
											<span class="text-[#e0e0e0] font-mono">{word.word}</span>
										}
									>
										<input
											type="text"
											value={editingWord()}
											onInput={(e) => setEditingWord(e.currentTarget.value)}
											onKeyDown={(e) => handleEditKeyDown(e, word.id)}
											class="flex-1 px-2 py-1 bg-[#0a0a0a] border border-[#333] text-[#e0e0e0] font-mono focus:outline-none focus:border-[#ff3e00] transition-colors mr-4"
											autofocus
										/>
									</Show>

									<div class="flex items-center gap-2 font-mono text-xs">
										<Show
											when={editingId() === word.id}
											fallback={
												<>
													<button
														type="button"
														onClick={() => startEdit(word)}
														class="text-[#666] hover:text-[#ff3e00] opacity-0 group-hover:opacity-100 transition-all uppercase tracking-wider"
														title={t("dictionary.edit")}
													>
														[EDIT]
													</button>
													<button
														type="button"
														onClick={() => handleDelete(word.id)}
														class="text-[#666] hover:text-[#ff3e00] opacity-0 group-hover:opacity-100 transition-all uppercase tracking-wider"
														title={t("dictionary.delete")}
													>
														[DEL]
													</button>
												</>
											}
										>
											<button
												type="button"
												onClick={() => handleEdit(word.id)}
												class="text-[#00ff88] hover:text-[#33ff99] uppercase tracking-wider"
												title={t("dictionary.save")}
											>
												[SAVE]
											</button>
											<button
												type="button"
												onClick={cancelEdit}
												class="text-[#666] hover:text-[#888] uppercase tracking-wider"
												title={t("dictionary.cancel")}
											>
												[CANCEL]
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
