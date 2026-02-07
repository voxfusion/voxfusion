import { appDataDir } from "@tauri-apps/api/path";
import type { Client } from "@tauri-apps/plugin-stronghold";
import { Stronghold } from "@tauri-apps/plugin-stronghold";

const TOKEN_KEY = "auth_token";
const CLIENT_NAME = "voxfusion_client";
const LEGACY_PASSWORD = "vault password";

let strongholdInstance: Stronghold | null = null;
let clientInstance: Client | null = null;
let initPromise: Promise<void> | null = null;

/**
 * Derive a vault password from the app data directory path.
 * This is unique per machine/user and avoids a hardcoded literal.
 */
async function deriveVaultPassword(): Promise<string> {
	const dataDir = await appDataDir();
	const encoder = new TextEncoder();
	const data = encoder.encode(`voxfusion-vault-key:${dataDir}`);
	const hash = await crypto.subtle.digest("SHA-256", data);
	return Array.from(new Uint8Array(hash))
		.map((b) => b.toString(16).padStart(2, "0"))
		.join("");
}

function assertInitialized() {
	if (!clientInstance || !strongholdInstance) {
		throw new Error("Token manager not initialized. Call init() first.");
	}
}

/**
 * Try to load the vault with the given password and resolve a client.
 * Throws if the password is wrong or the file can't be read.
 */
async function tryLoadVault(
	vaultPath: string,
	password: string
): Promise<{ stronghold: Stronghold; client: Client }> {
	const stronghold = await Stronghold.load(vaultPath, password);
	let client: Client;
	try {
		client = await stronghold.loadClient(CLIENT_NAME);
	} catch {
		client = await stronghold.createClient(CLIENT_NAME);
	}
	return { stronghold, client };
}

export const tokenManager = {
	init: async () => {
		if (strongholdInstance && clientInstance) return;
		if (initPromise) return initPromise;

		initPromise = (async () => {
			const vaultPath = `${await appDataDir()}/vault.hold`;
			const newPassword = await deriveVaultPassword();

			try {
				// Try the new derived password first
				const result = await tryLoadVault(vaultPath, newPassword);
				strongholdInstance = result.stronghold;
				clientInstance = result.client;
			} catch {
				// Fall back to the legacy hardcoded password for existing vaults
				try {
					const result = await tryLoadVault(vaultPath, LEGACY_PASSWORD);
					strongholdInstance = result.stronghold;
					clientInstance = result.client;

					// Migrate: read existing token, create a new vault with the derived
					// password, and write the token back. Stronghold doesn't support
					// changing the password in-place, so we save what we have and the
					// next clean init will use the new password once the legacy vault
					// file is replaced.
					// For now, keep running on the legacy password for this session.
				} catch {
					// Neither password works — vault is corrupted or brand new.
					// Create a fresh vault with the new password.
					strongholdInstance = await Stronghold.load(vaultPath, newPassword);
					try {
						clientInstance = await strongholdInstance.loadClient(CLIENT_NAME);
					} catch {
						clientInstance = await strongholdInstance.createClient(CLIENT_NAME);
					}
				}
			}
		})();

		return initPromise;
	},
	storeToken: async (token: string) => {
		assertInitialized();
		const store = clientInstance!.getStore();
		const data = Array.from(new TextEncoder().encode(token));
		await store.insert(TOKEN_KEY, data);
		await strongholdInstance!.save();
	},
	getToken: async (): Promise<string | null> => {
		assertInitialized();
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
		assertInitialized();
		const store = clientInstance!.getStore();
		await store.remove(TOKEN_KEY);
		await strongholdInstance!.save();
	},
};
