import { Elysia, status } from "elysia";
import { auth } from "../auth";

export const requireAuth = new Elysia({ name: "requireAuth" }).derive(async ({ request }) => {
	const session = await auth.api.getSession({
		headers: request.headers,
	});
	if (!session?.user) {
		return status(401, { error: "Unauthorized" });
	}
	return { session };
});
