import { betterAuth } from "better-auth";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import { bearer } from "better-auth/plugins/bearer";
import { jwt } from "better-auth/plugins/jwt";
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
	baseURL: process.env.BETTER_AUTH_URL || "http://localhost:3000",
	basePath: "/api/auth",
	secret: process.env.BETTER_AUTH_SECRET!,
	database: drizzleAdapter(db, {
		provider: "pg",
		schema: betterAuthSchema,
	}),
	socialProviders: {
		google: {
			clientId: process.env.GOOGLE_CLIENT_ID!,
			clientSecret: process.env.GOOGLE_CLIENT_SECRET!,
			scope: ["openid", "email", "profile"],
		},
	},
	session: {
		// Maximum allowed cookie duration: 400 days (34,560,000 seconds)
		expiresIn: 60 * 60 * 24 * 400,
		// Refresh window: 30 days - session expiration extends by 30 days on each access
		updateAge: 60 * 60 * 24 * 30,
	},
	account: {
		skipStateCookieCheck: true,
	},
	plugins: [
		bearer(),
		jwt({
			jwt: {
				issuer: process.env.BETTER_AUTH_URL || "http://localhost:3000",
				audience: "voxfusion",
				// Match session expiration: 400 days (max cookie duration)
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
	trustedOrigins: ["voxfusion://"],
});
