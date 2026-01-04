import type { Client } from "@tauri-apps/plugin-stronghold";
import { Stronghold } from "@tauri-apps/plugin-stronghold";
import { appDataDir } from "@tauri-apps/api/path";

const TOKEN_KEY = "auth_token";
const CLIENT_NAME = "voxfusion_client";
const VAULT_PASSWORD = "vault password";

let strongholdInstance: Stronghold | null = null;
let clientInstance: Client | null = null;

const initStronghold = async () => {
	if (strongholdInstance && clientInstance) {
		return { stronghold: strongholdInstance, client: clientInstance };
	}

	const vaultPath = `${await appDataDir()}/vault.hold`;
	const stronghold = await Stronghold.load(vaultPath, VAULT_PASSWORD);

	let client: Client;
	try {
		client = await stronghold.loadClient(CLIENT_NAME);
	} catch {
		client = await stronghold.createClient(CLIENT_NAME);
	}

	strongholdInstance = stronghold;
	clientInstance = client;

	return { stronghold, client };
};

export const tokenManager = {
	storeToken: async (token: string) => {
		const { stronghold, client } = await initStronghold();
		const store = client.getStore();
		const data = Array.from(new TextEncoder().encode(token));
		await store.insert(TOKEN_KEY, data);
		await stronghold.save();
	},
	getToken: async (): Promise<string | null> => {
		const { client } = await initStronghold();
		const store = client.getStore();
		try {
			const data = await store.get(TOKEN_KEY);
			if (!data) return null;
			return new TextDecoder().decode(new Uint8Array(data));
		} catch {
			return null;
		}
	},
	deleteToken: async () => {
		const { stronghold, client } = await initStronghold();
		const store = client.getStore();
		await store.remove(TOKEN_KEY);
		await stronghold.save();
	},
};
