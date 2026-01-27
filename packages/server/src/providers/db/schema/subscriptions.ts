import { boolean, pgTable, text, timestamp } from "drizzle-orm/pg-core";
import { users } from "./users";

export const subscriptions = pgTable("subscriptions", {
	id: text("id").primaryKey(),
	userId: text("user_id")
		.notNull()
		.references(() => users.id, { onDelete: "cascade" })
		.unique(), // One subscription per user
	yookassaPaymentMethodId: text("yookassa_payment_method_id"), // Saved for recurring payments
	yookassaLastPaymentId: text("yookassa_last_payment_id"), // Last successful payment
	plan: text("plan").notNull().default("free"), // "free" | "pro"
	status: text("status").notNull().default("active"), // "active" | "canceled" | "past_due"
	currentPeriodStart: timestamp("current_period_start"),
	currentPeriodEnd: timestamp("current_period_end"),
	cancelAtPeriodEnd: boolean("cancel_at_period_end").default(false),
	createdAt: timestamp("created_at").notNull().defaultNow(),
	updatedAt: timestamp("updated_at").notNull().defaultNow(),
});
