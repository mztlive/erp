#!/usr/bin/env bash
# E2E 服务管理：确保前端/后端已启动（已启动则复用，不重复拉起）。
#
# 后端: web-api（debug 二进制，CWD=backend，端口 10001，健康检查 /health）
# 前端: next dev（erp-client，端口 3000）
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CLIENT_DIR="$(cd "${E2E_DIR}/erp-client" && pwd)"

API_PORT=10001
FRONT_PORT=3000
API_HEALTH="http://127.0.0.1:${API_PORT}/health"
FRONT_URL="http://localhost:${FRONT_PORT}"

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
if curl -sf -o /dev/null --max-time 3 "${FRONT_URL}" 2>/dev/null; then
    echo "前端已启动（端口 ${FRONT_PORT}），复用"
else
    echo "前端未启动，准备拉起（next dev）..."
    mkdir -p "${E2E_DIR}/logs"
    (cd "${CLIENT_DIR}" && nohup npm run dev > "${E2E_DIR}/logs/next-dev.log" 2>&1 &)
    wait_healthy "${FRONT_URL}" "前端" 180
fi
