import { cors } from "@elysiajs/cors";
import { Elysia } from "elysia";
import { authRoutes } from "./routes/auth";
import { Groq } from "groq-sdk";
import { transcribeRoutes } from "./routes/transcribe";
import { BunAdapter } from "elysia/adapter/bun";
import { createAuthClient } from "better-auth/client";

const app = new Elysia({ prefix: "/api", adapter: BunAdapter })
	.use(cors({
		origin: true,
		credentials: true,
	}))
	.decorate("groq", new Groq({ apiKey: process.env.GROQ_API_KEY! }))
	.use(transcribeRoutes)
	.get("/", () => ({
		name: "VoxFusion API",
		version: "0.1.0",
		status: "healthy",
	}))
	.get("/google-auth-callback", async () => {
		const authClient = createAuthClient()
		const data = await authClient.signIn.social({
			provider: "google",
		})
		
		return data
	})
	.get("/health", () => ({ status: "ok" }))
	.use(authRoutes)
	.listen(3000);

console.log(`🦊 VoxFusion server running at ${app.server?.hostname}:${app.server?.port}`);

export type App = typeof app;
