import { Result } from "better-result";
import { ChevronDown } from "lucide-solid";
import { For, type JSX, Show, createSignal } from "solid-js";
import { useI18n } from "../i18n";
import type { CommandResult } from "../lib/commands/invokeResult";

export interface EditableWord {
	id: string;
	word: string;
}

const ADD_FORM_CLASSES = {
	page: {
		row: "flex gap-3",
		input:
			"flex-1 px-4 py-2 bg-th-input border border-border-strong text-txt-primary font-mono placeholder-txt-muted focus:outline-none focus:border-ac transition-colors",
		button:
			"flex items-center gap-2 px-4 py-2 bg-ac text-ac-on font-mono uppercase tracking-wider text-sm hover:bg-ac-hover disabled:opacity-50 transition-colors",
	},
	nested: {
		row: "flex gap-2",
		input:
			"flex-1 px-3 py-1.5 bg-th-input border border-border-strong text-txt-primary font-mono text-sm placeholder-txt-muted focus:outline-none focus:border-ac transition-colors",
		button:
			"flex items-center gap-1 px-3 py-1.5 bg-ac text-ac-on font-mono uppercase tracking-wider text-xs hover:bg-ac-hover disabled:opacity-50 transition-colors",
	},
} as const;

interface AddWordFormProps {
	variant: keyof typeof ADD_FORM_CLASSES;
	value: string;
	onValueChange: (value: string) => void;
	onSubmit: () => void;
	adding: boolean;
}

export function AddWordForm(props: AddWordFormProps) {
	const [t] = useI18n();
	const classes = () => ADD_FORM_CLASSES[props.variant];
	return (
		<div class={classes().row}>
			<input
				type="text"
				value={props.value}
				onInput={(e) => props.onValueChange(e.currentTarget.value)}
				onKeyDown={(e) => {
					if (e.key === "Enter") props.onSubmit();
				}}
				placeholder={t("dictionary.wordPlaceholder")}
				class={classes().input}
			/>
			<button
				type="button"
				onClick={() => props.onSubmit()}
				disabled={!props.value.trim() || props.adding}
				class={classes().button}
			>
				<span>+</span>
				{t("dictionary.addWord")}
			</button>
		</div>
	);
}

const WORD_ROW_CLASSES = {
	flat: {
		row: "bg-th-surface border border-border px-4 py-3 flex items-center justify-between group hover:border-border-strong transition-colors",
		word: "text-txt-primary font-mono",
		editInput:
			"flex-1 px-2 py-1 bg-th-input border border-border-strong text-txt-primary font-mono focus:outline-none focus:border-ac transition-colors mr-4",
		actions: "flex items-center gap-2 font-mono text-xs",
		hoverButton:
			"text-txt-muted hover:text-ac opacity-0 group-hover:opacity-100 transition-all uppercase tracking-wider",
	},
	nested: {
		row: "px-3 py-2 bg-th-input border border-border flex items-center justify-between gap-2 group/word",
		word: "text-txt-primary font-mono text-sm flex-1 min-w-0 truncate",
		editInput:
			"flex-1 px-2 py-1 bg-th-base border border-border-strong text-txt-primary font-mono text-sm focus:outline-none focus:border-ac transition-colors",
		actions: "flex items-center gap-2 font-mono text-xs shrink-0",
		hoverButton:
			"text-txt-muted hover:text-ac opacity-0 group-hover/word:opacity-100 transition-all uppercase tracking-wider",
	},
} as const;

interface WordRowProps {
	variant: keyof typeof WORD_ROW_CLASSES;
	word: EditableWord;
	editing: boolean;
	editValue: string;
	onEditValueChange: (value: string) => void;
	onStartEdit: () => void;
	onSaveEdit: () => void;
	onCancelEdit: () => void;
	onDelete: () => void;
}

export function WordRow(props: WordRowProps) {
	const [t] = useI18n();
	const classes = () => WORD_ROW_CLASSES[props.variant];
	return (
		<div class={classes().row}>
			<Show when={props.editing} fallback={<span class={classes().word}>{props.word.word}</span>}>
				<input
					type="text"
					value={props.editValue}
					onInput={(e) => props.onEditValueChange(e.currentTarget.value)}
					onKeyDown={(e) => {
						if (e.key === "Enter") props.onSaveEdit();
						else if (e.key === "Escape") props.onCancelEdit();
					}}
					class={classes().editInput}
					autofocus
				/>
			</Show>
			<div class={classes().actions}>
				<Show
					when={props.editing}
					fallback={
						<>
							<button
								type="button"
								onClick={() => props.onStartEdit()}
								class={classes().hoverButton}
								title={t("dictionary.edit")}
							>
								[EDIT]
							</button>
							<button
								type="button"
								onClick={() => props.onDelete()}
								class={classes().hoverButton}
								title={t("dictionary.delete")}
							>
								[DEL]
							</button>
						</>
					}
				>
					<button
						type="button"
						onClick={() => props.onSaveEdit()}
						class="text-success hover:opacity-80 uppercase tracking-wider"
						title={t("dictionary.save")}
					>
						[SAVE]
					</button>
					<button
						type="button"
						onClick={() => props.onCancelEdit()}
						class="text-txt-muted hover:text-txt-secondary uppercase tracking-wider"
						title={t("dictionary.cancel")}
					>
						[CANCEL]
					</button>
				</Show>
			</div>
		</div>
	);
}

interface WordGroup {
	words: EditableWord[];
}

interface WordListEditorProps<G extends WordGroup> {
	groups: G[];
	keyOf: (group: G) => string;
	icon: (group: G) => JSX.Element;
	title: (group: G) => string;
	subtitle?: (group: G) => string;
	expanded: Set<string>;
	onToggle: (key: string) => void;
	onRemoveGroup: (group: G) => void;
	addWord: (group: G, word: string) => Promise<CommandResult<unknown>>;
	onWordAdded: (group: G) => Promise<void>;
	onEditWord: (group: G, wordId: string, word: string) => Promise<void>;
	onDeleteWord: (group: G, wordId: string) => void;
}

export default function WordListEditor<G extends WordGroup>(props: WordListEditorProps<G>) {
	const [t] = useI18n();
	const [newWordByKey, setNewWordByKey] = createSignal<Record<string, string>>({});
	const [addingKeys, setAddingKeys] = createSignal<Set<string>>(new Set());
	const [editingWordId, setEditingWordId] = createSignal<string | null>(null);
	const [editingWord, setEditingWord] = createSignal("");

	const handleAddWord = async (group: G) => {
		const key = props.keyOf(group);
		const word = (newWordByKey()[key] ?? "").trim();
		if (!word) return;
		if (addingKeys().has(key)) return;

		setAddingKeys((prev) => {
			const next = new Set(prev);
			next.add(key);
			return next;
		});
		const result = await props.addWord(group, word);
		if (Result.isOk(result)) {
			setNewWordByKey((prev) => ({ ...prev, [key]: "" }));
			await props.onWordAdded(group);
		}
		setAddingKeys((prev) => {
			const next = new Set(prev);
			next.delete(key);
			return next;
		});
	};

	const startEditWord = (wordId: string, word: string) => {
		setEditingWordId(wordId);
		setEditingWord(word);
	};

	const cancelEditWord = () => {
		setEditingWordId(null);
		setEditingWord("");
	};

	const handleEditWord = async (group: G, wordId: string) => {
		const word = editingWord().trim();
		if (!word) return;
		setEditingWordId(null);
		setEditingWord("");
		await props.onEditWord(group, wordId, word);
	};

	return (
		<div class="space-y-1">
			<For each={props.groups}>
				{(group) => {
					const key = () => props.keyOf(group);
					const isOpen = () => props.expanded.has(key());
					const wordValue = () => newWordByKey()[key()] ?? "";
					const isAdding = () => addingKeys().has(key());
					return (
						<div class="bg-th-surface border border-border group hover:border-border-strong transition-colors">
							<div class="flex items-stretch">
								<button
									type="button"
									onClick={() => props.onToggle(key())}
									class="flex-1 px-4 py-3 flex items-center gap-3 text-left min-w-0"
									title={isOpen() ? t("dictionary.collapse") : t("dictionary.expand")}
								>
									{props.icon(group)}
									<div class="flex flex-col min-w-0 flex-1">
										<span class="text-txt-primary font-mono truncate">{props.title(group)}</span>
										<Show when={props.subtitle?.(group)}>
											{(subtitle) => (
												<span class="text-txt-muted font-mono text-xs truncate">{subtitle()}</span>
											)}
										</Show>
									</div>
									<span class="text-txt-muted font-mono text-xs uppercase tracking-wider shrink-0">
										{t("dictionary.wordCount").replace("{count}", String(group.words.length))}
									</span>
									<ChevronDown
										class={`w-4 h-4 text-txt-muted shrink-0 transition-transform ${
											isOpen() ? "rotate-180" : ""
										}`}
									/>
								</button>
								<button
									type="button"
									onClick={() => props.onRemoveGroup(group)}
									class="px-3 flex items-center text-txt-muted hover:text-ac opacity-0 group-hover:opacity-100 transition-all font-mono text-xs uppercase tracking-wider shrink-0"
									title={t("appInstructions.delete")}
								>
									[DEL]
								</button>
							</div>
							<Show when={isOpen()}>
								<div class="border-t border-border px-4 py-3 space-y-2">
									<AddWordForm
										variant="nested"
										value={wordValue()}
										onValueChange={(value) =>
											setNewWordByKey((prev) => ({ ...prev, [key()]: value }))
										}
										onSubmit={() => handleAddWord(group)}
										adding={isAdding()}
									/>

									<Show when={group.words.length > 0}>
										<div class="space-y-1 pt-1">
											<For each={group.words}>
												{(word) => (
													<WordRow
														variant="nested"
														word={word}
														editing={editingWordId() === word.id}
														editValue={editingWord()}
														onEditValueChange={setEditingWord}
														onStartEdit={() => startEditWord(word.id, word.word)}
														onSaveEdit={() => handleEditWord(group, word.id)}
														onCancelEdit={cancelEditWord}
														onDelete={() => props.onDeleteWord(group, word.id)}
													/>
												)}
											</For>
										</div>
									</Show>
								</div>
							</Show>
						</div>
					);
				}}
			</For>
		</div>
	);
}
