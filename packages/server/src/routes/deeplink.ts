import { Elysia } from "elysia";
import deeplinkDevHtml from "../templates/deeplink-dev.html" with { type: "text" };
import deeplinkProductionHtml from "../templates/deeplink-production.html" with { type: "text" };
import { auth } from "../auth";

export const deeplinkRoutes = new Elysia().all("/deeplink", async (ctx) => {
	const session = await auth.api.getSession({
		headers: ctx.request.headers,
	});

	if (!session) {
		return new Response("No session found. Please try logging in again.", {
			status: 401,
		});
	}

	const deepLink = `voxfusion://settings?token=${session.session.token}`;
	const isDev = process.env.NODE_ENV === "development";

	const template = isDev ? deeplinkDevHtml : deeplinkProductionHtml;
	let html = template.replaceAll("{{DEEP_LINK}}", deepLink);
	html = html.replaceAll("{{SESSION_TOKEN}}", session.session.token);

	return new Response(html, {
		headers: { "Content-Type": "text/html" },
	});
});
