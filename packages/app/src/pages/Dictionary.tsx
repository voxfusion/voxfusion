import { invoke } from "@tauri-apps/api/core";
import { Loader } from "lucide-solid";
import { For, Show, createSignal, onMount } from "solid-js";
import { useI18n } from "../i18n";
import { capture } from "../lib/posthog";

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
			const result = await invoke<DictionaryWord[]>("list_dictionary_words");
			setWords(result);
		} finally {
			setLoading(false);
		}
	};

	onMount(() => {
		capture("$pageview", { $current_url: "/dictionary" });
		fetchWords();
	});

	const handleAdd = async () => {
		const word = newWord().trim();
		if (!word || adding()) return;

		setAdding(true);
		try {
			await invoke("add_dictionary_word", { word });
			capture("dictionary_word_added");
			await fetchWords();
			setNewWord("");
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
		capture("dictionary_word_deleted");
		setWords(words().filter((w) => w.id !== id));
		await invoke("delete_dictionary_word", { id });
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

		capture("dictionary_word_edited");
		setWords(words().map((w) => (w.id === id ? { ...w, word } : w)));
		setEditingId(null);
		setEditingWord("");

		await invoke("update_dictionary_word", { id, word });
	};

	const handleEditKeyDown = (e: KeyboardEvent, id: string) => {
		if (e.key === "Enter") {
			handleEdit(id);
		} else if (e.key === "Escape") {
			cancelEdit();
		}
	};

	return (
		<div class="min-h-screen bg-th-base px-6 py-8">
			<div class="max-w-2xl mx-auto">
				<div class="mb-8">
					<div class="flex items-center justify-between">
						<h1 class="font-mono text-txt-primary text-sm">
							<span class="text-ac">[DICTIONARY]</span>
							<span class="text-txt-muted"> {">"} </span>
							<span class="text-txt-secondary">CUSTOM_TERMS</span>
						</h1>
						<Show when={words().length > 0}>
							<span class="text-txt-muted font-mono text-xs uppercase">
								{t("dictionary.wordCount").replace("{count}", String(words().length))}
							</span>
						</Show>
					</div>
					<p class="text-txt-muted font-mono text-xs mt-2 uppercase tracking-wide">
						{t("dictionary.description")}
					</p>
				</div>

				<div class="bg-th-surface border border-border p-4 mb-6">
					<div class="flex gap-3">
						<input
							type="text"
							value={newWord()}
							onInput={(e) => setNewWord(e.currentTarget.value)}
							onKeyDown={handleKeyDown}
							placeholder={t("dictionary.wordPlaceholder")}
							class="flex-1 px-4 py-2 bg-th-input border border-border-strong text-txt-primary font-mono placeholder-txt-muted focus:outline-none focus:border-ac transition-colors"
						/>
						<button
							type="button"
							onClick={handleAdd}
							disabled={!newWord().trim() || adding()}
							class="flex items-center gap-2 px-4 py-2 bg-ac text-ac-on font-mono uppercase tracking-wider text-sm hover:bg-ac-hover disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
						>
							<Show when={adding()} fallback={<span>+</span>}>
								<Loader class="w-4 h-4 animate-spin" />
							</Show>
							{t("dictionary.addWord")}
						</button>
					</div>
				</div>

				<Show when={loading()}>
					<div class="bg-th-surface border border-border p-12 flex justify-center">
						<Loader class="w-6 h-6 animate-spin text-ac" />
					</div>
				</Show>

				<Show when={!loading() && words().length === 0}>
					<div class="bg-th-surface border border-border p-12 flex flex-col items-center justify-center text-center">
						<div class="font-mono text-txt-muted text-sm mb-4">
							<span class="text-ac">[INFO]</span> NO_TERMS_FOUND
						</div>
						<p class="text-txt-secondary font-mono text-xs uppercase tracking-wide">
							{t("dictionary.emptyState")}
						</p>
						<p class="text-txt-muted font-mono text-xs mt-2 max-w-sm">
							{t("dictionary.emptyStateDescription")}
						</p>
					</div>
				</Show>

				<Show when={!loading() && words().length > 0}>
					<div class="space-y-1">
						<For each={words()}>
							{(word) => (
								<div class="bg-th-surface border border-border px-4 py-3 flex items-center justify-between group hover:border-border-strong transition-colors">
									<Show
										when={editingId() === word.id}
										fallback={<span class="text-txt-primary font-mono">{word.word}</span>}
									>
										<input
											type="text"
											value={editingWord()}
											onInput={(e) => setEditingWord(e.currentTarget.value)}
											onKeyDown={(e) => handleEditKeyDown(e, word.id)}
											class="flex-1 px-2 py-1 bg-th-input border border-border-strong text-txt-primary font-mono focus:outline-none focus:border-ac transition-colors mr-4"
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
														class="text-txt-muted hover:text-ac opacity-0 group-hover:opacity-100 transition-all uppercase tracking-wider"
														title={t("dictionary.edit")}
													>
														[EDIT]
													</button>
													<button
														type="button"
														onClick={() => handleDelete(word.id)}
														class="text-txt-muted hover:text-ac opacity-0 group-hover:opacity-100 transition-all uppercase tracking-wider"
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
												class="text-success hover:opacity-80 uppercase tracking-wider"
												title={t("dictionary.save")}
											>
												[SAVE]
											</button>
											<button
												type="button"
												onClick={cancelEdit}
												class="text-txt-muted hover:text-txt-secondary uppercase tracking-wider"
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
