import { treaty } from "@elysiajs/eden";
import type { App } from "@voxfusion/server";
import { tokenManager } from "./tokenManager";

const client = treaty<App>("localhost:3000", {
	onRequest: async (_path, options) => {
		try {
			const token = await tokenManager.getToken();
			if (token) {
				options.headers = {
					...options.headers,
					Authorization: `Bearer ${token}`,
					Cookie: `better-auth.session_token=${token}`,
				};
			}
		} catch (e) {
			console.error("Failed to get token for request:", e);
		}
		return options;
	},
});

export default client;
