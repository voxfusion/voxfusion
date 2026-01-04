import { createAuthClient } from "better-auth/client";
import { tokenManager } from "./tokenManager";

// Create a custom fetch that adds the Bearer token
const customFetch = async (url: string | URL | Request, init?: RequestInit): Promise<Response> => {
	try {
		const token = await tokenManager.getToken();
		if (token && typeof token === "string") {
			const headers = new Headers(init?.headers);
			document.cookie = `better-auth.session_token=${token}; path=/`;

			headers.set("Authorization", `Bearer ${token}`);
			return fetch(url, {
				...init,
				headers,
			});
		}
	} catch {
		// If token retrieval fails, continue without token
	}
	return fetch(url, init);
};

export const authClient = createAuthClient({
	baseURL: "http://localhost:3000",
	fetch: customFetch,
});
