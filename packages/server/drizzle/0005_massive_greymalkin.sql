CREATE TABLE "dictionary_words" (
	"id" text PRIMARY KEY NOT NULL,
	"user_id" text NOT NULL,
	"word" text NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "dictionary_words" ADD CONSTRAINT "dictionary_words_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;