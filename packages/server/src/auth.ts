import { betterAuth } from "better-auth";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import { db } from "./providers/db";
import * as schema from "./providers/db/schema";

const betterAuthSchema = {
	user: schema.users,
	session: schema.sessions,
	account: schema.accounts,
	verification: schema.verifications,
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
		expiresIn: 60 * 60 * 24 * 7, // 7 days
		updateAge: 60 * 60 * 24, // 1 day
	},
	account: {
		skipStateCookieCheck: true,
	},
	advanced: {
		useSecureCookies: true,
		defaultCookieAttributes: {
			sameSite: "none",
			secure: true,
		}
	}
});
