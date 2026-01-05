import {
	createAuthClient,
	type BetterFetchPlugin,
	type RequestContext,
	type ResponseContext,
	type SuccessContext,
} from "better-auth/client";
import { tokenManager } from "./tokenManager";

const API_BASE_URL = "http://localhost:3000";

/**
 * Extracts token from Set-Cookie header
 */
const extractTokenFromCookie = (cookieHeader: string | null): string | null => {
	if (!cookieHeader) return null;
	const sessionCookie = cookieHeader
		.split(";")
		.map((c) => c.trim())
		.find((c) => c.startsWith("better-auth.session_token="));
	return sessionCookie?.split("=")[1] || null;
};

/**
 * Extracts token from response body (if present in session data)
 */
const extractTokenFromBody = (data: unknown): string | null => {
	if (typeof data !== "object" || data === null) return null;

	// Check for token in various possible locations
	const obj = data as Record<string, unknown>;

	// Check obj.token
	if (obj.token && typeof obj.token === "string") {
		return obj.token;
	}

	// Check obj.session.token
	if (obj.session && typeof obj.session === "object" && obj.session !== null) {
		const session = obj.session as Record<string, unknown>;
		if (session.token && typeof session.token === "string") {
			return session.token;
		}
	}

	// Check obj.data.session.token
	if (obj.data && typeof obj.data === "object" && obj.data !== null) {
		const dataObj = obj.data as Record<string, unknown>;
		if (dataObj.session && typeof dataObj.session === "object" && dataObj.session !== null) {
			const session = dataObj.session as Record<string, unknown>;
			if (session.token && typeof session.token === "string") {
				return session.token;
			}
		}
	}

	return null;
};

const tokenPlugin: BetterFetchPlugin = {
	id: "auth-token-plugin",
	name: "Auth Token Plugin",
	version: "1.0.0",
	hooks: {
		onRequest: async (context: RequestContext) => {
			const urlString =
				typeof context.url === "string"
					? context.url
					: context.url instanceof URL
						? context.url.toString()
						: context.url;

			if (urlString.includes(API_BASE_URL) && urlString.includes("/api/auth")) {
				const token = await tokenManager.getToken();

				if (token) {
					context.headers.set("Authorization", `Bearer ${token}`);
				}
			}

			return context;
		},
		onResponse: async (context: ResponseContext) => {
			const urlString =
				typeof context.request.url === "string"
					? context.request.url
					: context.request.url instanceof URL
						? context.request.url.toString()
						: context.request.url;

			if (!urlString.includes(API_BASE_URL) || !urlString.includes("/api/auth")) {
				return context;
			}

			const setCookieHeader = context.response.headers.get("Set-Cookie");
			if (setCookieHeader) {
				const newToken = extractTokenFromCookie(setCookieHeader);
				if (newToken) {
					await tokenManager.storeToken(newToken);
				}
			}

			return context;
		},
		onSuccess: async (context: SuccessContext) => {
			const urlString =
				typeof context.request.url === "string"
					? context.request.url
					: context.request.url instanceof URL
						? context.request.url.toString()
						: context.request.url;

			// Only process auth-related responses
			if (!urlString.includes(API_BASE_URL) || !urlString.includes("/api/auth")) {
				return;
			}

			// Check for token in response body
			const newToken = extractTokenFromBody(context.data);
			if (newToken) {
				await tokenManager.storeToken(newToken);
			}
		},
	},
};

export const authClient = createAuthClient({
	baseURL: API_BASE_URL,
	fetchOptions: {
		plugins: [tokenPlugin],
	},
});
