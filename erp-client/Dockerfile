# syntax=docker/dockerfile:1.7
# 构建上下文是 erp-client/。
# NEXT_PUBLIC_API_BASE_URL 在 next build 时写入浏览器包，必须是浏览器能访问的 API 地址。

ARG NODE_IMAGE=docker.1ms.run/node:22-bookworm-slim

FROM ${NODE_IMAGE} AS deps
WORKDIR /app

COPY package.json package-lock.json ./
RUN npm ci

FROM ${NODE_IMAGE} AS builder
WORKDIR /app

COPY --from=deps /app/node_modules ./node_modules
COPY . .

ARG NEXT_PUBLIC_API_BASE_URL=http://127.0.0.1:10001
ENV NEXT_PUBLIC_API_BASE_URL=${NEXT_PUBLIC_API_BASE_URL} \
    NEXT_TELEMETRY_DISABLED=1

RUN npm run build

FROM ${NODE_IMAGE} AS runner
WORKDIR /app

ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    PORT=3000 \
    HOSTNAME=0.0.0.0

RUN set -eux; \
    groupadd --system --gid 10001 erp-client; \
    useradd --system --uid 10001 --gid erp-client \
        --home-dir /app --shell /usr/sbin/nologin erp-client

COPY --from=builder --chown=erp-client:erp-client /app/.next/standalone ./
COPY --from=builder --chown=erp-client:erp-client /app/.next/static ./.next/static
COPY --from=builder --chown=erp-client:erp-client /app/public ./public

USER erp-client

EXPOSE 3000

CMD ["node", "server.js"]
