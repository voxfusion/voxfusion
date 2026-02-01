import { integer, pgTable, text, timestamp } from "drizzle-orm/pg-core";
import { users } from "./users";

export const transcriptions = pgTable("transcriptions", {
	id: text("id").primaryKey(),
	userId: text("user_id")
		.notNull()
		.references(() => users.id, { onDelete: "cascade" }),
	fileUrl: text("file_url").notNull(),
	text: text("text").notNull(),
	wordCount: integer("word_count"),
	processingTimeMs: integer("processing_time_ms").notNull(),
	audioDurationMs: integer("audio_duration_ms"),
	provider: text("provider").notNull(),
	model: text("model").notNull(),
	rating: text("rating"),
	createdAt: timestamp("created_at").notNull().defaultNow(),
});
