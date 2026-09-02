#!/usr/bin/env bash
# 停止 web-api（E2E 重置前停写、修复后重启用）。
# 只匹配 backend/target/debug 下的 web-api 进程，避免误杀其他同名进程。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_DIR="$(cd "${SCRIPT_DIR}/../backend" && pwd)"
WEB_API_BIN="${BACKEND_DIR}/target/debug/web-api"
PIDS="$(pgrep -f "${WEB_API_BIN}" || true)"
if [[ -z "${PIDS}" ]]; then
    echo "web-api 未在运行"
    exit 0
fi
echo "停止 web-api: ${PIDS}"
kill ${PIDS}
for _ in $(seq 1 15); do
    if ! pgrep -f "${WEB_API_BIN}" >/dev/null 2>&1; then
        echo "web-api 已停止"
        exit 0
    fi
    sleep 1
done
echo "强制终止 web-api"
pkill -9 -f "${WEB_API_BIN}" || true
