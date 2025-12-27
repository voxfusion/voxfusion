# VoxFusion

A monorepo containing the VoxFusion platform, built with Turborepo.

## Packages

| Package | Description | Stack |
|---------|-------------|-------|
| `@voxfusion/app` | Desktop application | Tauri v2, SolidJS, Tailwind CSS, TypeScript |
| `@voxfusion/server` | Backend API server | Elysia.js, Drizzle ORM, PostgreSQL, better-auth |
| `@voxfusion/marketingsite` | Marketing landing page | Astro |

## Prerequisites

- [Bun](https://bun.sh/) (v1.0+)
- [Rust](https://rustup.rs/) (for Tauri development)
- [PostgreSQL](https://postgresql.org/) (for the server)

## Getting Started

### Install dependencies

```bash
bun install
```

### Development

Run all packages in development mode:

```bash
bun run dev
```

Or run individual packages:

```bash
# Desktop app (runs at http://localhost:1420)
bun run --filter @voxfusion/app dev

# Server (runs at http://localhost:3000)
bun run --filter @voxfusion/server dev

# Marketing site (runs at http://localhost:4321)
bun run --filter @voxfusion/marketingsite dev
```

### Tauri Desktop App

For full Tauri development with the native window:

```bash
cd packages/app
bun run tauri:dev
```

### Build

Build all packages:

```bash
bun run build
```

### Database (Server)

```bash
cd packages/server

# Generate migrations
bun run db:generate

# Run migrations
bun run db:migrate

# Push schema directly (development)
bun run db:push

# Open Drizzle Studio
bun run db:studio
```

## Linting & Formatting

This project uses [Biome](https://biomejs.dev/) for linting and formatting.

```bash
# Check all files
bun run check

# Fix all auto-fixable issues
bun run check:fix

# Format all files
bun run format

# Lint only
bun run lint
```

## Project Structure

```
voxfusion/
├── packages/
│   ├── app/                # Tauri + SolidJS desktop app
│   │   ├── src/            # Frontend source
│   │   └── src-tauri/      # Tauri/Rust backend
│   ├── server/             # Elysia.js API server
│   │   ├── src/
│   │   │   ├── db/         # Drizzle ORM schemas
│   │   │   └── routes/     # API routes
│   │   └── drizzle/        # Migrations
│   └── marketingsite/      # Astro marketing site
│       └── src/
│           ├── components/
│           ├── layouts/
│           └── pages/
├── turbo.json              # Turborepo configuration
├── biome.json              # Biome linter config
└── package.json            # Root package.json
```

## Scripts

| Script | Description |
|--------|-------------|
| `bun run dev` | Start all packages in dev mode |
| `bun run build` | Build all packages |
| `bun run lint` | Run Biome linter |
| `bun run lint:fix` | Fix linter issues |
| `bun run format` | Format all files with Biome |
| `bun run check` | Run Biome check (lint + format) |
| `bun run check:fix` | Fix all Biome issues |
| `bun run typecheck` | Run TypeScript type checking |

## Environment Variables

### Server (`packages/server/.env`)

```env
DATABASE_URL=postgresql://user:password@localhost:5432/voxfusion
BETTER_AUTH_SECRET=your-secret-key-here
BETTER_AUTH_URL=http://localhost:3000
```

## License

MIT
