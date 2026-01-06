import { Elysia, status } from "elysia";
import { auth } from "../auth";

export const requireAuth = new Elysia({ name: "requireAuth" })
	.derive(async ({ request }) => {
		const session = await auth.api.getSession({
			headers: request.headers,
		});
		return { session };
	})
	.onBeforeHandle(({ session }) => {
		if (!session || !session.user) {
			return status(401, { error: "Unauthorized" });
		}
	});
