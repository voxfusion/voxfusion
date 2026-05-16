import { invoke } from "@tauri-apps/api/core";

export interface DictionaryWord {
	id: string;
	word: string;
	createdAt: string;
	updatedAt: string;
}

export interface AppDictionaryWord {
	id: string;
	word: string;
	created_at: string;
	updated_at: string;
}

export interface AppDictionary {
	bundle_id: string;
	app_name: string;
	words: AppDictionaryWord[];
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

export async function listAppDictionaries(): Promise<AppDictionary[]> {
	return invoke<AppDictionary[]>("list_app_dictionaries");
}

export async function addAppDictionaryWord(
	bundleId: string,
	appName: string,
	word: string,
): Promise<AppDictionaryWord> {
	return invoke<AppDictionaryWord>("add_app_dictionary_word", {
		bundleId,
		appName,
		word,
	});
}

export async function updateAppDictionaryWord(
	id: string,
	word: string,
): Promise<AppDictionaryWord> {
	return invoke<AppDictionaryWord>("update_app_dictionary_word", { id, word });
}

export async function deleteAppDictionaryWord(id: string): Promise<void> {
	await invoke("delete_app_dictionary_word", { id });
}

export async function deleteAppDictionary(bundleId: string): Promise<void> {
	await invoke("delete_app_dictionary", { bundleId });
}
