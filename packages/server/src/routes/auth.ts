import { Elysia } from "elysia";
import { auth } from "../auth";

export const authRoutes = new Elysia({ prefix: "/auth" }).all("/*", async (ctx) => {
	return auth.handler(ctx.request);
});
