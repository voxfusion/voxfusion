import { Elysia } from "elysia";
import deeplinkDevHtml from "../templates/deeplink-dev.html" with { type: "text" };
import deeplinkProductionHtml from "../templates/deeplink-production.html" with { type: "text" };

export const deeplinkRoutes = new Elysia().all("/deeplink", async (ctx) => {
	const cookieHeader = ctx.request.headers.get("cookie") || "";
	const sessionToken = cookieHeader
		.split(";")
		.map((c) => c.trim())
		.find((c) => c.startsWith("better-auth.session_token=") || c.startsWith("__Secure-better-auth.session_token="))
		?.split("=")[1];

	if (!sessionToken) {
		return new Response("No session found. Please try logging in again.", {
			status: 401,
		});
	}

	const deepLink = `voxfusion://settings?token=${sessionToken}`;
	const isDev = process.env.NODE_ENV === "development";

	const template = isDev ? deeplinkDevHtml : deeplinkProductionHtml;
	let html = template.replaceAll("{{DEEP_LINK}}", deepLink);
	html = html.replaceAll("{{SESSION_TOKEN}}", sessionToken);

	return new Response(html, {
		headers: { "Content-Type": "text/html" },
	});
});
