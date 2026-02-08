import { cors } from "@elysiajs/cors";
import { Elysia } from "elysia";
import { BunAdapter } from "elysia/adapter/bun";
import { auth } from "./auth";
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
	.listen(3000);

export type App = typeof app;
