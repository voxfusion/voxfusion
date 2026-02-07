import { Elysia, status } from "elysia";

const rateLimitStore = new Map<string, { count: number; resetAt: number }>();

const RATE_LIMITS = {
	free: {
		requests: 20,
		windowMs: 60 * 1000,
	},
	pro: {
		requests: 100,
		windowMs: 60 * 1000,
	},
	anonymous: {
		requests: 10,
		windowMs: 60 * 1000,
	},
} as const;

type RateLimitTier = keyof typeof RATE_LIMITS;

function checkRateLimit(
	key: string,
	tier: RateLimitTier
): { allowed: boolean; remaining: number; resetAt: number } {
	const now = Date.now();
	const limit = RATE_LIMITS[tier];
	const record = rateLimitStore.get(key);

	if (!record || record.resetAt <= now) {
		rateLimitStore.set(key, {
			count: 1,
			resetAt: now + limit.windowMs,
		});
		return {
			allowed: true,
			remaining: limit.requests - 1,
			resetAt: now + limit.windowMs,
		};
	}

	record.count++;

	if (record.count > limit.requests) {
		return {
			allowed: false,
			remaining: 0,
			resetAt: record.resetAt,
		};
	}

	return {
		allowed: true,
		remaining: limit.requests - record.count,
		resetAt: record.resetAt,
	};
}

function cleanupExpiredEntries() {
	const now = Date.now();
	for (const [key, record] of rateLimitStore.entries()) {
		if (record.resetAt <= now) {
			rateLimitStore.delete(key);
		}
	}
}

setInterval(cleanupExpiredEntries, 5 * 60 * 1000);

export function createRateLimiter(options: { getTier?: (ctx: any) => RateLimitTier } = {}) {
	return new Elysia({ name: "rateLimit" }).derive(async ({ request, set }) => {
		const forwarded = request.headers.get("x-forwarded-for");
		const ip = forwarded?.split(",")[0]?.trim() || "unknown";

		const tier: RateLimitTier = "anonymous";

		const key = `${tier}:${ip}`;

		const result = checkRateLimit(key, tier);

		set.headers["X-RateLimit-Limit"] = String(RATE_LIMITS[tier].requests);
		set.headers["X-RateLimit-Remaining"] = String(result.remaining);
		set.headers["X-RateLimit-Reset"] = String(Math.ceil(result.resetAt / 1000));

		if (!result.allowed) {
			const retryAfter = Math.ceil((result.resetAt - Date.now()) / 1000);
			set.headers["Retry-After"] = String(retryAfter);
			return status(429, {
				error: "Too many requests",
				message: `Rate limit exceeded. Try again in ${retryAfter} seconds.`,
				retryAfter,
			});
		}

		return { rateLimitKey: key, rateLimitTier: tier };
	});
}

export async function checkUserRateLimit(
	userId: string,
	tier: "free" | "pro"
): Promise<{ allowed: boolean; remaining: number; resetAt: number; retryAfter?: number }> {
	const key = `user:${tier}:${userId}`;
	const result = checkRateLimit(key, tier);

	if (!result.allowed) {
		return {
			...result,
			retryAfter: Math.ceil((result.resetAt - Date.now()) / 1000),
		};
	}

	return result;
}

export function rateLimitCheck(
	userId: string | null,
	ip: string,
	tier: RateLimitTier = "anonymous"
) {
	const key = userId ? `user:${tier}:${userId}` : `ip:${tier}:${ip}`;
	return checkRateLimit(key, tier);
}

export const rateLimitMiddleware = createRateLimiter();
