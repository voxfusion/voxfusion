If you discover an insight useful for future agents while working on a task, please write it down.

Use bun instead of Node.js, npm, pnpm, or vite.

## Cursor Cloud specific instructions

### Overview

VoxFusion is a Bun/Turborepo monorepo with two packages:

- `@voxfusion/app` — Tauri v2 desktop app (SolidJS + Rust). Native builds support macOS and Linux; Omarchy/Hyprland is the primary Linux target.
- `@voxfusion/marketingsite` — Astro static marketing website.

### Running services

| Service | Command | Port | Notes |
|---------|---------|------|-------|
| App frontend (Vite) | `cd packages/app && bunx vite --host 0.0.0.0` | 1420 | SolidJS UI only; Tauri IPC unavailable in standalone browser mode |
| Marketing site (Astro) | `cd packages/marketingsite && bun run dev -- --host 0.0.0.0` | 4321 | Fully functional on Linux |
| Full Tauri app | `cd packages/app && bun run dev` | — | macOS or Linux with the native dependencies below |

### Lint / typecheck / build

Standard commands documented in `README.md` scripts section. Key notes:

- `bun run check` — runs Biome across the whole repo.
- `bun run --filter @voxfusion/app typecheck` — passes clean.
- `bun run --filter @voxfusion/marketingsite typecheck` — runs `astro check`.
- `bunx vite build` (from `packages/app`) — builds the frontend successfully on Linux.
- `bun run build` (from `packages/marketingsite`) — builds all 6 static pages successfully.

### Rust / Tauri backend on Linux

`cargo check` and `cargo test` now run on Linux. Metal and macOS permissions are target-specific dependencies. Linux uses CPU Whisper (Base by default), `wtype` for Wayland typing and temporary Hyprland bindings forwarding through a private Unix socket. See `docs/linux.md`; build the Arch archive with `bun run --filter @voxfusion/app build:linux`.

### System dependencies (Linux)

Ubuntu/Debian native development dependencies:

```
libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf libxdo-dev libssl-dev libasound2-dev cmake clang
```

Omarchy/Arch: `base-devel rust cmake clang pkgconf webkit2gtk-4.1 libappindicator-gtk3 alsa-lib libpulse xdotool wtype`. A stale Omarchy snapshot may have package URLs returning 404; exact packages from archive.archlinux.org avoid a partial system upgrade.

### System dependencies (macOS)

Required for `bun dev` / Tauri builds (whisper-rs-sys uses CMake to compile whisper.cpp):

```
brew install cmake
```

Without CMake, `bun dev` fails during `whisper-rs-sys` with `is cmake not installed?`.

### Gotchas

- Railway builds the marketing Dockerfile from the repository root. Keep `packages/app/package.json` available in `.dockerignore` and copy `patches/` before `bun install --frozen-lockfile`; the workspace lockfile requires both even when building only the marketing site.

- The marketing site's English strings live in `packages/marketingsite/src/i18n/ru/`; `translations.ts` maps those modules to the `en` locale despite the directory name. The homepage waveform uses deterministic heights and unitless animation scales; percentage values are invalid for `scaleY()`.

- Rust toolchain must be ≥1.85 (edition 2024 support). Run `rustup update stable && rustup default stable`.
- Parakeet supports dictionary biasing through CrispASR's `--hotwords` argument in the pinned engine. It does not support Whisper's prose style prompts. Resolve global/app/site vocabulary before engine dispatch and keep successful-recording retention in the shared transcription handler. Site icons are deliberately generated locally; external favicon services disclose configured domains.
- The app frontend at localhost:1420 shows a loading spinner and never progresses in a browser because it waits for the Tauri IPC bridge. Every route uses the same `App` root, so direct route navigation does not bypass this gate. Use a native build or an explicit Tauri test bridge for UI testing.
- For native QA, pass a separate `identifier` and `productName` through Tauri's `--config` JSON to isolate settings, SQLite history, models, logs, and single-instance behavior. An existing model file can be cloned into that profile without copying personal history. A `--no-sign` debug bundle has only the linker's signature; bind its app identifier and entitlements with an ad-hoc bundle signature before testing macOS permissions. Permission prompts and OS authentication may require the user to complete the system dialog. The September 2026 review and test scope are recorded in `review-output/review.md`.
- `turbo dev` runs both packages' dev scripts concurrently (uses TUI mode).
- Production macOS logs are written to `~/Library/Logs/io.voxfusion.app/voxfusion.log`. Tauri's file timestamps are UTC even when macOS is in another timezone; correlate them with `log show`/wall-clock times accordingly. For hidden-window shortcut failures, also inspect the macOS unified log's `com.apple.WebKit:ProcessSuspension` events: the shortcut callbacks currently live in the hidden `voice-control` webview, and `LSUIElement=true` builds can place its WebContent process in the background suspension lifecycle. Releases through v0.9.10 also accumulated `PRESSED_KEYS` from transitions without resynchronizing after activation, screen lock, or sleep/wake; the watcher now rebuilds state from `CGEventSourceKeyState` and logs `system_key_state_resynchronized` when it repairs a discontinuity.
- Cuelume 0.2.2 normally skips playback until `navigator.userActivation.hasBeenActive` is true. Native global-shortcut callbacks do not activate the hidden `voice-control` webview, even though Tauri enables media autoplay. Keep the tracked `patches/cuelume@0.2.2.patch` in place for shortcut-triggered recording cues. Play the start cue before microphone setup because `muteMediaForRecording` mutes the default output device and can cut off a cue scheduled immediately beforehand.

- Linux QA: terminal tools may omit the active GUI environment. Discover `hyprctl instances -j` and the sockets under `$XDG_RUNTIME_DIR`, then pass `WAYLAND_DISPLAY`, `DISPLAY`, and `HYPRLAND_INSTANCE_SIGNATURE` explicitly. Use isolated XDG data/config/cache directories and explicit PipeWire links from a temporary source to the app capture stream to test a known speech sample without changing the user's microphone selection. `wtype`'s synthetic keymap may not trigger Hyprland global shortcuts; an evdev/uinput keyboard does. `wf-recorder` can record the actual native window on a separate workspace.
- `whisper-rs-sys` forwards `GGML_*` environment flags but its Cargo cache does not always invalidate when those flags change. When switching an existing target directory from a native CPU build to a portable one, run `cargo clean -p whisper-rs-sys --release` once from `packages/app/src-tauri`, then rebuild and inspect its `CMakeCache.txt`. Fresh builds use the tracked `.cargo/config.toml` (`GGML_NATIVE=OFF`).
- Hyprland 0.52 move rules only subtract offsets with the `100%-` anchor; `50%-130` is accepted but ignores the subtraction. The Linux overlay uses the documented `100%-w-20` bottom/right anchors. Hyprland 0.53+ requires the newer window-rule effect names and expression syntax; select the rules by compositor version.

- User preference: do not add a browser extension for Linux website detection.
