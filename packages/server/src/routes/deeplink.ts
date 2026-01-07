import { Elysia } from "elysia";

const isDev = process.env.NODE_ENV === "development";

export const deeplinkRoutes = new Elysia().all("/deeplink", async (ctx) => {
	const cookieHeader = ctx.request.headers.get("cookie") || "";
	const sessionToken = cookieHeader
		.split(";")
		.map((c) => c.trim())
		.find((c) => c.startsWith("better-auth.session_token="))
		?.split("=")[1];

	if (!sessionToken) {
		return new Response("No session found. Please try logging in again.", {
			status: 401,
		});
	}

	const deepLinkScheme = isDev ? "voxfusion-dev" : "voxfusion";
	const deepLink = `${deepLinkScheme}://settings?token=${sessionToken}`;

	const devSection = isDev
		? `
		<div class="dev-section">
			<p class="dev-label">Development: If the app doesn't open, copy the token:</p>
			<button type="button" onclick="copyToken()" id="copyBtn" class="btn btn-secondary">
				Copy Token
			</button>
			<p id="success" class="success">
				✓ Token copied to clipboard!
			</p>
		</div>
	`
		: "";

	const copyScript = isDev
		? `
		function copyToken() {
			navigator.clipboard.writeText("${sessionToken}").then(() => {
				document.getElementById('success').style.display = 'block';
				document.getElementById('copyBtn').textContent = 'Copied!';
				setTimeout(() => {
					document.getElementById('success').style.display = 'none';
					document.getElementById('copyBtn').textContent = 'Copy Token';
				}, 2000);
			});
		}
	`
		: "";

	const html = `<!DOCTYPE html>
<html lang="en">
	<head>
		<meta charset="utf-8" />
		<meta name="viewport" content="width=device-width, initial-scale=1" />
		<title>VoxFusion - Authentication</title>
		<style>
			* { box-sizing: border-box; margin: 0; padding: 0; }
			body {
				font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
				min-height: 100vh;
				display: flex;
				align-items: center;
				justify-content: center;
				background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
				padding: 20px;
			}
			.card {
				background: white;
				border-radius: 16px;
				padding: 40px;
				max-width: 420px;
				width: 100%;
				box-shadow: 0 25px 50px -12px rgba(0,0,0,0.25);
				text-align: center;
			}
			h1 { color: #1e293b; margin-bottom: 8px; font-size: 24px; }
			.subtitle { color: #64748b; margin-bottom: 24px; }
			.btn {
				display: inline-block;
				padding: 14px 28px;
				border-radius: 10px;
				font-size: 16px;
				font-weight: 600;
				text-decoration: none;
				cursor: pointer;
				border: none;
				transition: all 0.2s;
				width: 100%;
				margin-bottom: 12px;
			}
			.btn-primary {
				background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
				color: white;
			}
			.btn-primary:hover { transform: translateY(-2px); box-shadow: 0 10px 20px rgba(102,126,234,0.3); }
			.btn-secondary {
				background: #f1f5f9;
				color: #475569;
			}
			.btn-secondary:hover { background: #e2e8f0; }
			.dev-section { margin-top: 24px; padding-top: 24px; border-top: 1px solid #e2e8f0; }
			.dev-label { font-size: 12px; color: #94a3b8; margin-bottom: 8px; }
			.success { color: #059669; display: none; margin-top: 12px; }
		</style>
		<script>
			setTimeout(() => { window.location.href = "${deepLink}"; }, 500);
			${copyScript}
		</script>
	</head>
	<body>
		<div class="card">
			<h1>✨ Authentication Successful</h1>
			<p class="subtitle">Opening VoxFusion app...</p>
			<a href="${deepLink}" class="btn btn-primary">
				Open VoxFusion App
			</a>
			${devSection}
		</div>
	</body>
</html>`;

	return new Response(html, {
		headers: { "Content-Type": "text/html" },
	});
});
