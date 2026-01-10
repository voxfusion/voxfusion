import { pgTable, text, timestamp } from "drizzle-orm/pg-core";
import { users } from "./users";

export const dictionaryWords = pgTable("dictionary_words", {
	id: text("id").primaryKey(),
	userId: text("user_id")
		.notNull()
		.references(() => users.id, { onDelete: "cascade" }),
	word: text("word").notNull(),
	createdAt: timestamp("created_at").notNull().defaultNow(),
	updatedAt: timestamp("updated_at").notNull().defaultNow(),
});
