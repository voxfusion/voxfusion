# VoxFusion

A local-first, offline-capable voice transcription desktop app. All transcription happens on-device using a Whisper V3 Large Turbo model — no servers, no accounts, no internet required after first-time model download.

## Packages

| Package                    | Description              | Stack                                                           |
| -------------------------- | ------------------------ | --------------------------------------------------------------- |
| `@voxfusion/app`           | Desktop application      | Tauri v2, SolidJS, Tailwind CSS, TypeScript, whisper-rs, SQLite |
| `@voxfusion/marketingsite` | Public marketing website | Astro, TypeScript, Wrangler                                     |

## Prerequisites

- [Bun](https://bun.sh/) (v1.0+)
- [Rust](https://rustup.rs/) (for Tauri development)
- [CMake](https://cmake.org/) (required to build whisper-rs; on macOS: `brew install cmake`)

## Getting Started

### Install dependencies

```bash
bun install
```

### Development

```bash
bun run dev
```

Or run the app directly:

```bash
bun run --filter @voxfusion/app dev
```

Or run the marketing site directly:

```bash
bun run --filter @voxfusion/marketingsite dev
```

### Tauri Desktop App

For full Tauri development with the native window:

```bash
cd packages/app
bun run dev
```

### Build

```bash
bun run build
```

## How it works

On first launch, the app walks the user through onboarding:

1. Microphone permission
2. Accessibility permission (for typing transcribed text into other apps)
3. Microphone device selection
4. Hotkey configuration
5. Privacy choice (opt in or out of anonymous usage analytics)
6. **Whisper model download** (~1.5 GB, one-time, requires internet)
7. Try-it-out / learning step
8. Completion

After onboarding, the app is fully offline. All transcriptions and dictionary words are stored locally in a SQLite database under the app's data directory.

The app can optionally send anonymous usage analytics (PostHog, EU-hosted, event metadata only — never audio or transcription text); you're asked about this during onboarding and can disable it anytime in Settings → Privacy.

## Linting & Formatting

This project uses [Biome](https://biomejs.dev/) for linting and formatting.

```bash
bun run check        # lint + format
bun run check:fix    # auto-fix
bun run format       # format only
bun run lint         # lint only
bun run typecheck    # TypeScript type checking
```

## Contributing

VoxFusion is released under the MIT license. See [CONTRIBUTING.md](CONTRIBUTING.md)
for setup instructions, pull request guidelines, and privacy expectations for
contributions.

## License

MIT. See [LICENSE](LICENSE).
