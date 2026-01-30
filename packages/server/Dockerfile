FROM oven/bun:1 AS turbo-pruner

WORKDIR /app

RUN bun add -g turbo@^2.7.2

COPY package.json bun.lock* turbo.json ./
COPY packages/server/package.json ./packages/server/
COPY packages/server/tsconfig.json ./packages/server/
COPY packages/server/drizzle.config.ts ./packages/server/
COPY packages/server/src ./packages/server/src/
COPY packages/server/drizzle ./packages/server/drizzle/

RUN turbo prune --scope=@voxfusion/server --docker

FROM oven/bun:1 AS builder

WORKDIR /app

RUN bun add -g turbo@^2.7.2
COPY --from=turbo-pruner /app/out/json/ ./
COPY --from=turbo-pruner /app/out/full/ ./

RUN bun install --frozen-lockfile

WORKDIR /app
RUN turbo run build --filter=@voxfusion/server

WORKDIR /app/packages/server
RUN bun build --compile src/index.ts --outfile server --target bun

FROM oven/bun:1 AS healthcheck-builder

WORKDIR /app
RUN echo 'fetch("http://localhost:3000/api/health").then(r => r.ok ? process.exit(0) : process.exit(1)).catch(() => process.exit(1))' > healthcheck.ts
RUN bun build --compile healthcheck.ts --outfile healthcheck --target bun

#deploy
FROM debian:12-slim

RUN useradd --create-home --shell /bin/bash appuser

WORKDIR /app

COPY --from=builder --chown=appuser:appuser /app/packages/server/server ./server
COPY --from=healthcheck-builder --chown=appuser:appuser /app/healthcheck ./healthcheck

USER appuser

EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["./healthcheck"]

ENTRYPOINT ["./server"]
