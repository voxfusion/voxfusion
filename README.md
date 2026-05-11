# VoxFusion

A local-first, offline-capable voice transcription desktop app. All transcription happens on-device using a Whisper V3 Large Turbo model — no servers, no accounts, no internet required after first-time model download.

## Packages

| Package | Description | Stack |
|---------|-------------|-------|
| `@voxfusion/app` | Desktop application | Tauri v2, SolidJS, Tailwind CSS, TypeScript, whisper-rs, SQLite |

## Prerequisites

- [Bun](https://bun.sh/) (v1.0+)
- [Rust](https://rustup.rs/) (for Tauri development)

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

### Tauri Desktop App

For full Tauri development with the native window:

```bash
cd packages/app
bun run tauri:dev
```

### Build

```bash
bun run build
```

## How it works

On first launch, the app walks the user through onboarding:

1. Microphone permission
2. Accessibility permission (for global hotkeys and typing into other apps)
3. Microphone device selection
4. Hotkey configuration
5. **Whisper model download** (~1.5 GB, one-time, requires internet)
6. Try-it-out / learning step
7. Completion

After onboarding, the app is fully offline. All transcriptions and dictionary words are stored locally in a SQLite database under the app's data directory.

## Linting & Formatting

This project uses [Biome](https://biomejs.dev/) for linting and formatting.

```bash
bun run check        # lint + format
bun run check:fix    # auto-fix
bun run format       # format only
bun run lint         # lint only
bun run typecheck    # TypeScript type checking
```

## Project Structure

```
voxfusion/
├── packages/
│   └── app/                # Tauri + SolidJS desktop app
│       ├── src/            # Frontend source (SolidJS)
│       │   ├── components/ # UI components, onboarding wizard
│       │   ├── pages/      # Home, Dictionary, VoiceControl
│       │   ├── lib/        # Settings, hotkey utils
│       │   └── i18n/       # Translations
│       └── src-tauri/      # Tauri/Rust backend
│           └── src/
│               ├── handlers/  # audio, whisper, db, text, media
│               └── listeners/ # accessibility, system keys
├── turbo.json              # Turborepo configuration
├── biome.json              # Biome linter config
└── package.json            # Root package.json
```

## Local storage

- **Whisper model**: `{app_data_dir}/models/ggml-large-v3-turbo.bin`
- **Transcriptions + dictionary**: `{app_data_dir}/voxfusion.db` (SQLite)
- **Settings**: Tauri store plugin (`settings.json` in app data dir)

## License

MIT
