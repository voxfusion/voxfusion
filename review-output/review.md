VoxFusion product and code review, 14 September 2026

VoxFusion has a useful, focused product: free local dictation with vocabulary and style settings for individual applications and websites. The main improvements should make dictation recoverable, make privacy behavior match the product promise, and explain which settings each speech engine supports. The existing visual identity is consistent, but its small monospace text, low contrast, technical labels, and hidden actions make the app harder to use than it needs to be.

This review covers the current worktree. Competitor capabilities and listed prices were checked against official sources. They were not tested with the same audio, so this is not an accuracy or speed ranking.

**What was tested**

| Check | Result |
| --- | --- |
| `bun install --frozen-lockfile` | Passed |
| `bun run lint` | Passed for both packages |
| `bun run typecheck` | Passed for both packages |
| App frontend production build | Passed |
| Marketing production build | Passed, all 6 pages |
| Native macOS development launch | Passed |
| Native debug app bundle | Passed; then signed locally with an ad-hoc signature |
| `cargo test` | 10 tests passed |
| `bun run check` | Failed with 6 formatting/import-order errors |
| Native history, dictionary, style, and settings | Exercised with isolated synthetic data |
| Marketing homepage and download navigation | Exercised at desktop and 390-pixel mobile width; no horizontal overflow at mobile width; no browser errors observed |
| Speech recording and insertion | Microphone and Accessibility granted; automated shortcut presses did not start recording. A physical-key test was requested. End-to-end transcription/insertion remains unverified. |

The review app uses `io.voxfusion.audit.e10d29e5`, separate settings/history, and Ctrl+Alt+Shift+F9/F10 shortcuts. An existing Whisper model was cloned into the review profile. This does not test a fresh model download or a notarized release installation. Some onboarding stages were bypassed by preparing the test profile; the full first-run flow has not been certified. The production VoxFusion app and its history were not modified.

The user authorized microphone and Accessibility access. System Settings confirmed both permissions were on for VoxFusion Audit. The computer-use tool cannot control UserNotificationCenter, so the user handled that prompt and the OS authentication step. No passwords were requested or handled by the agent. After restarting the audit app, automated Ctrl+Alt+Shift+F9 presses still produced no recording events in its log. This does not establish a shortcut defect: synthesized input may not reach the global shortcut backend. A blank TextEdit document was prepared for a physical-key test.

**Prioritized findings**

1. **High priority: analytics starts enabled before the privacy choice.** This is confirmed by code. The default is `analyticsEnabled: true`; the entrypoint calls `initAnalytics()`, which applies that value without requiring a completed onboarding choice. Privacy is step 5. A person can therefore receive analytics initialization before choosing whether to participate. Default to off, persist whether a choice has been made, and initialize only after an explicit opt-in. Verify that a fresh profile sends no analytics requests before consent and none after opting out. This review does not claim to have captured an outbound analytics payload. Sources: [settingsStore.ts](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/lib/settingsStore.ts:41), [posthog.ts](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/lib/posthog.ts:68), [index.tsx](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/index.tsx:20).

2. **Fixed: configured website domains were sent to Google’s favicon service.** Site icons are now local text initials with a globe fallback. All favicon requests, startup preloading, and domain-entry preloading were removed. Rendering site icons no longer contacts Google or the configured site. Source: [SiteIcon.tsx](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/components/SiteIcon.tsx).

3. **Correction: successful Parakeet transcription already performed recording cleanup.** The original review missed the cleanup call inside the Parakeet handler and incorrectly reported a retention defect. Both engines already kept the latest 20 recordings after successful transcription. Cleanup now runs once in the shared transcription handler to keep that lifecycle consistent. Failed recordings remain available for Retry. Source: [whisper.rs](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src-tauri/src/handlers/whisper.rs).

4. **Dictionary support fixed; style parity requires a product decision.** Both engines now resolve the global dictionary and matching app/site dictionaries before dispatch. Whisper receives the combined vocabulary in its initial prompt; Parakeet receives it through CrispASR’s native hotword boosting. The pinned engine supports this mechanism. Parakeet does not accept prose style prompts. Matching current Whisper style behavior would require routing styled dictations through Whisper; keeping Parakeet for styled dictation requires a separate text-processing stage. Neither routing nor a new text model has been added pending the user’s choice. Sources: [whisper.rs](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src-tauri/src/handlers/whisper.rs), [parakeet.rs](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src-tauri/src/handlers/parakeet.rs), [pinned engine documentation](https://github.com/CrispStrobe/CrispASR/blob/2cb51b114a03005195066c16b7a30de51e60506f/docs/cli.md).

5. **High priority: history is too limited to be a dependable recovery tool.** Reproduced in the native build with a synthetic 243-character entry. The card cuts off after 150 characters; clicking it never expands the text. Copy appears only on hover. There is no search, export, transcript deletion, or retention control. A keyboard user cannot reach a Copy button that has not been rendered by a mouse hover. Add a full-text detail view, always-reachable Copy, search, delete with undo, export, and a retention choice. Keep the current timestamp grouping. Sources: [TranscriptionCard.tsx](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/components/TranscriptionCard.tsx:46), [TranscriptionList.tsx](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/components/TranscriptionList.tsx), [db.rs](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src-tauri/src/handlers/db.rs:84).

6. **Medium priority: failures need a persistent result and recovery path.** `transcribeAndType()` ignores the `Result` returned by `saveTranscription()`. A failed database write can be followed by insertion and a history-created event. Transcription errors disappear after 5 seconds; typing errors disappear after 4 seconds. Accessibility failure opens Settings, whose Audio section has no permission-repair action. Check persistence explicitly and keep the transcript available for Copy, Retry, and Insert again. Show the failed stage and an appropriate repair action. Sources: [VoiceControl.tsx](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/pages/VoiceControl.tsx:403), [error timeouts](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/pages/VoiceControl.tsx:60), [AudioSettings.tsx](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/components/settings/AudioSettings.tsx).

7. **Medium priority: important instructions are difficult to read and controls lack accessible names.** Native screenshots showed faint model details and helper text. The dark faint-text pair is `#444444` on `#111111`, calculated contrast 1.94:1; light faint text is `#aaaaaa` on white, 2.32:1. Both Audio switches appear unnamed in the native accessibility tree. Settings also lacks dialog semantics/focus management, and custom selects lack standard selection semantics. Increase contrast, associate labels with switches, use a dialog with focus management, and use native or fully keyboard-operable selects. Sources: [styles.css](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/styles.css:11), [ToggleOption.tsx](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/components/ToggleOption.tsx:17), [SettingsModal.tsx](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/components/SettingsModal.tsx:101), [Select.tsx](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/components/settings/Select.tsx).

8. **Medium priority: initialization failures have no visible recovery screen.** `App` logs and rethrows initialization exceptions while `isReady` remains false. A settings-store failure can leave the loading indicator visible indefinitely. Display the failed startup step and a Retry action; preserve access to diagnostic information. A browser without the Tauri bridge is an expected unsupported environment, so its spinner is not evidence that the native app fails normally. Source: [App.tsx](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src/App.tsx:193).

9. **Lower priority: dictionary input accepts identical entries.** Reproduced by submitting `VoxFusionReview` twice. Both rows were saved, and the count became `2 WORDS`; one entry displays `1 WORDS`. Duplicate words also consume the limited recognition prompt. Normalize and deduplicate within each scope, show an existing-entry message, and fix count pluralization. Add, edit, and delete otherwise worked. Source: [db.rs](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/packages/app/src-tauri/src/handlers/db.rs:206).

**Design and setup changes**

Keep the orange accent, compact navigation, consistent spacing, and timestamp grouping. Use ordinary readable text for instructions and transcripts; reserve monospace styling for actual shortcuts and technical values. Replace labels such as `AUDIO_CONFIG`, `INPUT_DEVICE`, and `NO_TERMS_FOUND` with Audio, Microphone, and No saved words. Translate all user-facing text, including Settings headings and footer instructions.

The home screen should show whether dictation is ready, the active microphone/model, a visible test-recording action, both shortcut modes, and the last result. Currently it mainly displays history and a small shortcut hint.

Make onboarding explain the outcome and download size before asking for permissions. Group related setup tasks, offer a model recommendation based on the Mac, and provide a short microphone check with input level feedback. The eight numbered steps include privacy, a large download, and practice. The landing page's “start dictating in seconds” copy underestimates that setup for a new installation.

Style says to switch options to “preview” them, but selecting Professional immediately saves the global default. Separate preview from applying a style, or state that changes apply immediately. An actual before/after example would explain more than the current prose description.

The download page clearly separates Apple Silicon and Intel. Add the minimum supported macOS version, how to identify the Mac's chip, the one-time model download size, and what permissions are needed. The [desktop homepage](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/review-output/screenshots/marketing-desktop.png), [mobile homepage](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/review-output/screenshots/marketing-mobile.png), and [download page](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/review-output/screenshots/download.png) were captured. The floating Astro toolbar in these captures belongs to the development server.

**Feature ideas, in suggested order**

| Feature | User benefit | Initial scope |
| --- | --- | --- |
| Persistent last-result panel | Recover text after failed insertion or an accidental focus change | Full text, Copy, Retry, Insert again; explicit destination |
| Exact replacements and spoken snippets | Correct recurring names and expand phrases into precise text | Local rules such as “meeting link” to an exact URL; works with both speech engines |
| Combined app/site profiles | Explain all behavior active in one place | Vocabulary, style, language, model, retention; preview precedence and unsupported options |
| Speech-language control | Reduce wrong-language detection on short or mixed-language dictation | Auto plus explicit languages, optionally per app |
| Optional local cleanup with a diff | Remove repetitions and format paragraphs without hiding changes | Preserve the raw transcript; show the edited result and let the user revert |
| Reusable vocabulary packs | Share project terminology without requiring accounts | Import/export a documented local JSON/CSV format, with duplicate handling |

The most defensible product direction is reliable local dictation for people who use specialized vocabulary across different apps. The code already has app/site context and local storage; develop those into clear, testable behavior before expanding into meeting recording or a general AI assistant.

**Code and performance follow-up**

Whisper creates a new model context for every dictation. Parakeet starts an engine subprocess per request. Measure end-of-speech-to-insertion latency on repeated 5-second, 30-second, and 2-minute clips, with memory use and an explicit cold/warm distinction. Consider a persistent inference worker and idle unloading if those measurements justify it. No performance gain is claimed here without a benchmark.

The recognition prompt is cut to 700 characters after joining style, global vocabulary, app vocabulary, and site vocabulary. A large global dictionary can displace the more relevant app/site words without any UI feedback. Select vocabulary by relevance, deduplicate it, and give users feedback when their list exceeds the effective prompt budget. Style is implemented as recognition prompting; it is not a separate text-rewriting stage. Do not promise reliable rewriting without measuring it on a fixed audio corpus.

The 10 Rust tests cover browser-domain parsing and key-state discontinuities. Add focused tests for consent, model capabilities, retention across engines, persistence/insertion failures, and recording start/stop/cancel. A deliberate Tauri UI test bridge would also let browser automation cover history and settings. Avoid broad snapshot tests that only restate component markup.

There are existing strengths worth retaining: typed command wrappers, local SQLite with WAL, checksummed model downloads, resumable download handling, explicit audio-device error events, recording diagnostics, and key-state resynchronization. The full-repository check failure is formatting/import ordering, while the narrower lint job used in CI passes. The six failing paths are listed in [biome-check.log](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/review-output/biome-check.log); the native build and Rust test logs are also saved here.

**Alternatives**

| Alternative | Processing and listed price | Relevant comparison |
| --- | --- | --- |
| [Handy](https://github.com/cjpais/Handy) | Free, MIT, local; macOS, Windows, Linux | Closest direct competitor. Offers Whisper model sizes, Parakeet, voice activity detection, hold/toggle shortcuts, and CLI integration. Free/offline/open source alone is not a unique position for VoxFusion. |
| [VoiceInk](https://tryvoiceink.com/) | Local speech models; open-source code and paid ready-made app. [Pricing](https://tryvoiceink.com/pricing) lists $25 for one Mac, $39 for two, $49 for three, one time. | Custom vocabulary, exact replacements, selected-text rewriting, and voice-to-Markdown workflows are useful references. The current Mac build requires Apple Silicon and macOS 14.4+. Check the processing provider for each enhancement mode separately. |
| [Superwhisper](https://superwhisper.com/docs/billing/plans) | Pro includes local models and custom modes. $8.49/month, $84.99/year, or $249.99 lifetime. | Useful reference for modes, vocabulary, and history reprocessing. Current plan documentation places local models in Pro. |
| [Wispr Flow](https://wisprflow.ai/pricing) | Free tier; Pro $15/month or $12/month billed annually. [Requires internet for transcription](https://docs.wisprflow.ai/articles/2772472373-what-is-flow). | Useful reference for cleanup, self-corrections, learned vocabulary, and cross-platform workflows. VoxFusion offers local processing without a subscription. |
| [MacWhisper](https://www.macwhisper.com/) | Free version; Pro listed at €64 per license with lifetime updates. Local models plus optional AI integrations. | A broader transcription workspace: file/batch transcription, subtitles, speaker recognition, app-specific prompts, CLI, and export. Matching its whole scope would substantially expand VoxFusion. |
| [Apple Dictation](https://support.apple.com/en-ie/guide/mac-help/mh40584/mac) | Included in macOS. On-device processing depends on language and configuration, as shown in Keyboard settings. | Baseline for system-integrated text entry. VoxFusion needs to earn its additional setup through vocabulary accuracy, recoverable history, and predictable app-specific behavior. |

Prices above are what the cited official pages displayed on the review date, before any checkout-specific taxes or regional adjustments. These alternatives were researched, not installed and benchmarked during this review.

**Validation after the requested fixes**

App lint, typecheck, and frontend production build passed. All 13 Rust tests passed, including global/app/site dictionary isolation, absent contexts, and preservation of Whisper prompts. A synthetic WAV was transcribed successfully using the installed Parakeet engine with vocabulary arguments; the result is saved in [parakeet-vocabulary-output.txt](/Users/egortokarev/.t3/worktrees/voxfusion/t3code-e10d29e5/review-output/parakeet-vocabulary-output.txt). This smoke test checks file transcription with the new argument; it is not a vocabulary accuracy benchmark or a microphone/insertion test.
