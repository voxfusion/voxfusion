# Contributing to VoxFusion

Thanks for helping improve VoxFusion. This project is a local-first desktop
transcription app: a native Rust app built on GPUI (through GPUI Kit), with
Whisper and Parakeet for transcription and SQLite for storage. The marketing
site is a separate Astro project in the same repository.

## Project Priorities

- Keep transcription local-first and offline after the first model download.
- Preserve user privacy: avoid sending transcript, audio, dictionary, or device
  data to third-party services.
- Prefer small, reviewable changes over broad rewrites.
- Match the existing code style and user interface patterns.

## Development Setup

Install the required tools:

- [Rust](https://rustup.rs/) 1.85 or newer
- [CMake](https://cmake.org/) (whisper.cpp is compiled from source; on macOS:
  `brew install cmake`)
- [Bun](https://bun.sh/) 1.3.3 or newer (for the marketing site and the
  repository scripts)
- macOS for the full app. The interface also builds and runs on Linux for
  development, without the macOS-only services.

Run the desktop app:

```sh
cd packages/app
cargo run
```

macOS grants permissions (microphone, accessibility, system audio) to an app
bundle, so test permission flows with a bundle rather than `cargo run`:

```sh
packages/app/scripts/bundle-macos.sh --debug
open packages/app/dist/*/VoxFusion.app
```

Run the marketing site:

```sh
bun install
bun run --filter @voxfusion/marketingsite dev
```

## Useful Commands

Desktop app, from `packages/app`:

```sh
cargo check --all-targets
cargo test
cargo run --features fixture   # with VOXFUSION_FIXTURE=<scenario.json>: canned data, no backend
```

Marketing site and repository-wide checks, from the repository root:

```sh
bun run check
bun run --filter @voxfusion/marketingsite typecheck
bun run --filter @voxfusion/marketingsite build
```

## Repository Layout

- `packages/app/src`: the desktop app. `ui/` holds the windows, `backend/` the
  services behind them (recording, transcription, models, storage), and
  `platform/` the macOS integrations.
- `packages/app/assets`: icons, fonts, images and translations compiled into
  the app.
- `packages/app/macos`: Info.plist, entitlements and the app icon.
- `packages/app/scripts`: bundling and the Parakeet engine build.
- `packages/marketingsite/src`: Astro marketing site.
- `.github/workflows`: checks and release automation.

## Pull Request Guidelines

- Open an issue first for large features, behavior changes, or refactors.
- Keep pull requests focused on one user-facing change or one internal cleanup.
- Include screenshots or short recordings for UI changes.
- Update translations when changing user-facing text.
- Add or update tests when touching shared logic, data persistence, hotkeys,
  audio processing, or onboarding behavior.
- Run the relevant checks before requesting review and list what you ran in the
  pull request.

## Privacy and Telemetry

Do not add telemetry for transcripts, dictionary words, raw audio, selected
microphone names, file paths, or hotkey values. Analytics events should be
coarse-grained and should not include personal content.

## Licensing

By contributing, you agree that your contributions are licensed under the MIT
license used by this repository.
