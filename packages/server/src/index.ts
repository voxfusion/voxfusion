import { cors } from "@elysiajs/cors";
import { Elysia } from "elysia";
import { authRoutes } from "./routes/auth";

const app = new Elysia()
	.use(cors())
	.get("/", () => ({
		name: "VoxFusion API",
		version: "0.1.0",
		status: "healthy",
	}))
	.get("/health", () => ({ status: "ok" }))
	.use(authRoutes)
	.listen(3000);

console.log(`🦊 VoxFusion server running at ${app.server?.hostname}:${app.server?.port}`);

export type App = typeof app;
