import { invoke } from "@tauri-apps/api/core";


export const tokenManager = {
	storeToken: async (token: string) => await invoke("store_token", { token }),
	getToken: async () => await invoke("get_token"),
	deleteToken: async () => await invoke("delete_token"),
};