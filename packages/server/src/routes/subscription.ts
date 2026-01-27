import { and, eq, gte, lte, sql } from "drizzle-orm";
import { Elysia, status, t } from "elysia";
import { auth } from "../auth";
import { db } from "../providers/db";
import { subscriptions, transcriptions } from "../providers/db/schema";
import { type YooKassaWebhookPayload, yookassa } from "../providers/yookassa";

type Session = {
	user: {
		id: string;
		email: string;
		name?: string | null;
	};
};

// Constants for tier limits
const FREE_TIER_WORD_LIMIT = 25000; // 25K words per month
const PRO_TIER_WORD_LIMIT = Number.POSITIVE_INFINITY; // Unlimited for Pro
const SUBSCRIPTION_PRICE_RUB = "800.00"; // 800 RUB per month

/**
 * Get user's subscription plan
 */
export async function getUserSubscription(userId: string) {
	const result = await db
		.select()
		.from(subscriptions)
		.where(eq(subscriptions.userId, userId))
		.limit(1);

	if (result.length === 0) {
		// No subscription record = free tier
		return {
			plan: "free" as const,
			status: "active" as const,
			wordLimit: FREE_TIER_WORD_LIMIT,
		};
	}

	const sub = result[0]!;

	// Check if subscription is active
	const isActive =
		sub.status === "active" &&
		(!sub.currentPeriodEnd || new Date(sub.currentPeriodEnd) > new Date());

	return {
		plan: isActive ? (sub.plan as "free" | "pro") : ("free" as const),
		status: sub.status as "active" | "canceled" | "past_due",
		wordLimit: isActive && sub.plan === "pro" ? PRO_TIER_WORD_LIMIT : FREE_TIER_WORD_LIMIT,
		currentPeriodEnd: sub.currentPeriodEnd,
		cancelAtPeriodEnd: sub.cancelAtPeriodEnd,
		yookassaPaymentMethodId: sub.yookassaPaymentMethodId,
	};
}

/**
 * Get user's word usage for current month
 */
export async function getMonthlyWordUsage(userId: string): Promise<number> {
	const startOfMonth = new Date();
	startOfMonth.setDate(1);
	startOfMonth.setHours(0, 0, 0, 0);

	const result = await db
		.select({
			totalWords: sql<number>`COALESCE(SUM(${transcriptions.wordCount}), 0)::integer`,
		})
		.from(transcriptions)
		.where(and(eq(transcriptions.userId, userId), gte(transcriptions.createdAt, startOfMonth)));

	return result[0]?.totalWords ?? 0;
}

/**
 * Check if user can transcribe more words
 */
export async function canUserTranscribe(userId: string): Promise<{
	allowed: boolean;
	remaining: number;
	used: number;
	limit: number;
	plan: "free" | "pro";
}> {
	const subscription = await getUserSubscription(userId);
	const used = await getMonthlyWordUsage(userId);

	const remaining =
		subscription.wordLimit === Number.POSITIVE_INFINITY
			? Number.POSITIVE_INFINITY
			: Math.max(0, subscription.wordLimit - used);

	return {
		allowed: remaining > 0,
		remaining,
		used,
		limit: subscription.wordLimit,
		plan: subscription.plan,
	};
}

/**
 * Activate subscription for a user after successful payment
 */
async function activateSubscription(
	userId: string,
	paymentMethodId: string | null,
	paymentId: string
) {
	const now = new Date();
	const periodEnd = new Date(now);
	periodEnd.setMonth(periodEnd.getMonth() + 1); // 1 month subscription

	await db
		.insert(subscriptions)
		.values({
			id: crypto.randomUUID(),
			userId,
			yookassaPaymentMethodId: paymentMethodId,
			yookassaLastPaymentId: paymentId,
			plan: "pro",
			status: "active",
			currentPeriodStart: now,
			currentPeriodEnd: periodEnd,
			cancelAtPeriodEnd: false,
		})
		.onConflictDoUpdate({
			target: subscriptions.userId,
			set: {
				yookassaPaymentMethodId: paymentMethodId,
				yookassaLastPaymentId: paymentId,
				plan: "pro",
				status: "active",
				currentPeriodStart: now,
				currentPeriodEnd: periodEnd,
				cancelAtPeriodEnd: false,
				updatedAt: new Date(),
			},
		});
}

export const subscriptionRoutes = new Elysia({ prefix: "/subscription" })
	.derive(async ({ request }) => {
		const session = await auth.api.getSession({
			headers: request.headers,
		});
		if (!session?.user) {
			return status(401, { error: "Unauthorized" });
		}
		return { session };
	})
	// Get current subscription status
	.get("/", async (ctx) => {
		const session = (ctx as any).session as Session | undefined;

		if (!session?.user) {
			return status(401, { error: "Unauthorized" });
		}

		const subscription = await getUserSubscription(session.user.id);
		const usage = await canUserTranscribe(session.user.id);

		return {
			plan: subscription.plan,
			status: subscription.status,
			wordLimit:
				subscription.wordLimit === Number.POSITIVE_INFINITY ? null : subscription.wordLimit,
			wordsUsed: usage.used,
			wordsRemaining: usage.remaining === Number.POSITIVE_INFINITY ? null : usage.remaining,
			currentPeriodEnd: subscription.currentPeriodEnd,
			cancelAtPeriodEnd: subscription.cancelAtPeriodEnd,
		};
	})
	// Get usage statistics
	.get("/usage", async (ctx) => {
		const session = (ctx as any).session as Session | undefined;

		if (!session?.user) {
			return status(401, { error: "Unauthorized" });
		}

		const usage = await canUserTranscribe(session.user.id);

		return {
			plan: usage.plan,
			wordsUsed: usage.used,
			wordLimit: usage.limit === Number.POSITIVE_INFINITY ? null : usage.limit,
			wordsRemaining: usage.remaining === Number.POSITIVE_INFINITY ? null : usage.remaining,
			percentUsed:
				usage.limit === Number.POSITIVE_INFINITY ? 0 : Math.round((usage.used / usage.limit) * 100),
			canTranscribe: usage.allowed,
		};
	})
	// Create checkout session for Pro subscription
	.post(
		"/checkout",
		async (ctx) => {
			const session = (ctx as any).session as Session | undefined;

			if (!session?.user) {
				return status(401, { error: "Unauthorized" });
			}

			const { returnUrl } = ctx.body as { returnUrl: string };

			try {
				// Check if user already has an active subscription
				const existingSub = await getUserSubscription(session.user.id);
				if (existingSub.plan === "pro" && existingSub.status === "active") {
					return status(400, { error: "Already subscribed to Pro plan" });
				}

				// Create payment with YooKassa
				const payment = await yookassa.createSubscriptionPayment(session.user.id, returnUrl);

				if (!payment.confirmation?.confirmation_url) {
					console.error("No confirmation URL in payment response:", payment);
					return status(500, { error: "Failed to create payment" });
				}

				return {
					paymentId: payment.id,
					confirmationUrl: payment.confirmation.confirmation_url,
				};
			} catch (error) {
				console.error("Checkout error:", error);
				return status(500, { error: "Failed to create checkout session" });
			}
		},
		{
			body: t.Object({
				returnUrl: t.String(),
			}),
		}
	)
	// Cancel subscription (will remain active until period end)
	.post("/cancel", async (ctx) => {
		const session = (ctx as any).session as Session | undefined;

		if (!session?.user) {
			return status(401, { error: "Unauthorized" });
		}

		try {
			const result = await db
				.update(subscriptions)
				.set({
					cancelAtPeriodEnd: true,
					updatedAt: new Date(),
				})
				.where(eq(subscriptions.userId, session.user.id))
				.returning();

			if (result.length === 0) {
				return status(404, { error: "No subscription found" });
			}

			return {
				success: true,
				message: "Subscription will be canceled at the end of the current period",
				currentPeriodEnd: result[0]!.currentPeriodEnd,
			};
		} catch (error) {
			console.error("Cancel subscription error:", error);
			return status(500, { error: "Failed to cancel subscription" });
		}
	})
	// Reactivate a canceled subscription
	.post("/reactivate", async (ctx) => {
		const session = (ctx as any).session as Session | undefined;

		if (!session?.user) {
			return status(401, { error: "Unauthorized" });
		}

		try {
			const result = await db
				.update(subscriptions)
				.set({
					cancelAtPeriodEnd: false,
					updatedAt: new Date(),
				})
				.where(
					and(eq(subscriptions.userId, session.user.id), eq(subscriptions.cancelAtPeriodEnd, true))
				)
				.returning();

			if (result.length === 0) {
				return status(404, { error: "No canceled subscription found" });
			}

			return {
				success: true,
				message: "Subscription reactivated",
			};
		} catch (error) {
			console.error("Reactivate subscription error:", error);
			return status(500, { error: "Failed to reactivate subscription" });
		}
	});

// YooKassa webhook handler - handles payment events
export const yookassaWebhookRoutes = new Elysia({ prefix: "/yookassa" }).post(
	"/webhooks",
	async ({ body, request }) => {
		// YooKassa sends notifications to this endpoint
		// In production, verify IP is from YooKassa: https://yookassa.ru/developers/using-api/webhooks#ip
		const payload = body as YooKassaWebhookPayload;

		console.log("YooKassa webhook received:", payload.event, payload.object?.id);

		try {
			const payment = payload.object;

			if (!payment || !payment.metadata?.user_id) {
				console.error("No user_id in payment metadata");
				return { status: "error", message: "Missing user_id" };
			}

			const userId = payment.metadata.user_id;

			switch (payload.event) {
				case "payment.succeeded": {
					// Payment was successful
					console.log("Payment succeeded for user:", userId);

					// Get the saved payment method ID for recurring billing
					const paymentMethodId = payment.payment_method?.saved ? payment.payment_method.id : null;

					// Activate the subscription
					await activateSubscription(userId, paymentMethodId, payment.id);

					console.log("Subscription activated for user:", userId);
					break;
				}

				case "payment.canceled": {
					// Payment was canceled/failed
					console.log("Payment canceled for user:", userId);

					// If this was a renewal attempt, mark subscription as past_due
					if (payment.metadata.type === "subscription_renewal") {
						await db
							.update(subscriptions)
							.set({
								status: "past_due",
								updatedAt: new Date(),
							})
							.where(eq(subscriptions.userId, userId));
					}
					break;
				}

				case "payment.waiting_for_capture": {
					// For two-stage payments (we use auto-capture, so this shouldn't happen)
					console.log("Payment waiting for capture:", payment.id);
					break;
				}

				case "refund.succeeded": {
					// Refund was processed - cancel subscription
					console.log("Refund processed for user:", userId);

					await db
						.update(subscriptions)
						.set({
							status: "canceled",
							plan: "free",
							updatedAt: new Date(),
						})
						.where(eq(subscriptions.userId, userId));
					break;
				}

				default:
					console.log("Unhandled webhook event:", payload.event);
			}

			return { status: "ok" };
		} catch (error) {
			console.error("Webhook processing error:", error);
			return { status: "error", message: String(error) };
		}
	}
);

/**
 * Renew expiring subscriptions
 * This should be called by a cron job daily
 */
export async function renewExpiringSubscriptions() {
	const now = new Date();
	const tomorrow = new Date(now);
	tomorrow.setDate(tomorrow.getDate() + 1);

	// Find subscriptions expiring in the next 24 hours that are not canceled
	const expiring = await db
		.select()
		.from(subscriptions)
		.where(
			and(
				eq(subscriptions.status, "active"),
				eq(subscriptions.plan, "pro"),
				eq(subscriptions.cancelAtPeriodEnd, false),
				lte(subscriptions.currentPeriodEnd, tomorrow),
				gte(subscriptions.currentPeriodEnd, now)
			)
		);

	console.log(`Found ${expiring.length} subscriptions to renew`);

	const results = {
		renewed: 0,
		failed: 0,
		skipped: 0,
	};

	for (const sub of expiring) {
		if (!sub.yookassaPaymentMethodId) {
			console.log(`No payment method for user ${sub.userId}, skipping`);
			results.skipped++;
			continue;
		}

		try {
			// Attempt to charge using saved payment method
			const payment = await yookassa.renewSubscription(sub.userId, sub.yookassaPaymentMethodId);

			if (payment.status === "succeeded") {
				// Extend subscription period
				const newPeriodEnd = new Date(sub.currentPeriodEnd!);
				newPeriodEnd.setMonth(newPeriodEnd.getMonth() + 1);

				await db
					.update(subscriptions)
					.set({
						currentPeriodStart: sub.currentPeriodEnd,
						currentPeriodEnd: newPeriodEnd,
						yookassaLastPaymentId: payment.id,
						updatedAt: new Date(),
					})
					.where(eq(subscriptions.id, sub.id));

				console.log(`Renewed subscription for user ${sub.userId}`);
				results.renewed++;
			} else {
				console.log(`Payment status ${payment.status} for user ${sub.userId}`);
				results.failed++;
			}
		} catch (error) {
			console.error(`Failed to renew subscription for user ${sub.userId}:`, error);

			// Mark as past_due
			await db
				.update(subscriptions)
				.set({
					status: "past_due",
					updatedAt: new Date(),
				})
				.where(eq(subscriptions.id, sub.id));

			results.failed++;
		}
	}

	return results;
}

// Admin endpoint to trigger renewal manually (protected by secret)
export const subscriptionAdminRoutes = new Elysia({ prefix: "/admin/subscription" }).post(
	"/renew",
	async ({ headers }) => {
		// Simple secret-based auth for admin endpoints
		const adminSecret = headers["x-admin-secret"];
		if (adminSecret !== process.env.ADMIN_SECRET) {
			return status(401, { error: "Unauthorized" });
		}

		const results = await renewExpiringSubscriptions();
		return results;
	}
);
