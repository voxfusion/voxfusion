import { GladiaClient } from "@gladiaio/sdk";

// Live transcription client (WebSocket-based)
export const gladiaLive = new GladiaClient({
    apiKey: process.env.GLADIA_API_KEY!,
});

// ============================================
// Async (Pre-recorded) Transcription SDK
// ============================================

const GLADIA_API_URL = "https://api.gladia.io";

export interface TranslationConfig {
    target_languages: string[];
    model?: "base" | "enhanced";
    match_original_utterances?: boolean;
    lipsync?: boolean;
    context_adaptation?: boolean;
    context?: string;
    informal?: boolean;
}

export interface TranscribeOptions {
    audio_url: string;
    language_config?: {
        languages?: string[];
        code_switching?: boolean;
    };
    translation?: boolean;
    translation_config?: TranslationConfig;
    diarization?: boolean;
    diarization_config?: {
        min_speakers?: number;
        max_speakers?: number;
    };
    subtitles?: boolean;
    subtitles_config?: {
        formats?: ("srt" | "vtt")[];
    };
    summarization?: boolean;
    summarization_config?: {
        type?: "general" | "bullet_points" | "concise";
    };
    callback_url?: string;
}

export interface Utterance {
    text: string;
    start: number;
    end: number;
    speaker?: number;
    confidence?: number;
    words?: Array<{
        word: string;
        start: number;
        end: number;
        confidence: number;
    }>;
}

export interface TranscriptionResult {
    id: string;
    status: "queued" | "processing" | "done" | "error";
    result?: {
        transcription: {
            full_transcript: string;
            utterances: Utterance[];
        };
        translation?: Record<string, {
            full_transcript: string;
            utterances: Array<{
                text: string;
                start: number;
                end: number;
            }>;
        }>;
        summarization?: {
            summary: string;
        };
        subtitles?: Array<{
            format: string;
            url: string;
        }>;
    };
    error?: {
        message: string;
        code?: string;
    };
}

interface TranscriptionInitResponse {
    id: string;
    result_url: string;
}

class GladiaAsyncClient {
    private apiKey: string;
    private baseUrl: string;

    constructor(options: { apiKey: string; baseUrl?: string }) {
        this.apiKey = options.apiKey;
        this.baseUrl = options.baseUrl ?? GLADIA_API_URL;
    }

    private async request<T>(
        endpoint: string,
        options: RequestInit = {}
    ): Promise<T> {
        const response = await fetch(`${this.baseUrl}${endpoint}`, {
            ...options,
            headers: {
                "Content-Type": "application/json",
                "x-gladia-key": this.apiKey,
                ...options.headers,
            },
        });

        if (!response.ok) {
            const error = await response.text();
            throw new Error(`Gladia API error (${response.status}): ${error}`);
        }

        return response.json() as Promise<T>;
    }

    /**
     * Upload an audio file and get a URL that can be used for transcription
     */
    async upload(file: Blob | ArrayBuffer): Promise<{ audio_url: string }> {
        const formData = new FormData();
        formData.append(
            "audio",
            file instanceof Blob ? file : new Blob([file]),
            "audio.webm"
        );

        const response = await fetch(`${this.baseUrl}/v2/upload`, {
            method: "POST",
            headers: {
                "x-gladia-key": this.apiKey,
            },
            body: formData,
        });

        if (!response.ok) {
            const error = await response.text();
            throw new Error(`Gladia upload error (${response.status}): ${error}`);
        }

        return response.json() as Promise<{ audio_url: string }>;
    }

    /**
     * Initiate an async transcription job
     * Returns the job ID and result URL for polling
     */
    async transcribe(options: TranscribeOptions): Promise<TranscriptionInitResponse> {
        return this.request<TranscriptionInitResponse>("/v2/pre-recorded", {
            method: "POST",
            body: JSON.stringify(options),
        });
    }

    /**
     * Get the status/result of a transcription job
     */
    async getResult(transcriptionId: string): Promise<TranscriptionResult> {
        return this.request<TranscriptionResult>(`/v2/pre-recorded/${transcriptionId}`);
    }

    /**
     * Poll for transcription result until done or error
     */
    async waitForResult(
        transcriptionId: string,
        options?: { pollInterval?: number; timeout?: number }
    ): Promise<TranscriptionResult> {
        const pollInterval = options?.pollInterval ?? 1000;
        const timeout = options?.timeout ?? 300_000; // 5 minutes default
        const startTime = Date.now();

        while (true) {
            const result = await this.getResult(transcriptionId);

            if (result.status === "done") {
                return result;
            }

            if (result.status === "error") {
                throw new Error(
                    `Transcription failed: ${result.error?.message ?? "Unknown error"}`
                );
            }

            if (Date.now() - startTime > timeout) {
                throw new Error("Transcription timed out");
            }

            await new Promise((resolve) => setTimeout(resolve, pollInterval));
        }
    }

    /**
     * Upload file and transcribe in one call, waiting for the result
     */
    async transcribeFile(
        file: Blob | ArrayBuffer,
        options?: Omit<TranscribeOptions, "audio_url"> & {
            pollInterval?: number;
            timeout?: number;
        }
    ): Promise<TranscriptionResult> {
        // Upload the file
        const { audio_url } = await this.upload(file);

        // Start transcription
        const { id } = await this.transcribe({
            audio_url,
            ...options,
        });

        // Wait for result
        return this.waitForResult(id, {
            pollInterval: options?.pollInterval,
            timeout: options?.timeout,
        });
    }
}

// Export singleton instance
export const gladia = new GladiaAsyncClient({
    apiKey: process.env.GLADIA_API_KEY!,
});

// Export class for custom instances
export { GladiaAsyncClient };