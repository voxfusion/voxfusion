If you discover an insight useful for future agents while working on a task, please write it down.

Use bun instead of Node.js, npm, pnpm, or vite.

## Cursor Cloud specific instructions

### Overview

VoxFusion is a Bun/Turborepo monorepo with two packages:

- `@voxfusion/app` — Tauri v2 desktop app (SolidJS + Rust). **macOS-only** for full native builds.
- `@voxfusion/marketingsite` — Astro static marketing website.

### Running services

| Service | Command | Port | Notes |
|---------|---------|------|-------|
| App frontend (Vite) | `cd packages/app && bunx vite --host 0.0.0.0` | 1420 | SolidJS UI only; Tauri IPC unavailable in standalone browser mode |
| Marketing site (Astro) | `cd packages/marketingsite && bun run dev -- --host 0.0.0.0` | 4321 | Fully functional on Linux |
| Full Tauri app | `cd packages/app && bun run dev` | — | Requires macOS (Metal, accessibility, tray) |

### Lint / typecheck / build

Standard commands documented in `README.md` scripts section. Key notes:

- `bun run check` — runs Biome across the whole repo.
- `bun run --filter @voxfusion/app typecheck` — passes clean.
- `bun run --filter @voxfusion/marketingsite typecheck` — runs `astro check`.
- `bunx vite build` (from `packages/app`) — builds the frontend successfully on Linux.
- `bun run build` (from `packages/marketingsite`) — builds all 6 static pages successfully.

### Rust / Tauri backend on Linux

`cargo check` in `packages/app/src-tauri` **will fail** on Linux because:

1. `whisper-rs` is compiled with `features = ["metal"]` which requires macOS Metal framework.
2. Several dependencies (`core-graphics`, `objc2`, `objc2-app-kit`) are macOS-only.

For frontend-only development on Linux, use the Vite dev server directly (`bunx vite`). The Rust backend requires macOS for compilation.

To type-check shared Rust code on Linux anyway, check a scratch copy of `src-tauri` with `metal` removed from `whisper-rs` and the macOS-only window/run-event calls (`title_bar_style`, `hidden_title`, `RunEvent::Reopen`) stubbed out. macOS-only FFI can be checked in a small scratch crate with `cargo check --target aarch64-apple-darwin`; checking does not link, so no macOS SDK is needed. CI runs the real `cargo check` on macOS.

### System dependencies (Linux)

Required for `cargo check` to proceed as far as possible (before the macOS-specific failure):

```
libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf libxdo-dev libssl-dev libasound2-dev
```

Also requires `libstdc++.so` symlink: `sudo ln -sf /usr/lib/gcc/x86_64-linux-gnu/13/libstdc++.so /usr/lib/x86_64-linux-gnu/libstdc++.so`

### System dependencies (macOS)

Required for `bun dev` / Tauri builds (whisper-rs-sys uses CMake to compile whisper.cpp):

```
brew install cmake
```

Without CMake, `bun dev` fails during `whisper-rs-sys` with `is cmake not installed?`.

### Gotchas

- Railway builds the marketing Dockerfile from the repository root. Keep `packages/app/package.json` available in `.dockerignore` and copy `patches/` before `bun install --frozen-lockfile`; the workspace lockfile requires both even when building only the marketing site.

- The marketing site's English strings live in `packages/marketingsite/src/i18n/ru/`; `translations.ts` maps those modules to the `en` locale despite the directory name. The homepage waveform uses deterministic heights and unitless animation scales; percentage values are invalid for `scaleY()`.

- The marketing site is served from `voxfusion.io` (Railway project `voxfusion.io`, redeployed on every push to `main`). `voxfusion.com` has no DNS records, so canonical URLs, the sitemap, Open Graph tags, and contact addresses must use `.io`. Fonts are self-hosted through `@fontsource-variable/jetbrains-mono`; the site should make no third-party requests besides Umami.

- Landing page blocks marked `data-reveal` fade in on scroll (`BaseLayout.astro`); the hidden state is gated on `html.js`, so they stay visible without JavaScript. The reveal is a `backwards`-filled animation, not a transition, so a component's own `transition` or hover `transform` cannot collide with it; stagger siblings with `--reveal-delay`, and key child effects off the ancestor's `.is-visible`. Astro adds its scope attribute to every compound selector, so a scoped `.table tbody td` outranks `.table .modifier`. Umami's `data-umami-view` fires at 30% visibility, which a tall section never reaches on phones; put it on the section intro. When scripting full-page screenshots, scroll with `behavior: "instant"`: the site's `scroll-behavior: smooth` makes repeated `scrollTo` calls go nowhere, leaving reveal blocks blank. Emulate `prefers-reduced-motion: reduce` to capture every animation in its finished state.

- Typed-text effects on the landing page go through `src/scripts/typewriter.ts`, which keeps the untyped remainder in the flow (invisible) so lines never re-wrap mid-animation. That only covers one text; `HeroDemo.astro` cycles samples of different lengths, so every sample must still fit the `min-height` reserved for `.typed` and `.heard` at each breakpoint, or the hero changes height while the demo loops. Each animated block is server-rendered in its finished state for no-JS and reduced-motion visitors.

- `public/og.png` is rendered from `public/og.svg` at 1200×630 in a browser with JetBrains Mono loaded (the SVG alone falls back to a system font). After changing it, bump the `?v=` query in `Seo.astro`, because social networks cache the image by URL.

- Site copy must match what ships: release builds set `MACOSX_DEPLOYMENT_TARGET` to 11.0 (`release.yml`) even though `tauri.conf.json` still says 10.15, Parakeet covers 25 European languages, styles only affect Whisper, and the updater asks before installing.

- Rust toolchain must be ≥1.85 (edition 2024 support). Run `rustup update stable && rustup default stable`.
- Parakeet supports dictionary biasing through CrispASR's `--hotwords` argument in the pinned engine. It does not support Whisper's prose style prompts. Resolve global/app/site vocabulary before engine dispatch and keep successful-recording retention in the shared transcription handler. Site icons are deliberately generated locally; external favicon services disclose configured domains.
- The app frontend at localhost:1420 shows a loading spinner and never progresses in a browser because it waits for the Tauri IPC bridge. Every route uses the same `App` root, so direct route navigation does not bypass this gate. Use a native build or an explicit Tauri test bridge for UI testing.
- For native QA, pass a separate `identifier` and `productName` through Tauri's `--config` JSON to isolate settings, SQLite history, models, logs, and single-instance behavior. An existing model file can be cloned into that profile without copying personal history. A `--no-sign` debug bundle has only the linker's signature; bind its app identifier and entitlements with an ad-hoc bundle signature before testing macOS permissions. Permission prompts and OS authentication may require the user to complete the system dialog. The September 2026 review and test scope are recorded in `review-output/review.md`.
- Tao applies its activation policy in `applicationDidFinishLaunching`, and its default `Regular` overrides `Info.plist`'s `LSUIElement`; releases through v0.9.14 showed a Dock icon despite it. `lib.rs` sets `Accessory` on the built `App` before `run`, because calling it in `setup` runs after launch and flashes the Dock icon. Accessory apps have no menu bar, so menu-bar-only actions also need a tray item. A GitHub `macos-15` runner can verify this without a Mac: `NSRunningApplication.activationPolicy` and `lsappinfo info -only ApplicationType` (`UIElement` vs `Foreground`), with `screencapture` and System Events available. A Dock icon without a running dot is the "recent apps" entry from an earlier Regular launch, not a running app.
- The Parakeet engine compile takes ~2h in CI, so releases depend on the `crispasr-engine-macOS-<build script hash>` cache existing on `main` (tag caches are invisible to other tags). GitHub evicts caches unread for 7 days, which silently made v0.9.12–v0.9.15 recompile it. `engine-cache.yml` now reads it daily, and both workflows restore the engine from the newest release built from the same `build-parakeet-engine.sh` (its app bundles carry the engine in `Contents/Resources/bin`) before compiling. Check with `gh cache list --key crispasr-engine` before tagging.
- `turbo dev` runs both packages' dev scripts concurrently (uses TUI mode).
- Production macOS logs are written to `~/Library/Logs/io.voxfusion.app/voxfusion.log`. Tauri's file timestamps are UTC even when macOS is in another timezone; correlate them with `log show`/wall-clock times accordingly. For hidden-window shortcut failures, also inspect the macOS unified log's `com.apple.WebKit:ProcessSuspension` events: the shortcut callbacks currently live in the hidden `voice-control` webview, and `LSUIElement=true` builds can place its WebContent process in the background suspension lifecycle. Releases through v0.9.10 also accumulated `PRESSED_KEYS` from transitions without resynchronizing after activation, screen lock, or sleep/wake; the watcher now rebuilds state from each event's modifier flags and logs `system_key_state_resynchronized` when it repairs a discontinuity.
- Modifier-only hotkeys must read the side-specific `NX_DEVICE*KEYMASK` bits from `NSEvent.modifierFlags` without masking off device-dependent flags. On this laptop, `CGEventSourceKeyState` reported `{LeftCommand}` for right Command events, so `RightCommand` hold-to-speak never matched. Polling live state during event delivery can also read ahead of queued events. Use `CGEventSourceFlagsState` only for startup/lifecycle snapshots, and event flags for transitions.
- Cuelume 0.2.2 normally skips playback until `navigator.userActivation.hasBeenActive` is true. Native global-shortcut callbacks do not activate the hidden `voice-control` webview, even though Tauri enables media autoplay. Keep the tracked `patches/cuelume@0.2.2.patch` in place for shortcut-triggered recording cues. Play the start cue before microphone setup because `muteMediaForRecording` mutes the default output device and can cut off a cue scheduled immediately beforehand.
- Microphone hot-plugging: cpal 0.15's CoreAudio disconnect listener held a strong reference to its own stream, so streams on an explicitly selected mic were never dropped and their stale listeners kept calling the recorder's error callback. cpal 0.17 fixes that cycle, but it still ignores a disconnect of the device that is the system default input; the stream just stops delivering samples. The recorder therefore owns one id-tagged recording at a time and fails it from an input watchdog. It closes CoreAudio streams on a detached thread so a slow teardown of a just-removed device cannot wedge stop or the next start. In cpal 0.17, `description()` (and the deprecated `name()`) opens AudioUnits to count configs, so device enumeration is not free.
- "Mute media while recording" must unmute the device it muted, not whatever is the default output at restore time: plugging or unplugging a headset mid-recording moves the default. Devices unplugged while muted are unmuted when CoreAudio reports them again. The cue `AudioContext` in the long-lived `voice-control` webview is reopened after `audio-devices-changed` (see the cuelume patch's `resetAudioContext`).
- "Muffle media while recording" lowers the default output's virtual main volume (`vmvc`, the value behind the system volume slider) through `AudioObjectGetPropertyData`/`SetPropertyData`; the `AudioHardwareService*` wrappers are deprecated. One worker thread in `handlers/media/muffle.rs` owns every volume change and fade, so the Tauri commands only post a message and cannot delay recording start. Its state machine runs against a fake output in `cargo test`; on Linux, copy `muffle.rs` into a scratch crate with a stub `CoreAudioVolume` to run those tests. CoreAudio may apply volume changes asynchronously, so do not detect user volume changes by comparing read-back values: a false match would leave the output turned down.
- Many outputs have no `mute`, `vmvc`, or per-channel `volm` property at all, under any API (`AudioObject*` or `AudioHardwareService*`): USB audio interfaces with a hardware knob (Arturia MiniFuse) and DisplayPort/HDMI displays. CoreAudio answers `'who?'` (2003332927), and the macOS volume slider is greyed out for them. `handlers/media/tap.rs` handles these outputs with a process tap (macOS 14.2+, looked up at runtime). The tap covers the output's first stream, excludes the app's own process, and uses `CATapMutedWhenTapped`. A private aggregate device plays the tap back at a cubic gain; the tap is the last input buffer, after the output device's own inputs. A `CATapMuted` tap that nobody reads mutes nothing. Reading the tap prompts for System Audio Recording (`NSAudioCaptureUsageDescription`). If that permission is missing, the tap delivers silence but still holds the audio back, so muffle on these outputs mutes instead. Killing the process restores the audio at once. An Aggregate or Multi-Output Device used as the output cannot be nested in the playback device, which then has no output streams. Mute still works there, but muffle is refused instead of silencing the output.
- Measuring real output for audio QA: shell commands spawned by the agent host inherit its TCC identity, which has no microphone access, so input reads all zeros. Wrap the meter in a small ad-hoc signed `.app` with `NSMicrophoneUsageDescription` and launch it with `open`. In clamshell mode the built-in microphone is disconnected and also reads zeros. A narrowband 1 kHz detector on another microphone picks up the built-in speakers, and even headphone leakage, clearly. Every ad-hoc re-sign or rebuild changes the code hash, so macOS prompts again (microphone, System Audio Recording); batch rebuilds. Signing with the Apple Development identity blocks on a keychain prompt. In zsh, `log` is a shell builtin; use `/usr/bin/log show`. Synthesized key presses do not reach the dictation shortcuts. For scripted dictations, add a temporary trigger outside the commit: a setup thread watches a file and emits an event, and the `voice-control` webview answers it with `toggleRecording()`.
- Voice overlay positioning must use live monitor dimensions and the actual window position. Disconnecting an external display can leave the MacBook display at the same origin, so caching only the monitor origin misses the change. Keep `setPosition` in logical coordinates on macOS: Tao converts physical positions using the window's current scale, which may still belong to the previous display.
