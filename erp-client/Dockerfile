# syntax=fushangyun-vpc.tencentcloudcr.com/base/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e
# 构建上下文是 erp-client/。
# NEXT_PUBLIC_API_BASE_URL 在 next build 时写入浏览器包，必须是浏览器能访问的 API 地址。

ARG NODE_IMAGE=fushangyun-vpc.tencentcloudcr.com/base/node:22-bookworm-slim@sha256:43ac6c60b8f89723f746e8a92ce91abd5017e627ce1ddfe4238355d3a30b772c

FROM ${NODE_IMAGE} AS deps
WORKDIR /app

COPY package.json package-lock.json ./
# Debian 使用 GNU 原生模块。先按锁文件下载，避免 npm 将下载失败当作可选依赖跳过。
RUN npm cache add "$(node -p 'require("./package-lock.json").packages["node_modules/@next/swc-linux-" + process.arch + "-gnu"].resolved')" \
    && npm ci --include=optional \
    && node -e 'const pkg = "@next/swc-linux-" + process.arch + "-gnu"; require(pkg); console.log("Native SWC loaded:", pkg)'

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
    groupadd --gid 10001 erp; \
    useradd --no-log-init --no-create-home --uid 10001 --gid erp \
        --home-dir /app --shell /usr/sbin/nologin erp

COPY --from=builder --chown=erp:erp /app/.next/standalone ./
COPY --from=builder --chown=erp:erp /app/.next/static ./.next/static
COPY --from=builder --chown=erp:erp /app/public ./public

USER erp

EXPOSE 3000

CMD ["node", "server.js"]
