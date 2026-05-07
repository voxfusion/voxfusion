import { cors } from "@elysiajs/cors";
import { Elysia } from "elysia";
import { BunAdapter } from "elysia/adapter/bun";
import { auth } from "./auth";
import { deeplinkRoutes } from "./routes/deeplink";
import { dictionaryRoutes } from "./routes/dictionary";
import { transcribeRoutes } from "./routes/transcribe";

const withCredentialCors = (response: Response, request: Request) => {
	const origin = request.headers.get("Origin");
	if (!origin) return response;

	const headers = new Headers(response.headers);
	headers.set("Access-Control-Allow-Origin", origin);
	headers.set("Access-Control-Allow-Credentials", "true");
	headers.append("Vary", "Origin");

	return new Response(response.body, {
		status: response.status,
		statusText: response.statusText,
		headers,
	});
};

const authPreflight = (request: Request) =>
	withCredentialCors(
		new Response(null, {
			status: 204,
			headers: {
				"Access-Control-Allow-Headers":
					request.headers.get("Access-Control-Request-Headers") ?? "Content-Type, Authorization",
				"Access-Control-Allow-Methods": "GET, POST, PUT, PATCH, DELETE, OPTIONS",
				"Access-Control-Max-Age": "86400",
			},
		}),
		request
	);

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
	.get("/health", () => ({ status: "ok" }));

Bun.serve({
	port: 3000,
	fetch: async (request) => {
		const url = new URL(request.url);

		if (url.pathname === "/api/auth" || url.pathname.startsWith("/api/auth/")) {
			if (request.method === "OPTIONS") return authPreflight(request);

			return withCredentialCors(await auth.handler(request), request);
		}

		return app.fetch(request);
	},
});

export type App = typeof app;
