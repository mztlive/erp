#!/usr/bin/env bash
# 启动（或重启）web-api 并等待健康。
# 用法: bash scripts/restart-backend.sh [--build]
#   --build  先执行 cargo build -p web-api（后端代码变更后使用）
#   ERP_WEB_API_OWNED_PID_FILE 由调用方提供私有 PID 文件，并负责退出时关闭服务。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BACKEND_DIR="$(cd "${E2E_DIR}/backend" && pwd)"
# cargo 实际产物目录可能被 ~/.cargo/config.toml 的 target-dir 改到别处（如外接硬盘）：
# 用 cargo metadata 解析，失败时回退到 backend/target。
TARGET_DIR="$(cd "${BACKEND_DIR}" && cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("target_directory", ""))' 2>/dev/null || true)"
if [[ -z "${TARGET_DIR}" ]]; then
    TARGET_DIR="${BACKEND_DIR}/target"
fi
WEB_API_BIN="${TARGET_DIR}/debug/web-api"
API_HEALTH="http://127.0.0.1:10001/health"

mkdir -p "${E2E_DIR}/logs"

# 先停旧进程
if pgrep -f "${WEB_API_BIN}" >/dev/null 2>&1; then
    bash "${SCRIPT_DIR}/stop-backend.sh"
fi

if [[ "${1:-}" == "--build" ]]; then
    echo "构建 web-api（cargo build -p web-api）..."
    (cd "${BACKEND_DIR}" && cargo build -p web-api)
fi

if [[ ! -x "${WEB_API_BIN}" ]]; then
    echo "未找到 ${WEB_API_BIN}，先构建..."
    (cd "${BACKEND_DIR}" && cargo build -p web-api)
fi

if [[ ! -x "${WEB_API_BIN}" ]]; then
    echo "错误: 构建后仍未找到可执行文件 ${WEB_API_BIN}" >&2
    exit 1
fi

echo "启动 web-api: ${WEB_API_BIN}"
echo "后端日志: ${E2E_DIR}/logs/web-api.log"
rm -f "${E2E_DIR}/logs/web-api.pid"
if [[ -n "${ERP_WEB_API_OWNED_PID_FILE:-}" ]]; then
    # 受调用方管理的服务不脱离进程组；启动后立即登记 PID，健康检查失败也可清理。
    (cd "${BACKEND_DIR}" && exec "${WEB_API_BIN}") > "${E2E_DIR}/logs/web-api.log" 2>&1 < /dev/null &
    service_pid=$!
    printf '%s\n' "${service_pid}" > "${ERP_WEB_API_OWNED_PID_FILE}"
    printf '%s\n' "${service_pid}" > "${E2E_DIR}/logs/web-api.pid"
else
    # 默认模式通过 fork + setsid 脱离调用链，脚本结束后服务继续运行。
    (cd "${BACKEND_DIR}" && python3 -c "
import os, sys
if os.fork() > 0:
    os._exit(0)
os.setsid()
with open(sys.argv[2], 'w') as pid_file:
    pid_file.write(str(os.getpid()))
os.execv(sys.argv[1], [sys.argv[1]])
" "${WEB_API_BIN}" "${E2E_DIR}/logs/web-api.pid" > "${E2E_DIR}/logs/web-api.log" 2>&1 < /dev/null)
fi

started=${SECONDS}
next_progress=0
while (( SECONDS - started < 120 )); do
    elapsed=$((SECONDS - started))
    if (( elapsed >= next_progress )); then
        echo "等待 web-api 健康检查：${elapsed}/120s（${API_HEALTH}）"
        next_progress=$((elapsed + 10))
    fi
    if [[ -s "${E2E_DIR}/logs/web-api.pid" ]]; then
        service_pid="$(cat "${E2E_DIR}/logs/web-api.pid")"
        if ! kill -0 "${service_pid}" 2>/dev/null; then
            echo "错误: web-api 启动进程已退出，最近日志：" >&2
            tail -50 "${E2E_DIR}/logs/web-api.log" >&2 || true
            exit 1
        fi
    fi
    if curl -sf --max-time 3 "${API_HEALTH}" >/dev/null 2>&1; then
        echo "web-api 已就绪（/health OK，耗时 $((SECONDS - started))s）"
        exit 0
    fi
    sleep 2
done
echo "错误: web-api 120s 内未就绪，最近日志：" >&2
tail -50 "${E2E_DIR}/logs/web-api.log" >&2 || true
exit 1
