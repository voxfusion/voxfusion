import { betterAuth } from "better-auth";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import { bearer } from "better-auth/plugins/bearer";
import { jwt } from "better-auth/plugins/jwt";
import { env } from "./env";
import { db } from "./providers/db";
import * as schema from "./providers/db/schema";

const betterAuthSchema = {
	user: schema.users,
	session: schema.sessions,
	account: schema.accounts,
	verification: schema.verifications,
	jwks: schema.jwks,
};

export const auth = betterAuth({
	baseURL: env.BETTER_AUTH_URL,
	basePath: "/api/auth",
	secret: env.BETTER_AUTH_SECRET,
	database: drizzleAdapter(db, {
		provider: "pg",
		schema: betterAuthSchema,
	}),
	socialProviders: {
		google: {
			clientId: env.GOOGLE_CLIENT_ID,
			clientSecret: env.GOOGLE_CLIENT_SECRET,
			scope: ["openid", "email", "profile"],
		},
	},
	session: {
		expiresIn: 60 * 60 * 24 * 400,
		updateAge: 60 * 60 * 24 * 30,
	},
	account: {
		skipStateCookieCheck: true,
	},
	plugins: [
		bearer(),
		jwt({
			jwt: {
				issuer: env.BETTER_AUTH_URL,
				audience: "voxfusion",
				expirationTime: "400d",
			},
		}),
	],
	advanced: {
		disableCSRFCheck: true,
		defaultCookieAttributes: {
			sameSite: "none",
			secure: true,
		},
	},
	trustedOrigins: ["voxfusion://", env.BETTER_AUTH_URL],
});
