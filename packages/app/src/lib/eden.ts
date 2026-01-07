import { treaty } from "@elysiajs/eden";
import type { App } from "@voxfusion/server";
import { tokenManager } from "./tokenManager";

const client = treaty<App>("localhost:3000", {
	onRequest: async (_path, options) => {
		const token = await tokenManager.getToken();
		if (token) {
			// Treaty lowercases headers during merge. If we set `Authorization` (capital-A) here,
			// Treaty can later also create/merge `authorization` (lowercase) for the same header,
			// resulting in two Authorization header entries on the wire:
			// "Bearer X, Bearer X".
			//
			// Workaround: always set the lowercase key.
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
