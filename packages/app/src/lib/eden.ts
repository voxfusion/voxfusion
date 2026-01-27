import { treaty } from "@elysiajs/eden";
import type { App } from "@voxfusion/server";
import { tokenManager } from "./tokenManager";
import { API_BASE_URL } from "./authClient";

const client = treaty<App>(API_BASE_URL.replace(/^https?:\/\//, ""), {
	onRequest: async (_path, options) => {
		const token = await tokenManager.getToken();
		if (token) {
			Reflect.deleteProperty(options.headers as object, "authorization");
			Reflect.deleteProperty(options.headers as object, "Authorization");
			options.headers = {
				...options.headers,
				authorization: `Bearer ${token}`,
			};
		}

		return options;
	},
});

export default client;
