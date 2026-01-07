import { cors } from "@elysiajs/cors";
import { Elysia } from "elysia";
import { transcribeRoutes } from "./routes/transcribe";
import { deeplinkRoutes } from "./routes/deeplink";
import { BunAdapter } from "elysia/adapter/bun";
import { auth } from "./auth";

const app = new Elysia({ prefix: "/api", adapter: BunAdapter })
	.use(
		cors({
			origin: true,
			credentials: true,
		})
	)
	.mount(auth.handler)
	.use(transcribeRoutes)
	.use(deeplinkRoutes)
	.get("/", () => ({
		name: "VoxFusion API",
		version: "0.1.0",
		status: "healthy",
	}))
	.get("/health", () => ({ status: "ok" }))
	.listen(3000);

console.log(`🦊 VoxFusion server running at ${app.server?.hostname}:${app.server?.port}`);

export type App = typeof app;
