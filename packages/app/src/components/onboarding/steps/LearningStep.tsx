import { emit, listen } from "@tauri-apps/api/event";
import { Send } from "lucide-solid";
import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import { useI18n } from "../../../i18n";
import { useSettings } from "../../../lib/settingsStore";

interface LearningStepProps {
	onTranscriptionComplete: () => void;
}

function parseHotkeyParts(hotkey: string): string[] {
	const DISPLAY_MAP: Record<string, string> = {
		Command: "\u2318",
		Control: "\u2303",
		Alt: "\u2325",
		Shift: "\u21E7",
	};
	return hotkey.split("+").map((part) => DISPLAY_MAP[part] ?? part);
}

export default function LearningStep(props: LearningStepProps) {
	const [t] = useI18n();
	const settings = useSettings();
	const [messages, setMessages] = createSignal<string[]>([]);
	const [inputText, setInputText] = createSignal("");

	let inputRef: HTMLInputElement | undefined;

	const hotkeyParts = () => parseHotkeyParts(settings().hotkey);

	const addMessage = () => {
		const text = inputText().trim();
		if (!text) return;
		setMessages((prev) => [...prev, text]);
		setInputText("");
		props.onTranscriptionComplete();
	};

	onMount(async () => {
		let unlisten: (() => void) | undefined;

		onCleanup(() => {
			unlisten?.();
			emit("learning-step-active", false);
		});

		await emit("learning-step-active", true);

		unlisten = await listen("transcription-created", () => {
			// Small delay to ensure type_text has fully populated the input
			setTimeout(() => {
				addMessage();
				inputRef?.focus();
			}, 100);
		});

		inputRef?.focus();
	});

	return (
		<div class="w-full max-w-4xl mx-auto">
			{/* Terminal-style header */}
			<div class="font-mono text-ac text-sm mb-8 tracking-wider text-center">
				[STEP_06] &gt; TRY_IT_OUT
			</div>

			<h2 class="font-mono text-xl uppercase tracking-wider text-txt-primary mb-2 text-center">
				{t("onboarding.learningTitle")}
			</h2>
			<p class="font-mono text-sm text-txt-secondary mb-8 text-center">
				{t("onboarding.learningDescription")}
			</p>

			{/* Split view */}
			<div class="flex gap-6">
				{/* Left: Manual */}
				<div class="flex-1 border border-border bg-th-surface p-6">
					<div class="space-y-6">
						{/* Step 1 */}
						<div class="flex items-start gap-3">
							<span class="font-mono text-ac text-sm font-bold mt-0.5">1.</span>
							<div class="flex items-center gap-2 flex-wrap">
								<span class="font-mono text-sm text-txt-secondary">
									{t("onboarding.learningStep1Prefix")}
								</span>
								<For each={hotkeyParts()}>
									{(part) => (
										<kbd class="inline-flex items-center justify-center min-w-[2rem] px-2 py-1 font-mono text-sm border border-border-strong bg-th-base text-txt-primary rounded">
											{part}
										</kbd>
									)}
								</For>
								<span class="font-mono text-sm text-txt-secondary">
									{t("onboarding.learningStep1Suffix")}
								</span>
							</div>
						</div>

						{/* Step 2 */}
						<div class="flex items-start gap-3">
							<span class="font-mono text-ac text-sm font-bold mt-0.5">2.</span>
							<span class="font-mono text-sm text-txt-secondary">
								{t("onboarding.learningStep2")}
							</span>
						</div>

						{/* Step 3 */}
						<div class="flex items-start gap-3">
							<span class="font-mono text-ac text-sm font-bold mt-0.5">3.</span>
							<div class="flex items-center gap-2 flex-wrap">
								<span class="font-mono text-sm text-txt-secondary">
									{t("onboarding.learningStep3Prefix")}
								</span>
								<For each={hotkeyParts()}>
									{(part) => (
										<kbd class="inline-flex items-center justify-center min-w-[2rem] px-2 py-1 font-mono text-sm border border-border-strong bg-th-base text-txt-primary rounded">
											{part}
										</kbd>
									)}
								</For>
								<span class="font-mono text-sm text-txt-secondary">
									{t("onboarding.learningStep3Suffix")}
								</span>
							</div>
						</div>

						{/* Step 4 */}
						<div class="flex items-start gap-3">
							<span class="font-mono text-ac text-sm font-bold mt-0.5">4.</span>
							<span class="font-mono text-sm text-txt-secondary">
								{t("onboarding.learningStep4")}
							</span>
						</div>
					</div>
				</div>

				{/* Right: Chat-style */}
				<div class="flex-1 border border-border bg-th-surface flex flex-col h-[340px]">
					{/* Messages area */}
					<div class="flex-1 p-4 overflow-y-auto space-y-3">
						<Show
							when={messages().length > 0}
							fallback={
								<div class="h-full flex items-center justify-center">
									<span class="font-mono text-sm text-txt-muted">
										{t("onboarding.learningPlaceholder")}
									</span>
								</div>
							}
						>
							<For each={messages()}>
								{(msg) => (
									<div class="flex justify-end">
										<div class="max-w-[80%] px-4 py-2 rounded-2xl rounded-br-sm bg-ac text-ac-on font-mono text-sm">
											{msg}
										</div>
									</div>
								)}
							</For>
						</Show>
					</div>

					{/* Input area */}
					<div class="border-t border-border p-3 flex items-center gap-2">
						<input
							ref={inputRef}
							type="text"
							value={inputText()}
							onInput={(e) => setInputText(e.currentTarget.value)}
							placeholder={t("onboarding.learningPlaceholder")}
							class="flex-1 px-4 py-2 rounded-full bg-th-base border border-border font-mono text-sm text-txt-primary placeholder:text-txt-muted focus:outline-none focus:border-ac"
						/>
						<button
							type="button"
							onClick={addMessage}
							class="w-9 h-9 rounded-full bg-ac text-ac-on flex items-center justify-center hover:bg-ac-hover transition-colors disabled:opacity-30"
							disabled={!inputText()}
						>
							<Send class="w-4 h-4" />
						</button>
					</div>
				</div>
			</div>
		</div>
	);
}
