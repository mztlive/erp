#!/usr/bin/env bash
# 启动（或重启）web-api 并等待健康。
# 用法: bash scripts/restart-backend.sh [--build]
#   --build  先执行 cargo build -p web-api（后端代码变更后使用）
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BACKEND_DIR="$(cd "${E2E_DIR}/../backend" && pwd)"
WEB_API_BIN="${HOME}/Development/rust-build-target/debug/web-api"
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

echo "启动 web-api ..."
# python 双重 fork 完全脱离调用链（父进程立即退出，服务被 launchd 收养），
# 并在子进程中 setsid 脱离会话/进程组（macOS 无 setsid 命令）：
#   - 调用方进程组被终止时不会连带杀掉服务；
#   - 脚本 stdout 为管道时，bash 退出不会因等待后台子进程而挂住。
(cd "${BACKEND_DIR}" && python3 -c "
import os, sys
if os.fork() > 0:
    os._exit(0)
os.setsid()
os.execv(sys.argv[1], sys.argv[1:])
" "${WEB_API_BIN}" > "${E2E_DIR}/logs/web-api.log" 2>&1 < /dev/null)

waited=0
while [[ "${waited}" -lt 120 ]]; do
    if curl -sf --max-time 3 "${API_HEALTH}" >/dev/null 2>&1; then
        echo "web-api 已就绪（/health OK）"
        exit 0
    fi
    sleep 2
    waited=$((waited + 2))
done
echo "错误: web-api 120s 内未就绪，最近日志：" >&2
tail -50 "${E2E_DIR}/logs/web-api.log" >&2 || true
exit 1
