#!/usr/bin/env bash
# 本地开发：bash scripts/start-dev.sh
# 启动 API、Worker、Next.js；Ctrl+C 只停止本次启动的服务。
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKEND_DIR="${ROOT_DIR}/backend"
CLIENT_DIR="${ROOT_DIR}/erp-client"
LOG_DIR="${ROOT_DIR}/logs"
WORKER_CONFIG="${BACKEND_DIR}/config.worker.local.toml"
PIDS=()
NAMES=()

if [[ "${1:-}" == "--help" ]]; then
    echo "用法：bash scripts/start-dev.sh"
    echo "启动本地 API（10001）、Worker（10002）、前端（3000）。"
    echo "已启动的服务直接复用；Ctrl+C 只停止本次启动的服务。"
    exit 0
fi
if [[ $# -gt 0 ]]; then
    echo "不支持的参数；用法：bash scripts/start-dev.sh" >&2
    exit 1
fi

cleanup() {
    trap '' INT TERM
    if [[ ${#PIDS[@]} -gt 0 ]]; then
        echo "停止本次启动的服务……"
        for pid in "${PIDS[@]}"; do
            kill -TERM -- "-${pid}" 2>/dev/null || true
        done
        for pid in "${PIDS[@]}"; do
            wait "${pid}" 2>/dev/null || true
        done
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

for cmd in cargo npm python3 curl lsof; do
    command -v "${cmd}" >/dev/null || { echo "缺少命令：${cmd}" >&2; exit 1; }
done
mkdir -p "${LOG_DIR}"

# 首次运行时从 API 配置生成 Worker 配置，沿用数据库、密钥和存储配置。
python3 - "${BACKEND_DIR}/config.toml" "${WORKER_CONFIG}" <<'PY'
import os
import re
import sys
import tomllib
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

api_path, worker_path = map(Path, sys.argv[1:])
api = tomllib.loads(api_path.read_text())
if api["app"]["port"] != 10001:
    sys.exit("本地 API 配置的 app.port 必须为 10001")
if not worker_path.exists():
    text = api_path.read_text()
    if not api.get("xxl_job"):
        sys.exit("config.toml 缺少 [xxl_job]；请按 config.toml.example 配置")
    url = urlsplit(api["xxl_job"]["executor_url"])
    host = url.hostname
    if ":" in host:
        host = f"[{host}]"
    executor_url = urlunsplit(url._replace(netloc=f"{host}:10002"))
    section = None
    lines = []
    for line in text.splitlines(keepends=True):
        match = re.match(r"\s*\[([^\]]+)\]", line)
        if match:
            section = match[1].strip()
        if section == "app" and re.match(r"\s*port\s*=", line):
            line = "port = 10002\n"
        if section == "xxl_job" and re.match(r"\s*executor_url\s*=", line):
            line = f'executor_url = "{executor_url}"\n'
        lines.append(line)
    with open(worker_path, "x", opener=lambda path, flags: os.open(path, flags, 0o600)) as out:
        out.write("".join(lines))
    print(f"已生成本地 Worker 配置：{worker_path}")
worker = tomllib.loads(worker_path.read_text())
if worker["app"]["port"] != 10002:
    sys.exit("config.worker.local.toml 的 app.port 必须为 10002")
if not worker.get("xxl_job"):
    sys.exit("config.worker.local.toml 缺少 [xxl_job]")
for key in ("database", "s3"):
    if api.get(key) != worker.get(key):
        sys.exit(f"API 与 Worker 的 {key} 配置不同，请同步本地配置")
if api["app"]["secret"] != worker["app"]["secret"]:
    sys.exit("API 与 Worker 的 app.secret 不同，请同步本地配置")
PY

healthy() {
    curl -fsS -o /dev/null --max-time 3 "$1" 2>/dev/null
}

API_URL="http://127.0.0.1:10001/health"
WORKER_URL="http://127.0.0.1:10002/health"
FRONT_URL="http://127.0.0.1:3000"
START_API=1 START_WORKER=1 START_FRONT=1
if healthy "${API_URL}"; then START_API=0; fi
if healthy "${WORKER_URL}"; then START_WORKER=0; fi
if healthy "${FRONT_URL}"; then START_FRONT=0; fi

for entry in "${START_API}:10001" "${START_WORKER}:10002" "${START_FRONT}:3000"; do
    if [[ "${entry%%:*}" == 1 ]] && lsof -tiTCP:"${entry##*:}" -sTCP:LISTEN >/dev/null 2>&1; then
        echo "端口 ${entry##*:} 已占用，但服务检查未通过，请先检查占用进程。" >&2
        exit 1
    fi
done
if [[ "${START_FRONT}" == 1 && ! -d "${CLIENT_DIR}/node_modules" ]]; then
    echo "请先安装前端依赖：cd ${CLIENT_DIR} && npm ci" >&2
    exit 1
fi
if [[ "${START_API}" == 1 || "${START_WORKER}" == 1 ]]; then
    echo "编译本地 API 和 Worker……"
    (cd "${BACKEND_DIR}" && cargo build -p erp-server --bin web-api --bin erp-worker)
    TARGET_DIR="$(cd "${BACKEND_DIR}" && cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
fi

start_service() {
    local name="$1" directory="$2" logfile="$3"
    shift 3
    echo "启动 ${name}，日志：${logfile}"
    # 独立进程组包含 npm / Next.js 子进程，退出时一并关闭。
    python3 -c 'import os,sys; os.setsid(); os.chdir(sys.argv[1]); os.execvp(sys.argv[2], sys.argv[2:])' \
        "${directory}" "$@" > "${logfile}" 2>&1 < /dev/null &
    PIDS+=("$!")
    NAMES+=("${name}")
}

if [[ "${START_API}" == 1 ]]; then
    start_service web-api "${BACKEND_DIR}" "${LOG_DIR}/web-api.log" \
        "${TARGET_DIR}/debug/web-api" --config-path "${BACKEND_DIR}/config.toml"
else
    echo "复用已运行的 API（10001）"
fi
if [[ "${START_WORKER}" == 1 ]]; then
    start_service erp-worker "${BACKEND_DIR}" "${LOG_DIR}/erp-worker.log" \
        "${TARGET_DIR}/debug/erp-worker" --config-path "${WORKER_CONFIG}"
else
    echo "复用已运行的 Worker（10002）"
fi
if [[ "${START_FRONT}" == 1 ]]; then
    start_service next-dev "${CLIENT_DIR}" "${LOG_DIR}/next-dev.log" npm run dev -- --port 3000
else
    echo "复用已运行的前端（3000）"
fi

check_children() {
    local i
    for ((i=0; i<${#PIDS[@]}; i++)); do
        if ! kill -0 "${PIDS[$i]}" 2>/dev/null; then
            echo "${NAMES[$i]} 已退出，请查看 ${LOG_DIR}/${NAMES[$i]}.log" >&2
            exit 1
        fi
    done
}

started=${SECONDS}
while ! { healthy "${API_URL}" && healthy "${WORKER_URL}" && healthy "${FRONT_URL}"; }; do
    check_children
    if (( SECONDS - started >= 180 )); then
        echo "服务 180 秒内未全部就绪，请检查 ${LOG_DIR} 中的日志。" >&2
        exit 1
    fi
    echo "等待服务就绪（$((SECONDS - started))/180 秒）……"
    sleep 2
done

echo "本地服务已就绪：前端 http://localhost:3000 | API :10001 | Worker :10002"
if [[ ${#PIDS[@]} == 0 ]]; then
    echo "三个服务原本已运行，无需重复启动。"
    exit 0
fi
echo "保持此终端打开；Ctrl+C 停止本次启动的服务。"
while true; do
    check_children
    sleep 2
done
