import { cors } from "@elysiajs/cors";
import { Elysia } from "elysia";
import { BunAdapter } from "elysia/adapter/bun";
import { auth } from "./auth";
import { env } from "./env";
import { deeplinkRoutes } from "./routes/deeplink";
import { dictionaryRoutes } from "./routes/dictionary";
import { transcribeRoutes } from "./routes/transcribe";

const app = new Elysia({ prefix: "/api", adapter: BunAdapter })
	.use(
		cors({
			origin: true,
			credentials: true,
		})
	)
	.onError(({ error, code }) => {
		if (code === "NOT_FOUND") {
			return { error: "Not found" };
		}
		console.error(`Unhandled error [${code}]:`, error);
		return { error: "Internal server error" };
	})
	.mount(auth.handler)
	.use(transcribeRoutes)
	.use(dictionaryRoutes)
	.use(deeplinkRoutes)
	.get("/", () => ({
		name: "VoxFusion API",
		version: "0.1.0",
		status: "healthy",
	}))
	.get("/health", () => ({ status: "ok" }))
	.listen(env.PORT);

console.log(`🦊 VoxFusion server running at ${app.server?.hostname}:${app.server?.port}`);

export type App = typeof app;
