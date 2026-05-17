import type { CommandResult } from "./invokeResult";
import { invokeResult } from "./invokeResult";

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

export async function listDictionaryWords(): Promise<CommandResult<DictionaryWord[]>> {
	return invokeResult<DictionaryWord[]>("list_dictionary_words");
}

export async function addDictionaryWord(word: string): Promise<CommandResult<DictionaryWord>> {
	return invokeResult<DictionaryWord>("add_dictionary_word", { word });
}

export async function updateDictionaryWord(
	id: string,
	word: string
): Promise<CommandResult<DictionaryWord>> {
	return invokeResult<DictionaryWord>("update_dictionary_word", { id, word });
}

export async function deleteDictionaryWord(id: string): Promise<CommandResult<void>> {
	return invokeResult<void>("delete_dictionary_word", { id });
}

export async function getDictionaryPrompt(): Promise<CommandResult<string | null>> {
	return invokeResult<string | null>("get_dictionary_prompt");
}

export async function listAppDictionaries(): Promise<CommandResult<AppDictionary[]>> {
	return invokeResult<AppDictionary[]>("list_app_dictionaries");
}

export async function addAppDictionaryWord(
	bundleId: string,
	appName: string,
	word: string
): Promise<CommandResult<AppDictionaryWord>> {
	return invokeResult<AppDictionaryWord>("add_app_dictionary_word", {
		bundleId,
		appName,
		word,
	});
}

export async function updateAppDictionaryWord(
	id: string,
	word: string
): Promise<CommandResult<AppDictionaryWord>> {
	return invokeResult<AppDictionaryWord>("update_app_dictionary_word", { id, word });
}

export async function deleteAppDictionaryWord(id: string): Promise<CommandResult<void>> {
	return invokeResult<void>("delete_app_dictionary_word", { id });
}

export async function deleteAppDictionary(bundleId: string): Promise<CommandResult<void>> {
	return invokeResult<void>("delete_app_dictionary", { bundleId });
}
