import type { Client } from "@tauri-apps/plugin-stronghold";
import { Stronghold } from "@tauri-apps/plugin-stronghold";
import { appDataDir } from "@tauri-apps/api/path";

const TOKEN_KEY = "auth_token";
const CLIENT_NAME = "voxfusion_client";
const VAULT_PASSWORD = "vault password";

let strongholdInstance: Stronghold | null = null;
let clientInstance: Client | null = null;

export const tokenManager = {
	init: async () => {
		if (strongholdInstance && clientInstance) return;

		const vaultPath = `${await appDataDir()}/vault.hold`;
		strongholdInstance = await Stronghold.load(vaultPath, VAULT_PASSWORD);

		try {
			clientInstance = await strongholdInstance.loadClient(CLIENT_NAME);
		} catch {
			clientInstance = await strongholdInstance.createClient(CLIENT_NAME);
		}
	},
	storeToken: async (token: string) => {
		const store = clientInstance!.getStore();
		const data = Array.from(new TextEncoder().encode(token));
		await store.insert(TOKEN_KEY, data);
		await strongholdInstance!.save();
	},
	getToken: async (): Promise<string | null> => {
		const store = clientInstance!.getStore();
		try {
			const data = await store.get(TOKEN_KEY);
			if (!data) return null;
			return new TextDecoder().decode(new Uint8Array(data));
		} catch {
			return null;
		}
	},
	deleteToken: async () => {
		const store = clientInstance!.getStore();
		await store.remove(TOKEN_KEY);
		await strongholdInstance!.save();
	},
};
