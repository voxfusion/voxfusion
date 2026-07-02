# VoxFusion Improvement Audit — 2026-07-02

Produced by a five-track deep audit (Rust backend, app frontend, product UX, marketing site, tooling/CI/docs). Every finding was verified against the actual code — line numbers are current as of commit `b0135b8`. ~190 unique findings after deduplication.

Severity: **H** = high (user-facing breakage, security/privacy, trust), **M** = medium, **L** = low.

---

## Top priorities (start here)

1. **Cache the Whisper model** — the ~1.5 GB model is loaded from disk on *every* transcription (`src-tauri/src/handlers/whisper.rs:176`), adding multi-second latency to every dictation. Cache `WhisperContext` in Tauri managed state, invalidate on model switch. Single biggest perceived-performance win in the product.
2. **Make the core dictation loop stop failing silently** — one theme, many sites: transcription errors vanish with no UI (`VoiceControl.tsx:311-314`), recording-start failures do nothing visible (`VoiceControl.tsx:410-417`), a mic unplugged mid-recording is swallowed by `eprintln!` (`audio.rs:122-124`), and a revoked Accessibility permission means text silently never types — the recovery event `accessibility-permission-needed` is listened for (`App.tsx:150-154`) but *never emitted anywhere*. Add an error state to the overlay pill + a retry path (the WAV survives on disk).
3. **Fix the recording state latch** — any error after `is_recording.store(true)` (`audio.rs:87`) leaves the recorder latched; every later dictation fails with "Recording is already in progress" until restart. Use a drop-guard that resets the flag on error paths.
4. **The overlay window ignores the user's default style** — `initSettings()` only runs in the main window; the voice-control window is a separate JS context, so `settings().defaultStyle` is always `"default"` (`VoiceControl.tsx:100,309`). User-facing data bug on every transcription.
5. **Close the security holes as a set** — `"csp": null` (`tauri.conf.json:44`) + every system-wide keystroke broadcast into the webview (`system_key_watcher.rs:133-146`) + `read_audio_file` accepting arbitrary absolute paths (`handlers/text.rs:12`) compose into: any webview compromise = system keylogger + arbitrary file exfiltration. Set a CSP, match hotkeys in Rust, and restrict paths to the recordings dir.
6. **Tell one honest privacy story** — the product's positioning ("local-first, 0 data sent to servers") is contradicted by: PostHog running unconditionally with no consent or opt-out (`src/lib/posthog.ts:25-39`), every dictation WAV kept on disk forever with no cleanup (`audio.rs:244-258`), the marketing hero stat "0 data sent to servers", a privacy policy that never names PostHog/Umami, and a README claiming "no servers". Fix together: analytics opt-out toggle + onboarding disclosure, WAV deletion after transcription (or retention setting), reworded marketing claims ("0 *audio* sent to servers"), updated policy.
7. **Harden the 1.5 GB model download** — not resumable (partial deleted before every retry, `models.rs:183-189`), not cancellable, no checksum (URL pins mutable `resolve/main`), no in-flight guard (double-click = two writers on the same temp file), 1-hour *total* timeout kills slow connections, and progress events fire per chunk (hundreds of thousands of IPC events). Add Range resume, cancel, SHA-256 + pinned revision, per-model in-flight lock, and emit on whole-percent change.
8. **`panic = "abort"` defeats every `catch_unwind`** (`Cargo.toml:62` vs `apps.rs:238,270,347`) — a malformed `Info.plist`/`.icns` in /Applications, or an ObjC exception in `get_frontmost_app` (runs at the start of *every* dictation), aborts the whole app in release. Drop `panic = "abort"` or stop relying on catch_unwind.
9. **Hotkeys permanently die if Accessibility isn't granted at first watcher start** — `addGlobalMonitorForEventsMatchingMask_handler` returns `None`, it's silently forgotten, and the `WATCHER_RUNNING` latch blocks all retries until app restart (`system_key_watcher.rs:160,176-193`).
10. **Onboarding dead-end: denied microphone permission** — no "Open System Settings" link and no skip; once macOS TCC denies, every retry auto-fails and the user is stuck on step 1 forever (`MicrophonePermissionStep.tsx:35-51`).
11. **Marketing says the wrong hotkey** — site teaches `Cmd+;` in three places (`i18n/ru/hero.ts:6`, `features.ts:8`, `index.astro:83`); the app default is `LeftControl+LeftOption` (`hotkeyUtils.ts:44`). Also: Terms prohibit decompiling / competing products for an MIT-licensed app with a public repo — direct license contradiction.
12. **No CI whatsoever** — no PR checks, and `release.yml` ships from a tag with zero lint/typecheck/test gates. Add a `ci.yml` (biome, tsc, astro check, vite build on ubuntu; `cargo check`/`cargo test` on macos) and make release depend on it.
13. **Update progress bar is mathematically broken** — raw chunk *bytes* accumulate into the percent signal and render as `width: bytes/10 %`, pegging at 100% after ~1 KB (`UpdateNotification.tsx:103-105,152`); `contentLength` is checked but never stored. Also no changelog shown, no dismiss button.

### Quick wins (minutes each, big payoff)

- **One-file tsconfig fix kills all ~1157 `astro check` errors**: marketingsite tsconfig inherits the root's repo-wide `include` + `jsx: "react-jsx"`, so it type-checks the app's Solid TSX as React. Change to `"extends": "astro/tsconfigs/strict"`, `"include": ["src", ".astro/types.d.ts"]`.
- **Two-line biome.json override makes `bun run lint` exit clean**: add `noUnusedImports: off` + `useImportType: off` to the `**/*.astro` override.
- **Docs' first command is broken**: `bun run tauri:dev` doesn't exist (README:50, CONTRIBUTING:33, AGENTS:20) — the script is `dev`.
- **Home-screen hotkey hint is hardcoded to the default hotkey** (`en.ts:219,292-293`) — users with a custom hotkey are shown wrong instructions on the main screen. Interpolate `hotkeyDisplayName(settings().hotkey)` as `LearningStep.tsx:36` already does.
- **`bun install --frozen-lockfile`** in release.yml:40 and the Dockerfile.
- **Remove the squatting-risk updater endpoint** — `tauri.conf.json:50-53` tries `github.com/voxfusion/voxfusion` first, an org that doesn't match the actual repo owner.

---

## A. Desktop app — Rust backend (src-tauri)

### High
- **A1 (H)** `Cargo.toml:62` + `apps.rs:238,270,347` — `panic = "abort"` makes `catch_unwind` useless; malformed plist/icns or ObjC exception aborts the app (see Top #8).
- **A2 (H)** `audio.rs:87` — errors after `is_recording.store(true)` latch the recorder forever (see Top #3).
- **A3 (H)** `whisper.rs:176` — model loaded from disk every transcription (see Top #1).
- **A4 (H)** `system_key_watcher.rs:133-146` — every global keydown's keycode emitted into the webview; with no CSP this is a keylogger stream. Match chords in Rust or emit only during hotkey recording.
- **A5 (H)** `tauri.conf.json:44` — `"csp": null`. Set a CSP restricted to `'self'` + PostHog origin.
- **A6 (H)** `audio.rs:121-124` — cpal stream errors (mic disconnect) swallowed by `eprintln!`; UI keeps showing "recording". Emit a `recording-error` event and finalize.
- **A7 (H)** `system_key_watcher.rs:160,176-193` — watcher install failure silently forgotten + `WATCHER_RUNNING` latch prevents retry (see Top #9).

### Medium
- **A8 (M)** `audio.rs:210-235` — stop takes the writer *before* pausing the stream; audio tail between the two is dropped — users lose the last word. Pause/drop stream first.
- **A9 (M)** `audio.rs:290,298` — callback drops whole chunks on `try_lock` failure and swallows write errors; disk-full = silently corrupt WAV. Use a lock-free channel + surface write failures.
- **A10 (M)** `audio.rs:16-19` — `unsafe impl Send/Sync for SafeStream` overrides cpal's deliberate `!Send`/`!Sync` with no justification; stream is created and dropped on different tokio workers. Own the stream on a dedicated thread driven by a channel.
- **A11 (M)** Sync commands block the main thread: `audio_processing.rs:4` (full WAV decode+resample), `text.rs:12` (whole-file read), `text.rs:5` (enigo types entire transcription while the event loop is frozen). Make async + `spawn_blocking`.
- **A12 (M)** `audio_processing.rs:4`, `text.rs:12` — `Vec<u8>` returned as JSON number arrays over IPC (~4× size). Return `tauri::ipc::Response::new(bytes)`.
- **A13 (M)** `whisper.rs:189-320`, `parakeet.rs:84-93` — seconds-to-minutes of inference runs inline on a tokio worker, starving the runtime. Wrap in `spawn_blocking` / `tokio::process`.
- **A14 (M)** `parakeet.rs:84-93` — `Command::output()` with no timeout; hung engine hangs transcription forever (browser.rs osascript gets 800 ms — apply same discipline).
- **A15 (M)** `parakeet.rs:41` + `entitlements.plist:13` — engine falls back to `$PATH` resolution and `disable-library-validation` lets the app load unsigned dylibs from user-writable app-data; any same-user process can plant a `crispasr` binary. Drop the PATH fallback, sign the engine, remove the entitlement.
- **A16 (M)** `entitlements.plist:7,9,13` — `allow-unsigned-executable-memory` is a superset of `allow-jit` and likely neither is needed. Empirically trim.
- **A17 (M)** `models.rs` — no in-flight download guard; concurrent downloads interleave into the same `.download.tmp` (see Top #7).
- **A18 (M)** `models.rs:53,66,109` — no checksum; URL pins mutable `resolve/main`; only size is checked. Pin a revision hash + verify SHA-256.
- **A19 (M)** `models.rs:192,211,222` — 1-hour total request timeout, blocking `std::fs` on the runtime, no Range resume (see Top #7).
- **A20 (M)** `models.rs:234` — progress emitted per chunk. Emit on integer-percent change.
- **A21 (M)** `audio.rs:243-258` — recordings written to app-data forever; nothing deletes them (see Top #6).
- **A22 (M)** `text.rs:12-14` (+ `audio_processing.rs:4`, whisper `audio_path`) — commands accept arbitrary absolute paths from the webview; generic file-read primitive. Canonicalize + require paths inside the recordings dir.
- **A23 (M)** `posthog.ts:25-39` / `index.tsx:19` — analytics with no opt-out (see Top #6). Payloads verified metadata-only.
- **A24 (M)** `lib.rs:143-148` — single-instance plugin registered inside `.setup()` with `let _ =`; must be the *first* plugin registered, and errors are swallowed.
- **A25 (M)** `tray.rs:80,107` — CoreAudio device enumeration runs synchronously on the main thread in menu handlers; errors discarded.
- **A26 (M)** `apps.rs:560+` — `list_installed_apps` re-scans three dirs and re-decodes every `.icns`→PNG→base64 on every call, no cache. Cache keyed by path+mtime.
- **A27 (M)** `capabilities/desktop.json:14` — `core:webview:allow-internal-toggle-devtools` granted in production capability.
- **A28 (M)** `media.rs` — crash/quit between mute and restore leaves system output muted; default-output change mid-recording unmutes the wrong device. Restore in `RunEvent::Exit`, remember the device id.
- **A29 (M)** `tauri.conf.json:50-53` — updater endpoint #1 points at a GitHub org that isn't yours (see Quick wins).

### Low
- **A30 (L)** `audio.rs:255` — second-resolution filenames; two recordings in one second overwrite. Add ms/UUID.
- **A31 (L)** `db.rs:261`, `apps.rs:598`, `sites.rs:184` — updates return `created_at: String::new()`. Use `RETURNING`.
- **A32 (L)** `db.rs:118+` — cursor pagination on `created_at < ?` skips rows sharing a timestamp. Composite `(created_at, id)` cursor.
- **A33 (L)** `db.rs:73` — only index on `dictionary_words` is on `word`, which no query uses; queries order by `created_at`.
- **A34 (L)** `eprintln!` on the hot path never reaches the log file in bundled apps (`whisper.rs:196,226,270,278`, `audio.rs:123`, `browser.rs:120,131,141`); `whisper.rs:270` prints the user's dictionary contents to stderr. Use `log::` and drop user content.
- **A35 (L)** Duplicated decode→mono→resample pipeline (`audio_processing.rs:12-63` vs `whisper.rs:31-70`); `whisper.rs:118-133` duplicates `db.rs:275-292`.
- **A36 (L)** Linear-interpolation resample to 16 kHz with no anti-aliasing low-pass degrades transcription accuracy. Use `rubato`.
- **A37 (L)** Dep hygiene: duplicate `objc2` 0.5/0.6 + `core-foundation` 0.9/0.10 trees; `tokio = "full"` when only rt/fs/process/time/macros are needed.
- **A38 (L)** `tray.rs:100-137` — mic submenu never removes disconnected devices; menu IDs collide for identically-named devices. Rebuild the submenu each time.
- **A39 (L)** `audio.rs:22-45` — `Mutex<RecordingState>` whose every field is also `Arc<Mutex<Option<…>>>`; flatten to `Mutex<Option<ActiveRecording>>`.
- **A40 (L)** `system_key_watcher.rs:102` — `.expect()` inside the event monitor aborts under panic=abort. Use `let Ok(..) else return`.
- **A41 (L)** `window.rs:29-53` — window-creation failures swallowed with no log.
- **A42 (L)** `browser.rs:57-86` — hand-rolled URL parsing mangles IPv6 hosts. Use the `url` crate.
- **A43 (L)** whisper/parakeet — `split_whitespace().count()` gives word_count=1 for CJK; the app ships a zh locale. Use unicode segmentation.
- **A44 (L)** `capabilities/desktop.json:8,10` — `clipboard-manager:default` listed twice; `opener:default` may subsume the explicit scoped allowlist.

**Verified fine:** WAL + FK pragmas and sensible transcription indices; atomic temp-file+rename on downloads; updater minisign pubkey pinned; AppleScript only for allowlisted bundle IDs; osascript 800 ms deadline; `spawn_blocking` used correctly in apps.rs; PostHog payloads contain no transcription text.

---

## B. Desktop app — frontend (SolidJS)

Framework confirmed **SolidJS** (README is correct). Several bugs below are React idioms that silently do nothing in Solid.

### High
- **B1 (H)** `VoiceControl.tsx:100,309` — overlay window never runs `initSettings()`; always transcribes with `"default"` style (see Top #4).
- **B2 (H)** `SettingsModal.tsx:81-92` — `createEffect` "returning a cleanup" is a React-ism; the cleanup never runs, so every modal open adds a permanent window keydown listener and Escape fires `onClose` even when closed. Use `onCleanup`.
- **B3 (H)** `ModelSettings.tsx:35-46` — `onCleanup` after `await` in async `onMount` runs outside the reactive owner → `model-download-progress` listener leaks on every Model-section open, mutating disposed signals. Register synchronous `onCleanup` + `disposed` flag (correct pattern exists in `UpdateNotification.tsx:64-80`).
- **B4 (H)** `ModelDownloadStep.tsx:33-41` — same bug: listener leaks each time step 5 mounts.
- **B5 (H)** `UpdateNotification.tsx:104,152` — progress bar accumulates raw bytes, renders `bytes/10 %` (see Top #13).

### Medium — correctness
- **B6 (M)** `OnboardingWizard.tsx:32-57` — no `isAnimating()` guard on Next/Back; double-click skips a step and bypasses `canProceed()` permission gating.
- **B7 (M)** `VoiceControl.tsx:207-250` + `hotkeyUtils.ts:309-331` — concurrent hotkey re-registration has no mutex; interleaving leaks registrations so one keypress fires two handlers (instant start+stop). Serialize through a promise chain.
- **B8 (M)** `TranscriptionList.tsx:100-103,129-132` and `AccessibilityPermissionStep.tsx:80-82` — `unlisten` assigned after `await listen(...)`; unmount before resolve leaks the listener into a disposed component.
- **B9 (M)** `App.tsx:139,148,154,164` — four no-op `onCleanup`s after awaits (harmless only because App never unmounts; breaks HMR and invites copy-paste bugs — B3/B4 are that bug).
- **B10 (M)** `useHotkeyRecorder.ts:175-181` — failed hotkey save returns false without `setError`; recorder hangs in recording mode with no feedback.
- **B11 (M)** `settingsStore.ts:197-204` — `updateLanguage` is the only updater that doesn't emit `settings-changed`.
- **B12 (M)** Locale double-persisted: localStorage is the only read path (`i18n/index.ts:63-73`), Tauri-store `language` is write-only. Clearing localStorage reverts UI to English. Single-source it.
- **B13 (M)** `VoiceControl.tsx:291-314` — stop/transcribe failures swallowed (see Top #2).
- **B14 (M)** All Dictionary/Style pages swallow fetch/add errors (`DictionaryDefault.tsx:21-24,36-42`, `DictionarySites.tsx:30-36`, `DictionaryPerApp.tsx:39-42`, `StylePerApp.tsx:32-35,125`, `StylePerSite.tsx:22-28,42`). Shared error signal + inline banner.
- **B15 (M)** `TranscriptionList.tsx:100-103` — every `transcription-created` event resets pagination to page 1 mid-scroll. Prepend/merge by id instead.
- **B16 (M)** `DictionaryDefault.tsx:116-128` — empty state flashes before first fetch resolves (sibling pages gate on `loading()`; this one doesn't).
- **B17 (M)** `SettingsModal.tsx:100-171` — no `role="dialog"`, no `aria-modal`, no focus trap/restore; close button unlabeled.
- **B18 (M)** `TranscriptionCard.tsx:66-77` — copy button only exists in DOM while hovered; keyboard users can never reach it. Render always, reveal via CSS.
- **B19 (M)** Three dropdowns with zero/divergent keyboard + ARIA support (`settings/Select.tsx`, `StyleSelect.tsx`, inline clone in `MicrophoneStep.tsx:19-84`). One accessible component.
- **B20 (M)** `AddSiteForm.tsx:53-58` — input cleared before `onAdd` resolves; combined with B14 a failed add wipes input with no feedback.

### Medium — i18n
- **B21 (M)** Hold-to-speak strings shipped in English in all 6 non-English locales (`ru.ts:49,164,186-189,216-217` + equivalents in es/zh/de/fr/it; scattered extras: fr `sidebar.style`, it `sidebar.home`, de `update.newVersion`). Key sets are complete — values just untranslated.
- **B22 (M)** ~30 user-visible hardcoded English strings bypass i18n while their keys exist unused: theme labels (`AppearanceSettings.tsx:12-22` vs unused `settings.light/dark/system`), `HotkeySettings.tsx` labels, `AudioSettings.tsx:61,67`, `SettingsModal.tsx:26-32,166`, list empty/error states (`TranscriptionList.tsx:144-157`, all Dictionary/Style pages), onboarding `[STEP_0X]` headers, `[EDIT]/[DEL]/[SAVE]/[CANCEL]` buttons, aria-labels (`VoiceControl.tsx:448,470`, `DotMatrixSpinner.tsx:35`), validation errors (`hotkeyUtils.ts:368,378`, `useHotkeyRecorder.ts:300,308`). Also `LanguageSettings.tsx:14` — "RUSSIAN" in English while others use native names.
- **B23 (M)** ~49 dead i18n keys × 7 locales ≈ 350 lines of dead weight (full list in audit; several correspond exactly to the hardcoded strings above — wire them up).
- **B24 (M)** Manual `.replace("{count}", …)` in 6 call sites while `resolveTemplate` is already wired into `t()`.

### Medium — duplication (~500 removable lines)
- **B25 (M)** `DictionaryPerApp.tsx` (536) vs `DictionarySites.tsx` (338) are ~70% clones; `DictionaryDefault.tsx:130-195` repeats the editable-word-row a third time. Extract `<WordListEditor>` (~350 lines saved).
- **B26 (M)** App-search combobox + `AppRowSkeleton` + `AppIcon` duplicated verbatim between `StylePerApp.tsx` and `DictionaryPerApp.tsx` (~160 lines × 2). Extract `<AppSearchCombobox>`.
- **B27 (M)** Three dropdown implementations (see B19) — consolidate.
- **B28 (M)** `styleLabel` switch duplicated 3× (`StyleDefault.tsx:20-31`, `StylePerApp.tsx:146-157`, `StylePerSite.tsx:61-72`).
- **B29 (M)** `HotkeySettings.tsx` / `HotkeyStep.tsx` each contain two near-identical recorder blocks. Extract `<HotkeyRecorderField>`.
- **B30 (M)** `applyTheme` duplicated (`settingsStore.ts:218-227`, `voice-control.tsx:10-18`); `Dictionary.tsx`/`Style.tsx` same shell twice; `Navigation.tsx:22-54` three copy-pasted blocks.
- **B31 (M)** `useAudioDevices.ts:7` — `error` signal never set (enumeration failures swallowed into `[]` by `settingsStore.ts:150-157`); `SettingsModal.tsx:38,69-72` reimplements the fetch instead of using the hook.
- **B32 (M)** `TranscriptionCard.tsx:7-13` redefines the `Transcription` type instead of importing (already drifted: missing `word_count`).
- **B33 (L)** `MicrophoneStep.tsx:91-93` — `createEffect` tracking nothing; should be `onMount`.
- **B34 (L)** `DictionaryPerApp.tsx:44-53` awaits sequentially what `StylePerApp.tsx:39` correctly does with `Promise.all`.

### Performance
- **B35 (M)** `VoiceControl.tsx:186` — 1 s reposition poll runs forever, 2 IPC calls/sec even while hidden (battery). Reposition on show instead.
- **B36 (M)** `commands/apps.ts:42-58` — `installedAppsCache` never invalidated; newly installed apps invisible until restart.
- **B37 (M)** `settingsStore.ts:67-82` — 9 sequential store round-trips per `loadSettings`, re-run in full on every `settings-changed` in VoiceControl.
- **B38 (M)** `TranscriptionList.tsx` — unbounded DOM growth (no virtualization) + O(n²) group rebuild + `getDateLabel` omits year.
- **B39 (M)** `favicons.ts:4` — favicons fetched from Google (leaks configured site domains) and eagerly preloaded at every launch (`App.tsx:114-123`); `index.html`/`voice-control.html` load Inter from Google Fonts at runtime (network dependency in a desktop app, blank fonts offline). Proxy/cache favicons; bundle the font.

### Low
- **B40 (L)** `HotkeySettings.tsx:41-44,78-81` — a stale validation error on one recorder disables the *other* recorder's Change button. Key on `isRecording` only.
- **B41 (L)** `autofocus` on dynamically-inserted edit inputs unreliable in WKWebView (`DictionaryDefault.tsx:145`, `DictionaryPerApp.tsx:433`, `DictionarySites.tsx:278`). Use ref + `.focus()`.
- **B42 (L)** Unhandled rejections: `SettingsModal.tsx:64,77`, `TranscriptionCard.tsx:25`, `OnboardingWizard.tsx:41,54`, module-level calls in `voice-control.tsx:21-26`.
- **B43 (L)** Un-cleared timeouts firing on disposed components: `TranscriptionCard.tsx:31`, `SettingsModal.tsx:66`, `LearningStep.tsx:60-63`.
- **B44 (L)** `settingsStore.ts:109-113,169` — `saveBrowserValue` Result silently dropped (theme FOUC mirror can silently stop updating).
- **B45 (L)** `settingsStore.ts:44-65` — `getStore()` caches the resolved value, not the promise; concurrent init double-loads.
- **B46 (L)** `settingsStore.ts:238-243` — onboarding uses explicit `save()`, everything else relies on `autoSave: 500` (quit within 500 ms loses changes). Pick one.
- **B47 (L)** `posthog.ts:5,27` — hardcoded key with a dead `if (!POSTHOG_KEY)` guard; no opt-out (see Top #6).
- **B48 (L)** `VoiceControl.tsx:443` — `align-center` isn't a Tailwind class (meant `items-center`).
- **B49 (L)** `tailwind.config.js` — `primary`/`midnight`/`accent` palettes (~40 lines) have zero usages.
- **B50 (L)** `hotkeyUtils.ts:353-360` — `"CommandOrControl"` renders as `"⌘Or⌃"`.
- **B51 (L)** `DictionaryPerApp.tsx:37` / `StylePerApp.tsx:30` — `optionRefs` Map never pruned as filters shrink.
- **B52 (L)** `SettingsModal.tsx:42-60` — both hotkey recorders (4 permanent Tauri listeners) instantiated at app start because the modal is always mounted. Lazy-create.

---

## C. Product UX (beyond the code bugs above)

### High
- **C1 (H)** Denied mic permission is a hard dead-end in onboarding (see Top #10).
- **C2 (H)** Model download UX: no resume, no cancel, no speed/ETA/bytes — just a percentage; "Retry" restarts a 1.5 GB download from 0% (`ModelDownloadStep.tsx:93-98,120-126` + backend, Top #7).
- **C3 (H)** Missing-mic fallback: hotkey with an unplugged saved device does literally nothing (`audio.rs:99-102` errors, `VoiceControl.tsx:410` hides). Fall back to system default input + one-line pill error.
- **C4 (H)** Accessibility revocation post-onboarding: transcribes but never types, no prompt to re-grant (dead `accessibility-permission-needed` event, Top #2).

### Medium
- **C5 (M)** Hotkey validation is structural only — no deny-list for system shortcuts (users can bind `Cmd+Q`); identical toggle + hold-to-speak hotkeys silently drop hold-to-speak (`hotkeyUtils.ts:366-381`, `VoiceControl.tsx:172`).
- **C6 (M)** Escape is registered as a *system-wide* shortcut for the whole duration of every recording (`VoiceControl.tsx:347-354`) — swallowed from the frontmost app, and nothing tells the user Escape = cancel. Observe via the NSEvent watcher instead; show "ESC to cancel" in the pill.
- **C7 (M)** Any regular keypress during hold-to-speak silently cancels and discards the recording (`VoiceControl.tsx:252-259`). Stop-and-transcribe instead, or flash a cancelled state.
- **C8 (M)** Home hotkey hint hardcoded to the default hotkey (see Quick wins).
- **C9 (M)** Accessibility onboarding step force-marks granted after 4 failed probes because the distributed notification fires for *any* app's AX toggle (`AccessibilityPermissionStep.tsx:61-68`) — defers failure to a silent typing error later.
- **C10 (M)** Failed practice transcription in the learning step shows nothing; `onboarding.learningError` exists but is never used (`LearningStep.tsx:58-64`).
- **C11 (M)** Overlay pill positioned against full monitor bounds (not work area) — sits in the Dock's region; not movable, no position preference (`VoiceControl.tsx:51,93-96`). (Verified good: follows cursor's monitor, never steals focus, accepts first mouse.)
- **C12 (M)** No transcription-language override — auto-detect only, and the UI-language setting explicitly doesn't affect it (`whisper.rs:191-199`); misfires on short/accented utterances with no recourse.
- **C13 (M)** Model download state is component-local: closing Settings or navigating the wizard shows "Download" again while a download streams (also corruption risk, A17). Backend-derived state.

### Low
- **C14 (L)** Tray mic submenu empty until any settings change (`tray.rs:48` builds with `vec![]`).
- **C15 (L)** No recording indicator in the tray — the icon is static; the pill is the only ambient mic-live signal.
- **C16 (L)** History: copy-only — no delete, no search, no full-text expand (truncated at 150 chars). Sensitive history can't be removed.
- **C17 (L)** Dictionary/Style: no bulk add, import/export anywhere — words entered one at a time; no backup/migration path.
- **C18 (L)** Dictionary/Style never introduced: onboarding ends at "You're all set" with no pointer to the differentiating features.
- **C19 (L)** No sleep/wake handling — hold-to-speak spanning sleep loses its key-release event and records silence indefinitely. Observe `NSWorkspace.willSleepNotification`.
- **C20 (L)** Raw backend error strings shown verbatim and untranslated (`ModelDownloadStep.tsx:53`, `ModelSettings.tsx:184-186`, `useHotkeyRecorder.ts:308`). Map failure classes to translated messages.
- **C21 (L)** Closing the main window hides to tray with no hint the app (and mic hotkey) keeps running (`lib.rs:169-180`). One-time "keeps running in the menu bar" notice.

**Verified good:** onboarding progress persists per-step across quits; Back navigation exists; deleted/corrupt model auto-resumes onboarding at the download step; settings save immediately and say so; diagnostics are metadata-only into a 5 MB-capped local log; updates are opt-in, never forced.

---

## D. Marketing site

### High
- **D1 (H)** Hero stat "0 data sent to servers" is factually false given PostHog + Umami (`i18n/ru/hero.ts:9-11`). Change to "0 *audio* sent to servers".
- **D2 (H)** Wrong hotkey taught in three places — `Cmd+;` vs actual `Ctrl+Option` (see Top #11).
- **D3 (H)** `nginx.conf` has zero security headers (no HSTS, CSP, nosniff, frame-ancestors, referrer-policy, permissions-policy). Note `add_header` inside the asset `location` suppresses inherited headers — repeat them there.
- **D4 (H)** Terms §2.2 prohibits decompiling/competing products; the product is MIT-licensed with a public repo link on the same homepage. Rewrite to defer to MIT.
- **D5 (H)** "No internet required after install" / "100% offline after install" — models download *after* install. Say "after the one-time model download".
- **D6 (H)** Privacy policy never names PostHog or Umami, doesn't mention the persistent `distinct_id` in localStorage, offers no opt-out.

### Medium
- **D7 (M)** `Layout.astro` is imported by no page — all six pages hand-roll `<html>`; the mail.ru site-verification meta inside it never ships (verification silently broken), and the file is dead code.
- **D8 (M)** Dockerfile omits `packages/app/package.json` from the workspace install and lacks `--frozen-lockfile` — non-reproducible builds.
- **D9 (M)** Two conflicting deploy paths (wrangler scripts vs Docker/nginx/Railway); if Cloudflare Pages is real, no `_headers`/`_redirects` exist. Pick one.
- **D10 (M)** `listen ${PORT};` with unset PORT → invalid `listen ;`, crash-loop; no `EXPOSE`/`HEALTHCHECK`. Default the port in CMD.
- **D11 (M)** HTML gets no `Cache-Control` — stale pages can reference purged hashed assets post-deploy. `no-cache` for HTML + `gzip_vary on`.
- **D12 (M)** Render-blocking Google Fonts CSS on every page; Space Mono requested (2 weights) but only ever a fallback. Self-host JetBrains Mono woff2 with `font-display: swap`; drop Space Mono.
- **D13 (M)** No `prefers-reduced-motion` anywhere — infinite `blink`/`wave`/`scan` (40 animated bars) and 404 `glitch`/`pulse` run unconditionally.
- **D14 (M)** ~1,000 lines of duplicated nav/footer/base CSS across the six pages, already drifted (`.logo-bracket` is `#fff` on index, `#ff3e00` on download). Extract `SiteHeader`/`BaseLayout`.
- **D15 (M)** WCAG contrast failures: `#666` body text on `#0a0a0a` ≈ 3.4:1 (needs 4.5:1) across features/cards/stats/footer; 404 `.timestamp` `#444` on `#111` ≈ 1.9:1.
- **D16 (M)** `Downloads.astro:18,23` — decorative `<h2>DOWNLOADS</h2>` precedes the page `<h1>` (broken heading order) and is hardcoded untranslated.
- **D17 (M)** Mobile menu button lacks `aria-expanded`/`aria-controls` and the menu contains a single "Features" link.
- **D18 (M)** Download cards: no version, file size, min macOS version, checksum, or "signed & notarized" note; no Apple Silicon vs Intel detection/hint. Inject release metadata at build time.
- **D19 (M)** Missing pages: changelog, FAQ, support/contact (email buried in legal body text; footer About/Contact keys exist unrendered).
- **D20 (M)** Site is English-only while the app ships 7 locales; i18n scaffolding (`getLocalizedPath`, `ogLocales.ru`) sits half-built with no hreflang. Localize top locales or delete the dead plumbing.
- **D21 (M)** No product visuals anywhere — the hero is a fake terminal for a GUI app; no screenshots or demo video.
- **D22 (M)** Legal pages governed exclusively by Russian law (152-FZ, §6.4 disclosure to Russian authorities) for a globally-marketed product; no GDPR/CCPA; operator identity conflicts with LICENSE attribution.

### Low
- **D23 (L)** `public/og.svg` is dead (only og.png referenced). Delete.
- **D24 (L)** `expires 1y; immutable` applies to non-hashed `favicon.svg`/`og.png`; scope immutable caching to `/_astro/`.
- **D25 (L)** `try_files` serves `/download` and `/download/` both as 200 — add a 301 to the canonical trailing-slash form.
- **D26 (L)** 404 page emits both `noindex` and a canonical URL — conflicting signals; skip canonical when noindex.
- **D27 (L)** SoftwareApplication schema always uses SITE_URL, omits author/version/screenshot/availability; sitemap lacks lastmod.
- **D28 (L)** `twitter:site @voxfusion` unverifiable — drop if the account doesn't exist.
- **D29 (L)** Umami production website ID is the hardcoded *fallback* — every dev/preview build reports to production analytics. Env-only, render nothing when absent.
- **D30 (L)** Commented-out Windows section still ships an empty spaced `<div>` + dead translation keys.
- **D31 (L)** English source strings live in a directory named `src/i18n/ru/` feeding the `en` table — maintenance trap; rename to `en/`.
- **D32 (L)** `© 2025` hardcoded (stale vs "Last updated: May 11, 2026" legal pages). Use build-time year.
- **D33 (L)** Static green-dot "All systems operational" with no status page (untranslated on 404). Remove or link something real.
- **D34 (L)** 404's fake error log renders the *build* timestamp as if live.
- **D35 (L)** Download page nav CTA labeled "Features →" links to `/` not `/#features`.
- **D36 (L)** index/download lack a `<main>` landmark; no skip link; no custom `:focus-visible` styles anywhere.
- **D37 (L)** Decorative glyphs (`⌘;`, `⚡`, `📖`, `⏱`, `↓`, Apple SVG) lack `aria-hidden`.
- **D38 (L)** Footer responsive rule sets grid columns on a flex container — does nothing.
- **D39 (L)** `nanoid` dependency never imported. Remove.
- **D40 (L)** Terms §8.1 promises email notice 14 days before changes; §3.2 says no email is ever collected. Internal contradiction.

**Verified consistent:** download URLs exactly match release artifacts; robots/sitemap/canonicals all agree; 404 correctly noindexed + excluded from sitemap; og.png is a genuine 1200×630.

---

## E. Tooling, CI, docs

### High
- **E1 (H)** No CI on PRs at all; release pipeline has zero quality gates (see Top #12).
- **E2 (H)** `bun run tauri:dev` documented in README:50, CONTRIBUTING:33, AGENTS:20 — doesn't exist (see Quick wins).
- **E3 (H)** Root cause of ~1157 `astro check` errors: marketingsite tsconfig inherits root's repo-wide include + React JSX (see Quick wins).
- **E4 (H)** Effectively no test infrastructure: zero JS/TS tests, one Rust test module (`browser.rs:157-207`) that nothing ever runs — while CONTRIBUTING instructs contributors to add tests. Add vitest/`bun test` + `cargo test` in CI.

### Medium
- **E5 (M)** README describes single-model Whisper; app is dual-engine (Whisper + Parakeet/CrispASR, `models.rs:45-75`). Document both + the engine build script.
- **E6 (M)** README stack table says Wrangler; real deploy is Docker/nginx/Railway; wrangler scripts can't run (no wrangler.toml). Update + delete stale dep (also kills the esbuild 0.17 advisory line in bun.lock).
- **E7 (M)** README "no servers" vs embedded PostHog (see Top #6) — disclose in README.
- **E8 (M)** Rust ≥1.85 requirement (edition 2024) undocumented in README and unenforced — add `rust-version = "1.85"` to Cargo.toml.
- **E9 (M)** Version drift: tauri.conf.json 0.3.0 vs package.json/Cargo.toml 0.1.0 (release.yml patches only tauri.conf.json in CI). One bump script or release-please.
- **E10 (M)** `bun install` unfrozen in release.yml:40 (and Dockerfile). `--frozen-lockfile`.
- **E11 (M)** `MACOSX_DEPLOYMENT_TARGET: 11.0` in release vs `minimumSystemVersion: 10.15` in tauri.conf — Catalina users can install Big Sur+ binaries. Align (realistically 11.0).
- **E12 (M)** Dead `VITE_API_URL: https://voxfusion-production.up.railway.app` in release env — zero usages, contradicts "no servers". Delete.
- **E13 (M)** No changelog automation; updater manifest hardcodes "See release notes on GitHub". `generate_release_notes: true` is one line.
- **E14 (M)** `tauri.conf.json:15` hardcodes your personal signing identity — `tauri:build` fails on any other machine. Remove; rely on the `APPLE_SIGNING_IDENTITY` env CI already injects.
- **E15 (M)** No Dependabot/Renovate (`bun`, `cargo`, `github-actions` ecosystems).
- **E16 (M)** Biome .astro override misses the rules that actually fire (see Quick wins).
- **E17 (M)** Root tsconfig sets `jsx: "react-jsx"` in a Solid monorepo — co-culprit of E3. Remove from root.
- **E18 (M)** turbo.json: `build` outputs list `src-tauri/target/**` (wrong — vite writes `dist/`; would cache GB of Rust artifacts if it ever matched); dead `check`/`format` tasks with no matching scripts; lint/typecheck don't hash `biome.json`/`tsconfig.json`; no `test` task; `build` declares no `env: ["VITE_*"]` so turbo can serve stale builds after env changes.
- **E19 (M)** No pre-commit hooks; combined with E1 nothing catches drift before review. lefthook + `biome check --staged`.
- **E20 (M)** `.env.example` documents 1 of 4 used env vars — missing `PUBLIC_UMAMI_WEBSITE_ID`, `PUBLIC_UMAMI_SCRIPT_URL`, `VOXFUSION_PARAKEET_BIN`.
- **E21 (M)** `.railwayignore` excludes neither `packages/app/` nor `target/` — Railway uploads can include the multi-GB Rust target dir. Mirror `.dockerignore`.
- **E22 (M)** Pinning inconsistent: `"@types/bun": "latest"`, `"wrangler": "^3"`, mixed `~`/`^` tauri plugins; two TS versions (5.7.3 + 5.9.3) and three esbuild versions (incl. 0.17.19 with a known dev-server advisory) in bun.lock.

### Low
- **E23 (L)** AGENTS.md stale counts (says ~1098 errors; now 1157) and garbled opening line ("If you got some instingt…"). Fix E3/E16 and delete the caveats.
- **E24 (L)** `Cargo.toml:5` — `authors = ["you"]`.
- **E25 (L)** Bun version guidance inconsistent (README "v1.0+", CONTRIBUTING "1.3.3+", root pins 1.3.3).
- **E26 (L)** release.yml grants unused `actions: write`; no bun cache; sequential dual-target build with no `concurrency:` group.
- **E27 (L)** Dead path aliases: root `@voxfusion/app`, marketingsite `@/*` — imported nowhere.
- **E28 (L)** Biome a major behind (1.9.4; 2.x fixes the .astro false positives durably).
- **E29 (L)** `.sc/` only in local git exclude, `.cursor/` not ignored at all; generated Tauri schemas (~400 KB, `src-tauri/gen/schemas/`) tracked in git — diff noise.

**Verified good:** updater private key via secrets with pubkey pinned in config; signing → notarization → stapling → Gatekeeper verify all present in release.yml; no build artifacts tracked in git; MIT license fields consistent across package.json files; issue/PR templates well-structured.
