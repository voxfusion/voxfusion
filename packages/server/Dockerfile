FROM oven/bun:1 AS turbo-pruner

WORKDIR /app

# Install Turbo Repo
RUN bun add -g turbo@^2.7.2

# Copy root package files needed for turbo prune
COPY package.json bun.lock* turbo.json ./
COPY packages/server/package.json ./packages/server/
COPY packages/server/tsconfig.json ./packages/server/
COPY packages/server/drizzle.config.ts ./packages/server/

# Use Turbo Repo to prune the monorepo and fetch only server dependencies
RUN turbo prune --scope=@voxfusion/server --docker

FROM oven/bun:1 AS builder

WORKDIR /app

# Install Turbo Repo
RUN bun add -g turbo@^2.7.2

# Copy the pruned JSON files (package.json files)
COPY --from=turbo-pruner /app/out/json/ ./

# Copy the pruned full source code
COPY --from=turbo-pruner /app/out/full/ ./

# Install dependencies using bun (Turbo prune ensures only server deps are included)
RUN bun install --frozen-lockfile

# Build using Turbo Repo (will only build if changes detected via cache)
WORKDIR /app
RUN turbo run build --filter=@voxfusion/server

# Compile the server binary
WORKDIR /app/packages/server
RUN bun build --compile src/index.ts --outfile server --target bun

FROM oven/bun:1 AS healthcheck-builder

WORKDIR /app

RUN echo 'fetch("http://localhost:3000/api/health").then(r => r.ok ? process.exit(0) : process.exit(1)).catch(() => process.exit(1))' > healthcheck.ts

RUN bun build --compile healthcheck.ts --outfile healthcheck --target bun

FROM gcr.io/distroless/base-debian12:nonroot

WORKDIR /app

COPY --from=builder --chown=nonroot:nonroot /app/packages/server/server ./server

COPY --from=healthcheck-builder --chown=nonroot:nonroot /app/healthcheck ./healthcheck

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["./healthcheck"]

ENTRYPOINT ["./server"]
