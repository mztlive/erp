#!/usr/bin/env bash
# 停止 web-api（E2E 重置前停写、修复后重启用）。
# 只匹配 cargo 产物目录下的 web-api 进程，避免误杀其他同名进程。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_DIR="$(cd "${SCRIPT_DIR}/../backend" && pwd)"
# 与 restart-backend.sh 同源：产物目录以 cargo metadata 为准，避免误杀其他同名进程。
TARGET_DIR="$(cd "${BACKEND_DIR}" && cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("target_directory", ""))' 2>/dev/null || true)"
if [[ -z "${TARGET_DIR}" ]]; then
    TARGET_DIR="${BACKEND_DIR}/target"
fi
WEB_API_BIN="${TARGET_DIR}/debug/web-api"
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
