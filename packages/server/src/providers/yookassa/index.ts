const YOOKASSA_API_URL = "https://api.yookassa.ru/v3";

export interface YooKassaConfig {
	shopId: string;
	secretKey: string;
}

export interface CreatePaymentParams {
	amount: {
		value: string;
		currency: string;
	};
	capture: boolean;
	confirmation: {
		type: "redirect";
		return_url: string;
	};
	description?: string;
	metadata?: Record<string, string>;
	save_payment_method?: boolean;
	payment_method_id?: string;
}

export interface YooKassaPayment {
	id: string;
	status: "pending" | "waiting_for_capture" | "succeeded" | "canceled";
	paid: boolean;
	amount: {
		value: string;
		currency: string;
	};
	confirmation?: {
		type: string;
		confirmation_url: string;
	};
	created_at: string;
	description?: string;
	metadata?: Record<string, string>;
	payment_method?: {
		type: string;
		id: string;
		saved: boolean;
		title?: string;
		card?: {
			first6: string;
			last4: string;
			expiry_month: string;
			expiry_year: string;
			card_type: string;
		};
	};
	recipient: {
		account_id: string;
		gateway_id: string;
	};
	refundable: boolean;
	test: boolean;
}

export interface CreateRecurringPaymentParams {
	amount: {
		value: string;
		currency: string;
	};
	capture: boolean;
	payment_method_id: string;
	description?: string;
	metadata?: Record<string, string>;
}

export interface YooKassaWebhookPayload {
	type: "notification";
	event:
		| "payment.waiting_for_capture"
		| "payment.succeeded"
		| "payment.canceled"
		| "refund.succeeded";
	object: YooKassaPayment;
}

class YooKassaClient {
	private shopId: string;
	private secretKey: string;
	private authHeader: string;

	constructor(config: YooKassaConfig) {
		this.shopId = config.shopId;
		this.secretKey = config.secretKey;
		this.authHeader = `Basic ${btoa(`${config.shopId}:${config.secretKey}`)}`;
	}

	private async request<T>(
		method: string,
		endpoint: string,
		body?: object,
		idempotenceKey?: string
	): Promise<T> {
		const headers: Record<string, string> = {
			Authorization: this.authHeader,
			"Content-Type": "application/json",
		};

		if (idempotenceKey) {
			headers["Idempotence-Key"] = idempotenceKey;
		}

		const response = await fetch(`${YOOKASSA_API_URL}${endpoint}`, {
			method,
			headers,
			body: body ? JSON.stringify(body) : undefined,
		});

		if (!response.ok) {
			const error = await response.json().catch(() => ({}));
			throw new Error(`YooKassa API error: ${response.status} - ${JSON.stringify(error)}`);
		}

		return response.json();
	}

	async createPayment(
		params: CreatePaymentParams,
		idempotenceKey?: string
	): Promise<YooKassaPayment> {
		return this.request<YooKassaPayment>(
			"POST",
			"/payments",
			params,
			idempotenceKey || crypto.randomUUID()
		);
	}

	async getPayment(paymentId: string): Promise<YooKassaPayment> {
		return this.request<YooKassaPayment>("GET", `/payments/${paymentId}`);
	}

	async capturePayment(
		paymentId: string,
		amount?: { value: string; currency: string },
		idempotenceKey?: string
	): Promise<YooKassaPayment> {
		return this.request<YooKassaPayment>(
			"POST",
			`/payments/${paymentId}/capture`,
			amount ? { amount } : {},
			idempotenceKey || crypto.randomUUID()
		);
	}

	async cancelPayment(paymentId: string, idempotenceKey?: string): Promise<YooKassaPayment> {
		return this.request<YooKassaPayment>(
			"POST",
			`/payments/${paymentId}/cancel`,
			{},
			idempotenceKey || crypto.randomUUID()
		);
	}

	async createRecurringPayment(
		params: CreateRecurringPaymentParams,
		idempotenceKey?: string
	): Promise<YooKassaPayment> {
		return this.request<YooKassaPayment>(
			"POST",
			"/payments",
			{
				...params,
				capture: true,
			},
			idempotenceKey || crypto.randomUUID()
		);
	}

	async createSubscriptionPayment(
		userId: string,
		returnUrl: string,
		description = "Подписка VoxFusion Pro"
	): Promise<YooKassaPayment> {
		return this.createPayment(
			{
				amount: {
					value: "800.00",
					currency: "RUB",
				},
				capture: true,
				confirmation: {
					type: "redirect",
					return_url: returnUrl,
				},
				description,
				save_payment_method: true,
				metadata: {
					user_id: userId,
					type: "subscription",
					plan: "pro",
				},
			},
			`subscription-${userId}-${Date.now()}`
		);
	}

	async renewSubscription(userId: string, paymentMethodId: string): Promise<YooKassaPayment> {
		return this.createRecurringPayment(
			{
				amount: {
					value: "800.00",
					currency: "RUB",
				},
				capture: true,
				payment_method_id: paymentMethodId,
				description: "Продление подписки VoxFusion Pro",
				metadata: {
					user_id: userId,
					type: "subscription_renewal",
					plan: "pro",
				},
			},
			`renewal-${userId}-${Date.now()}`
		);
	}
}

export const yookassa = new YooKassaClient({
	shopId: process.env.YOOKASSA_SHOP_ID || "",
	secretKey: process.env.YOOKASSA_SECRET_KEY || "",
});

export { YooKassaClient };
