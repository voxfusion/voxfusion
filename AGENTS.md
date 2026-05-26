If you got some instingt useful for future agents, while working on task please write it down.

Use bun instead of Node.js, npm, pnpm, or vite.

## Cursor Cloud specific instructions

### Architecture

This is a Tauri v2 monorepo with two packages:
- `@voxfusion/app` — macOS desktop app (Tauri + SolidJS + Rust/whisper-rs)
- `@voxfusion/marketingsite` — Astro marketing website

The Tauri desktop app **cannot be compiled or run on Linux** because it depends on macOS-only frameworks (Metal for whisper-rs, CoreGraphics, objc2-app-kit, Accessibility APIs). On Linux VMs, only the JS/TS frontend layer and the marketing site are testable.

### What works on this Linux VM

| Task | Command | Notes |
|------|---------|-------|
| Install deps | `bun install` | From workspace root |
| Lint/format | `bun run check` | Biome; pre-existing warnings exist in codebase |
| TypeScript check (app) | `bun run --filter @voxfusion/app typecheck` | Passes cleanly |
| TypeScript check (marketing) | `bun run --filter @voxfusion/marketingsite typecheck` | Has pre-existing errors from app TSX imports |
| App frontend dev server | `cd packages/app && bunx vite --port 1420` | Runs standalone without Tauri backend |
| Marketing site dev server | `bun run --filter @voxfusion/marketingsite dev` | Astro on port 4321 |
| Build app frontend | `cd packages/app && bun run build` | Vite build (no Rust) |
| Build marketing site | `cd packages/marketingsite && bun run build` | Full static build |

### Rust toolchain

Rust stable (>=1.85 for edition 2024) is required. Run `rustup override set stable` at the workspace root. `cargo check` in `packages/app/src-tauri` will fail on Linux due to macOS-only native deps — this is expected.

### System dependencies for partial Rust compilation

If you need to attempt Rust compilation (e.g., to check non-macOS-specific modules), install:
```
sudo apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev libasound2-dev
sudo ln -sf /usr/lib/x86_64-linux-gnu/libstdc++.so.6 /usr/lib/x86_64-linux-gnu/libstdc++.so
```

### Key caveats

- The monorepo uses Turborepo (`turbo`) for task orchestration. `bun run dev` invokes `turbo dev` which tries to start both packages (including `tauri dev` which will fail on Linux). Use per-package commands instead.
- The marketing site's `typecheck` (`astro check`) processes `../app/src` files due to project references and will report errors on Linux — these are pre-existing and not blocking.
- Bun is at `/home/ubuntu/.bun/bin/bun`. Make sure `~/.bun/bin` is in PATH.
