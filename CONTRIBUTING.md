# Contributing to VoxFusion

Thanks for helping improve VoxFusion. This project is a local-first desktop
transcription app built with Tauri, SolidJS, TypeScript, Rust, SQLite, and
Whisper.

## Project Priorities

- Keep transcription local-first and offline after the first model download.
- Preserve user privacy: avoid sending transcript, audio, dictionary, or device
  data to third-party services.
- Prefer small, reviewable changes over broad rewrites.
- Match the existing code style and user interface patterns.

## Development Setup

Install the required tools:

- [Bun](https://bun.sh/) 1.3.3 or newer
- [Rust](https://rustup.rs/)
- macOS for full desktop and permission-flow development

Install dependencies:

```sh
bun install
```

Run the full desktop app:

```sh
cd packages/app
bun run tauri:dev
```

Run the root development task:

```sh
bun run dev
```

## Useful Commands

```sh
bun run check
bun run typecheck
bun run build
```

Package-specific checks can be run from the repository root:

```sh
bun run --filter @voxfusion/app typecheck
bun run --filter @voxfusion/app lint
bun run --filter @voxfusion/marketingsite typecheck
bun run --filter @voxfusion/marketingsite lint
```

Rust checks for the Tauri backend:

```sh
cd packages/app/src-tauri
cargo check
cargo test
```

## Repository Layout

- `packages/app/src`: SolidJS frontend for the desktop app.
- `packages/app/src-tauri/src`: Rust handlers, listeners, and Tauri setup.
- `packages/marketingsite/src`: Astro marketing site.
- `.github/workflows`: release automation.

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
