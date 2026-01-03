import { invoke } from "@tauri-apps/api/core";

export interface TokenManager {
	storeToken: (token: string) => Promise<void>;
	getToken: () => Promise<string | null>;
	deleteToken: () => Promise<void>;
}

export const tokenManager: TokenManager = {
	storeToken: async (token: string) => {
		await invoke("store_token", { token });
	},

	getToken: async () => {
		return await invoke("get_token");
	},

	deleteToken: async () => {
		await invoke("delete_token");
	},
};