import { treaty } from "@elysiajs/eden";
import type { App } from "@voxfusion/server";
import { tokenManager } from "./tokenManager";

const client = treaty<App>("localhost:3000", {
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
