import { invoke } from "@tauri-apps/api/core";

export interface DictionaryWord {
	id: string;
	word: string;
	createdAt: string;
	updatedAt: string;
}

export async function listDictionaryWords(): Promise<DictionaryWord[]> {
	return invoke<DictionaryWord[]>("list_dictionary_words");
}

export async function addDictionaryWord(word: string): Promise<DictionaryWord> {
	return invoke<DictionaryWord>("add_dictionary_word", { word });
}

export async function updateDictionaryWord(id: string, word: string): Promise<DictionaryWord> {
	return invoke<DictionaryWord>("update_dictionary_word", { id, word });
}

export async function deleteDictionaryWord(id: string): Promise<void> {
	await invoke("delete_dictionary_word", { id });
}

export async function getDictionaryPrompt(): Promise<string | null> {
	return invoke<string | null>("get_dictionary_prompt");
}
