import { and, desc, eq } from "drizzle-orm";
import { Elysia, status, t } from "elysia";
import { auth } from "../auth";
import { db } from "../providers/db";
import { dictionaryWords } from "../providers/db/schema";

type Session = {
	user: {
		id: string;
		email: string;
		name?: string | null;
	};
};

export const dictionaryRoutes = new Elysia({ prefix: "/dictionary" })
	.derive(async ({ request }) => {
		const session = await auth.api.getSession({
			headers: request.headers,
		});
		if (!session?.user) {
			return status(401, { error: "Unauthorized" });
		}
		return { session };
	})
	// GET /api/dictionary - List all words for user
	.get("/", async (ctx) => {
		const session = (ctx as any).session as Session;
		const words = await db
			.select()
			.from(dictionaryWords)
			.where(eq(dictionaryWords.userId, session.user.id))
			.orderBy(desc(dictionaryWords.createdAt));

		return { words };
	})
	// POST /api/dictionary - Add new word
	.post(
		"/",
		async (ctx) => {
			const session = (ctx as any).session as Session;
			const { word } = ctx.body;

			const id = crypto.randomUUID();
			const now = new Date();
			await db.insert(dictionaryWords).values({
				id,
				userId: session.user.id,
				word: word.trim(),
				createdAt: now,
				updatedAt: now,
			});

			return { id, word: word.trim() };
		},
		{
			body: t.Object({
				word: t.String({ minLength: 1, maxLength: 100 }),
			}),
		}
	)
	// PATCH /api/dictionary/:id - Update word
	.patch(
		"/:id",
		async (ctx) => {
			const session = (ctx as any).session as Session;
			const { id } = ctx.params;
			const { word } = ctx.body;

			// Verify ownership
			const existing = await db
				.select()
				.from(dictionaryWords)
				.where(and(eq(dictionaryWords.id, id), eq(dictionaryWords.userId, session.user.id)))
				.limit(1);

			if (existing.length === 0) {
				return status(404, { error: "Word not found" });
			}

			await db
				.update(dictionaryWords)
				.set({
					word: word.trim(),
					updatedAt: new Date(),
				})
				.where(eq(dictionaryWords.id, id));

			return { success: true };
		},
		{
			params: t.Object({ id: t.String() }),
			body: t.Object({
				word: t.String({ minLength: 1, maxLength: 100 }),
			}),
		}
	)
	// DELETE /api/dictionary/:id - Delete word
	.delete(
		"/:id",
		async (ctx) => {
			const session = (ctx as any).session as Session;
			const { id } = ctx.params;

			await db
				.delete(dictionaryWords)
				.where(and(eq(dictionaryWords.id, id), eq(dictionaryWords.userId, session.user.id)));

			return { success: true };
		},
		{
			params: t.Object({ id: t.String() }),
		}
	);
