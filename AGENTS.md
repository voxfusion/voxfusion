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

- Rust toolchain must be ≥1.85 (edition 2024 support). Run `rustup update stable && rustup default stable`.
- The app frontend at localhost:1420 shows a loading spinner and never progresses in a browser because it waits for the Tauri IPC bridge. This is expected; UI component development still works by navigating directly to routes.
- `turbo dev` runs both packages' dev scripts concurrently (uses TUI mode).
