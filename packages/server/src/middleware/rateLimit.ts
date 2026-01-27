import { Elysia, status } from "elysia";

// Simple in-memory rate limiter
// For production, consider using Redis for distributed rate limiting
const rateLimitStore = new Map<string, { count: number; resetAt: number }>();

// Configuration for different rate limit tiers
const RATE_LIMITS = {
	free: {
		requests: 20, // requests per window
		windowMs: 60 * 1000, // 1 minute
	},
	pro: {
		requests: 100, // requests per window
		windowMs: 60 * 1000, // 1 minute
	},
	// Global limit for unauthenticated requests
	anonymous: {
		requests: 10,
		windowMs: 60 * 1000,
	},
} as const;

type RateLimitTier = keyof typeof RATE_LIMITS;

/**
 * Check if request should be rate limited
 */
function checkRateLimit(
	key: string,
	tier: RateLimitTier
): { allowed: boolean; remaining: number; resetAt: number } {
	const now = Date.now();
	const limit = RATE_LIMITS[tier];
	const record = rateLimitStore.get(key);

	// If no record or expired, create new
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

	// Increment count
	record.count++;

	// Check if over limit
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

/**
 * Clean up expired entries periodically
 */
function cleanupExpiredEntries() {
	const now = Date.now();
	for (const [key, record] of rateLimitStore.entries()) {
		if (record.resetAt <= now) {
			rateLimitStore.delete(key);
		}
	}
}

// Run cleanup every 5 minutes
setInterval(cleanupExpiredEntries, 5 * 60 * 1000);

/**
 * Rate limiting middleware for Elysia
 * Usage: .use(rateLimit({ tier: 'free' })) or use the derive pattern
 */
export function createRateLimiter(options: { getTier?: (ctx: any) => RateLimitTier } = {}) {
	return new Elysia({ name: "rateLimit" }).derive(async ({ request, set }) => {
		// Get client identifier (IP or user ID)
		const forwarded = request.headers.get("x-forwarded-for");
		const ip = forwarded?.split(",")[0]?.trim() || "unknown";

		// Default to anonymous tier
		const tier: RateLimitTier = "anonymous";

		// Create rate limit key
		const key = `${tier}:${ip}`;

		const result = checkRateLimit(key, tier);

		// Set rate limit headers
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

/**
 * Check rate limit for authenticated user with specific tier
 */
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

/**
 * Simple rate limit check for use in routes
 */
export function rateLimitCheck(
	userId: string | null,
	ip: string,
	tier: RateLimitTier = "anonymous"
) {
	const key = userId ? `user:${tier}:${userId}` : `ip:${tier}:${ip}`;
	return checkRateLimit(key, tier);
}

export const rateLimitMiddleware = createRateLimiter();
