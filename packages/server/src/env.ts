function requireEnv(name: string): string {
	const value = process.env[name];
	if (!value) {
		throw new Error(`Missing required environment variable: ${name}`);
	}
	return value;
}

export const env = {
	DATABASE_URL: requireEnv("DATABASE_URL"),
	BETTER_AUTH_SECRET: requireEnv("BETTER_AUTH_SECRET"),
	BETTER_AUTH_URL: process.env.BETTER_AUTH_URL || "http://localhost:3000",
	GROQ_API_KEY: requireEnv("GROQ_API_KEY"),
	GOOGLE_CLIENT_ID: requireEnv("GOOGLE_CLIENT_ID"),
	GOOGLE_CLIENT_SECRET: requireEnv("GOOGLE_CLIENT_SECRET"),
	S3_ENDPOINT: requireEnv("S3_ENDPOINT"),
	S3_BUCKET: requireEnv("S3_BUCKET"),
	PORT: Number(process.env.PORT) || 3000,
} as const;
