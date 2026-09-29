#!/usr/bin/env bash
# E2E 服务管理：确保前端/后端已启动（已启动则复用，不重复拉起）。
#
# 后端: web-api（dev 二进制，依赖已开优化；CWD=backend，端口 10001，健康检查 /health）
# 前端: 默认 E2E_FRONTEND=prod，用 next build 的 standalone server（端口 E2E_FRONT_PORT，默认 3100）。
#       源码比构建新（或 E2E_FRONT_BUILD=1）时先重建再重启；不占用开发用的 next dev 端口 3000。
#       E2E_FRONTEND=dev 时沿用 next dev（端口 3000，已启动则复用）。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CLIENT_DIR="$(cd "${E2E_DIR}/erp-client" && pwd)"

API_PORT=10001
API_HEALTH="http://127.0.0.1:${API_PORT}/health"
FRONT_MODE="${E2E_FRONTEND:-prod}"
if [[ "${FRONT_MODE}" == "dev" ]]; then
    FRONT_PORT=3000
else
    FRONT_PORT="${E2E_FRONT_PORT:-3100}"
fi
if [[ "${FRONT_MODE}" == "dev" ]]; then
    FRONT_URL="http://localhost:${FRONT_PORT}"
else
    # standalone server 只监听回环地址，不对局域网暴露。
    FRONT_URL="http://127.0.0.1:${FRONT_PORT}"
fi
FRONT_PID_FILE="${E2E_DIR}/logs/next-e2e.pid"
FRONT_BUILD_ID="${CLIENT_DIR}/.next/BUILD_ID"
STANDALONE_DIR="${CLIENT_DIR}/.next/standalone"

wait_healthy() {
    local url="$1" name="$2" timeout="$3"
    local started=${SECONDS} next_progress=0 elapsed
    while (( SECONDS - started < timeout )); do
        elapsed=$((SECONDS - started))
        if (( elapsed >= next_progress )); then
            echo "等待 ${name} 就绪：${elapsed}/${timeout}s（${url}）"
            next_progress=$((elapsed + 10))
        fi
        if curl -sf --max-time 3 "${url}" >/dev/null 2>&1; then
            echo "${name} 已就绪"
            return 0
        fi
        sleep 2
    done
    echo "错误: ${name} 在 ${timeout}s 内未就绪（${url}）" >&2
    return 1
}

# ---- 后端 ----
if curl -sf --max-time 3 "${API_HEALTH}" >/dev/null 2>&1; then
    echo "后端已启动（端口 ${API_PORT}），复用"
else
    echo "后端健康检查未通过，启动或重启后端..."
    bash "${SCRIPT_DIR}/restart-backend.sh"
fi

# ---- 前端 ----
# 源码有文件比 BUILD_ID 新即视为构建过期。node_modules、构建产物、测试、文档，以及 next dev /
# tsc 自己会改写的 next-env.d.ts、*.tsbuildinfo 不参与判断，避免开着 dev 时每轮都重建。
front_build_stale() {
    [[ "${E2E_FRONT_BUILD:-0}" == "1" ]] && return 0
    [[ -f "${FRONT_BUILD_ID}" && -f "${STANDALONE_DIR}/server.js" ]] || return 0
    local newer
    newer="$(cd "${CLIENT_DIR}" && find . \
        \( -path './.*' -o -path ./node_modules -o -path ./tests -o -name '*.test.*' \
           -o -name '*.md' -o -name '*.tsbuildinfo' -o -name next-env.d.ts \) -prune -o \
        -type f -newer "${FRONT_BUILD_ID}" -print -quit)"
    [[ -n "${newer}" ]]
}

stop_prod_front() {
    if [[ -s "${FRONT_PID_FILE}" ]]; then
        local pid
        pid="$(cat "${FRONT_PID_FILE}")"
        if kill -0 "${pid}" 2>/dev/null; then
            echo "停止旧的前端生产服务（PID ${pid}）"
            kill "${pid}" 2>/dev/null || true
            for _ in {1..20}; do
                kill -0 "${pid}" 2>/dev/null || break
                sleep 0.5
            done
        fi
        rm -f "${FRONT_PID_FILE}"
    fi
    if curl -sf -o /dev/null --max-time 3 "${FRONT_URL}" 2>/dev/null; then
        echo "错误: 端口 ${FRONT_PORT} 仍被非本脚本启动的进程占用，请先释放或改 E2E_FRONT_PORT。" >&2
        exit 1
    fi
}

build_prod_front() {
    echo "前端构建（next build，standalone）..."
    (cd "${CLIENT_DIR}" && npm run build) > "${E2E_DIR}/logs/next-build.log" 2>&1 || {
        echo "错误: 前端构建失败，最近日志：" >&2
        tail -50 "${E2E_DIR}/logs/next-build.log" >&2 || true
        exit 1
    }
    # standalone server.js 不自带静态资源，按官方做法复制进 standalone 目录。
    rm -rf "${STANDALONE_DIR}/.next/static" "${STANDALONE_DIR}/public"
    cp -R "${CLIENT_DIR}/.next/static" "${STANDALONE_DIR}/.next/static"
    if [[ -d "${CLIENT_DIR}/public" ]]; then
        cp -R "${CLIENT_DIR}/public" "${STANDALONE_DIR}/public"
    fi
}

start_prod_front() {
    echo "启动前端生产服务（端口 ${FRONT_PORT}），日志: ${E2E_DIR}/logs/next-e2e.log"
    (cd "${STANDALONE_DIR}" && NODE_ENV=production PORT="${FRONT_PORT}" HOSTNAME=127.0.0.1 \
        nohup node server.js > "${E2E_DIR}/logs/next-e2e.log" 2>&1 < /dev/null &
        printf '%s\n' "$!" > "${FRONT_PID_FILE}")
    wait_healthy "${FRONT_URL}" "前端" 60
}

mkdir -p "${E2E_DIR}/logs"
if [[ "${FRONT_MODE}" == "dev" ]]; then
    if curl -sf -o /dev/null --max-time 3 "${FRONT_URL}" 2>/dev/null; then
        echo "前端 next dev 已启动（端口 ${FRONT_PORT}），复用"
    else
        echo "前端未启动，准备拉起（next dev）..."
        (cd "${CLIENT_DIR}" && nohup npm run dev > "${E2E_DIR}/logs/next-dev.log" 2>&1 &)
        wait_healthy "${FRONT_URL}" "前端" 180
    fi
elif front_build_stale; then
    stop_prod_front
    build_prod_front
    start_prod_front
elif curl -sf -o /dev/null --max-time 3 "${FRONT_URL}" 2>/dev/null; then
    echo "前端生产服务已启动（端口 ${FRONT_PORT}），构建未过期，复用"
else
    stop_prod_front
    start_prod_front
fi
