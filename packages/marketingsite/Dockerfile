FROM oven/bun:1 AS builder

WORKDIR /app

COPY package.json bun.lock* turbo.json tsconfig.json ./
COPY packages/marketingsite/package.json ./packages/marketingsite/

RUN bun install

COPY packages/marketingsite/ ./packages/marketingsite/

WORKDIR /app/packages/marketingsite
RUN bun run build

FROM nginx:alpine

COPY --from=builder /app/packages/marketingsite/dist /usr/share/nginx/html
COPY packages/marketingsite/nginx.conf /etc/nginx/nginx.conf.template

CMD ["/bin/sh", "-c", "envsubst '$PORT' < /etc/nginx/nginx.conf.template > /etc/nginx/conf.d/default.conf && nginx -g 'daemon off;'"]
