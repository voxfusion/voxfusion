import { tmpdir } from "node:os";
import { join } from "node:path";
import { and, desc, eq, gte, lt, sql } from "drizzle-orm";
import { Elysia, status, t } from "elysia";
import {
	MONTHLY_TRANSCRIPTION_WORD_LIMITS,
	type SubscriptionPlan,
} from "@voxfusion/shared/subscriptionPlans";
import { auth } from "../auth";
import { logger as rootLogger } from "../logger";
import { db } from "../providers/db";
import { dictionaryWords, subscriptions, transcriptions } from "../providers/db/schema";
import { groq } from "../providers/groq";

const log = rootLogger.child({ module: "transcribe" });

async function getAudioDuration(buffer: ArrayBuffer): Promise<number | null> {
	const tempFile = join(tmpdir(), `audio-${crypto.randomUUID()}.webm`);
	try {
		await Bun.write(tempFile, buffer);

		const proc = Bun.spawn(
			["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", tempFile],
			{ stdout: "pipe", stderr: "pipe" }
		);
		const output = await new Response(proc.stdout).text();
		await proc.exited;

		const duration = Number.parseFloat(output.trim());
		if (!Number.isNaN(duration)) {
			return Math.round(duration * 1000);
		}
		return null;
	} catch {
		return null;
	} finally {
		(await Bun.file(tempFile).exists()) && (await Bun.$`rm ${tempFile}`);
	}
}

async function getUserPlan(userId: string): Promise<SubscriptionPlan> {
	const result = await db
		.select({
			plan: subscriptions.plan,
			status: subscriptions.status,
			currentPeriodEnd: subscriptions.currentPeriodEnd,
		})
		.from(subscriptions)
		.where(and(eq(subscriptions.userId, userId), eq(subscriptions.status, "active")))
		.limit(1);
	const sub = result[0];
	if (sub?.plan === "pro" && sub.currentPeriodEnd && sub.currentPeriodEnd > new Date()) {
		return "pro";
	}
	return "free";
}

function getMonthStart(): Date {
	const now = new Date();
	return new Date(now.getFullYear(), now.getMonth(), 1);
}

async function getMonthlyWordCount(userId: string): Promise<number> {
	const result = await db
		.select({ total: sql<number>`coalesce(sum(${transcriptions.wordCount}), 0)` })
		.from(transcriptions)
		.where(and(eq(transcriptions.userId, userId), gte(transcriptions.createdAt, getMonthStart())));
	return Number(result[0]?.total ?? 0);
}

function countWords(text: string): number {
	const words = text
		.trim()
		.split(/\s+/)
		.filter((word) => word.length > 0);
	return words.length;
}

type TranscriptionResult = {
	text: string;
	wordCount?: number;
	usage?: {
		wordsUsed: number;
		wordsRemaining: number | null;
		plan: "free" | "pro";
	};
};

type Session = {
	user: {
		id: string;
		email: string;
		name?: string | null;
	};
};

type TranscriptionMetadata = {
	text: string;
	wordCount: number;
	processingTimeMs: number;
	fileBuffer: ArrayBuffer;
};

export const transcribeRoutes = new Elysia({ prefix: "/transcribe" })
	.derive(async ({ request }) => {
		log.info({ method: request.method, url: request.url }, "incoming request");
		const session = await auth.api.getSession({
			headers: request.headers,
		});
		if (!session?.user) {
			log.warn("unauthorized request — no valid session");
			return status(401, { error: "Unauthorized" });
		}
		log.info({ userId: session.user.id }, "authenticated user");
		return {
			session,
			transcriptionMetadata: null as TranscriptionMetadata | null,
		};
	})
	.post(
		"/",
		async (ctx) => {
			const file = ctx.body.file;
			const session = (ctx as any).session as Session;

			log.info({ userId: session.user.id, fileName: file.name, fileSize: file.size }, "POST /transcribe");

			try {
				const [wordsUsedBefore, plan] = await Promise.all([
					getMonthlyWordCount(session.user.id),
					getUserPlan(session.user.id),
				]);
				log.info({ plan, wordsUsedBefore }, "fetched user plan and usage");

			if (plan === "free" && wordsUsedBefore >= MONTHLY_TRANSCRIPTION_WORD_LIMITS[plan]) {
				log.warn({ userId: session.user.id, wordsUsedBefore }, "monthly limit reached");
				return status(403, {
					error: "Monthly transcription limit reached",
					usage: {
						wordsUsed: wordsUsedBefore,
						wordLimit: MONTHLY_TRANSCRIPTION_WORD_LIMITS[plan],
					},
				});
				}

				const fileBuffer = await file.arrayBuffer();
				log.info({ bufferSize: fileBuffer.byteLength }, "read file buffer");

				const userWords = await db
					.select({ word: dictionaryWords.word })
					.from(dictionaryWords)
					.where(eq(dictionaryWords.userId, session.user.id))
					.orderBy(desc(dictionaryWords.updatedAt))
					.limit(50);

				let prompt = "";
				if (userWords.length > 0) {
					const wordList = userWords.map((w) => w.word).join(", ");
					prompt += ` Specialized terms: ${wordList}.`;
				}
				log.info({ dictionaryWords: userWords.length, promptLength: prompt.length }, "built prompt");

				const startTime = performance.now();
				log.info("sending to Groq whisper-large-v3");

				const transcription = await groq.audio.transcriptions.create({
					model: "whisper-large-v3",
					file: new File([fileBuffer], "recording.webm", {
						type: "audio/webm",
					}),
					prompt,
				});

				const endTime = performance.now();
				const processingTimeMs = Math.round(endTime - startTime);
				log.info({ processingTimeMs, textLength: transcription.text.length }, "groq response received");

				const transcriptionText = transcription.text.trim();
				const wordCount = countWords(transcriptionText);

				(ctx as any).transcriptionMetadata = {
					text: transcriptionText,
					wordCount,
					processingTimeMs,
					fileBuffer,
				};

				const wordsUsedAfter = wordsUsedBefore + wordCount;
				log.info({ wordCount, wordsUsedAfter }, "transcription succeeded");
				return {
					text: transcriptionText,
					wordCount,
					usage: {
						wordsUsed: wordsUsedAfter,
						wordsRemaining:
							plan === "pro"
								? null
								: Math.max(0, MONTHLY_TRANSCRIPTION_WORD_LIMITS[plan] - wordsUsedAfter),
						plan,
					},
				} satisfies TranscriptionResult;
			} catch (error) {
				log.error({ err: error }, "transcription failed");
				return status(500, {
					error: "Transcription failed",
					details: error instanceof Error ? error.message : error,
				});
			}
		},
		{
			body: t.Object({
				file: t.File(),
			}),
			afterResponse: async (ctx) => {
				const session = (ctx as any).session as Session | undefined;
				const metadata = (ctx as any).transcriptionMetadata as TranscriptionMetadata | null;

				if (!session || !metadata) {
					log.debug("afterResponse skipped — no session or metadata");
					return;
				}

				try {
					const audioDurationMs = await getAudioDuration(metadata.fileBuffer);
					log.info({ audioDurationMs }, "computed audio duration");

					const fileName = `${crypto.randomUUID()}.webm`;
					const s3Path = `uploads/recordings/${fileName}`;
					await Bun.s3.write(s3Path, new Blob([metadata.fileBuffer], { type: "audio/webm" }), {
						acl: "public-read",
					});
					log.info({ s3Path }, "uploaded recording to S3");

					const fileUrl = `${process.env.S3_ENDPOINT}/${process.env.S3_BUCKET}/${s3Path}`;

					const transcriptionId = crypto.randomUUID();
					await db.insert(transcriptions).values({
						id: transcriptionId,
						userId: session.user.id,
						fileUrl,
						text: metadata.text,
						wordCount: metadata.wordCount,
						processingTimeMs: metadata.processingTimeMs,
						audioDurationMs,
						provider: "groq",
						model: "whisper-large-v3-turbo",
					});
					log.info({ transcriptionId, userId: session.user.id }, "saved transcription to DB");
				} catch (error) {
					log.error({ err: error }, "afterResponse failed — could not save transcription");
				}
			},
		}
	)
	.get("/usage", async (ctx) => {
		const session = (ctx as any).session as Session;
		const [wordsUsed, plan] = await Promise.all([
			getMonthlyWordCount(session.user.id),
			getUserPlan(session.user.id),
		]);
		return {
			wordsUsed,
			wordLimit: MONTHLY_TRANSCRIPTION_WORD_LIMITS[plan],
			plan,
		};
	})
	.get(
		"/",
		async (ctx) => {
			const session = (ctx as any).session as Session | undefined;

			if (!session?.user) {
				return {
					transcriptions: [],
					nextCursor: null,
					hasMore: false,
				};
			}

			const { cursor, limit } = ctx.query;
			const pageSize = Math.min(limit ?? 20, 50);

			let cursorDate: Date | null = null;
			if (cursor) {
				const cleanCursor = cursor.replace(/^"|"$/g, "");
				cursorDate = new Date(cleanCursor);
				if (Number.isNaN(cursorDate.getTime())) {
					return status(400, { error: `Invalid cursor date format: ${cursor}` });
				}
			}

			const whereCondition = cursorDate
				? and(eq(transcriptions.userId, session.user.id), lt(transcriptions.createdAt, cursorDate))
				: eq(transcriptions.userId, session.user.id);

			const results = await db
				.select()
				.from(transcriptions)
				.where(whereCondition)
				.orderBy(desc(transcriptions.createdAt))
				.limit(pageSize + 1);

			const hasMore = results.length > pageSize;
			const items = hasMore ? results.slice(0, -1) : results;
			const nextCursor =
				hasMore && items.length > 0 ? items[items.length - 1]!.createdAt.toISOString() : null;

			return {
				transcriptions: items,
				nextCursor,
				hasMore,
			};
		},
		{
			query: t.Object({
				cursor: t.Optional(t.String()),
				limit: t.Optional(t.Numeric()),
			}),
		}
	)
	.get(
		"/:id",
		async (ctx) => {
			const session = (ctx as any).session as Session;
			const result = await db
				.select()
				.from(transcriptions)
				.where(eq(transcriptions.id, ctx.params.id))
				.limit(1);

			if (result.length === 0) {
				return status(404, { error: "Transcription not found" });
			}

			const transcription = result[0]!;
			if (transcription.userId !== session.user.id) {
				return status(403, { error: "Forbidden" });
			}

			return transcription;
		},
		{
			params: t.Object({
				id: t.String(),
			}),
		}
	)
	.patch(
		"/:id/rating",
		async (ctx) => {
			const session = (ctx as any).session as Session;
			const { id } = ctx.params;
			const { rating } = ctx.body;

			const existing = await db
				.select()
				.from(transcriptions)
				.where(eq(transcriptions.id, id))
				.limit(1);

			if (existing.length === 0) {
				return status(404, { error: "Transcription not found" });
			}

			if (existing[0]!.userId !== session.user.id) {
				return status(403, { error: "Forbidden" });
			}

			await db.update(transcriptions).set({ rating }).where(eq(transcriptions.id, id));

			return { success: true, rating };
		},
		{
			params: t.Object({
				id: t.String(),
			}),
			body: t.Object({
				rating: t.Union([t.Literal("up"), t.Literal("down"), t.Null()]),
			}),
		}
	);
