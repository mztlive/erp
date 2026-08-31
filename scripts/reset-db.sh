#!/usr/bin/env bash
# ERP 开发数据库重置与种子填充入口。
#
# 默认执行合同：
#   1. 停止 web-api，禁止清理期间继续写入；
#   2. 复用 backend/scripts/reset-dev-business-data.sh 完成 preview/execute/verify；
#   3. 启动 web-api，必要时用 CLI init-admin 修复超级管理员；
#   4. 按 §11 创建全部岗位账号，再写仓库、财务责任、客户与合同；
#   5. 按文档发布 PROCESS_REQUIRED 审批定义（供应商付款不审批；资金单出纳提交、总监只审批）；
#   6. 填充供应商收款账户、商品与公司商品池；
#   7. 保持 web-api 运行，执行结束后可直接使用。
#
# 数据范围：
#   - 默认清理业务数据，保留账号/RBAC、供应商/商品/仓库主数据、source_systems、
#     file_assets、对象存储、审计记录与编号计数器；
#   - ERP_RESET_INCLUDE_CATALOG=1 时额外清供应商/商品/SKU/供给、仓库、分类/品牌/单位及全部 Party；
#   - 账号/RBAC 始终保留；缺失的岗位账号由种子按固定登录名补齐，开发密码无法登录时重置；
#   - ERP_RESET_ONLY=1 仅供 E2E 编排使用：只清库、不填种子、不重启 web-api。
#
# 安全门禁：
#   - 必须显式 E2E_RESET=1（防止误执行）；
#   - 目标为远程开发库时必须同时提供 E2E_ALLOW_REMOTE_RESET=1，
#     且主机白名单从 config.toml 的 database.uri 精确解析（禁止通配符）。
#
# 日常用法（清业务数据并恢复全部基础种子）：
#   E2E_RESET=1 [E2E_ALLOW_REMOTE_RESET=1] bash scripts/reset-db.sh
#
# E2E 只清库模式：
#   E2E_RESET=1 ERP_RESET_ONLY=1 [E2E_ALLOW_REMOTE_RESET=1] bash scripts/reset-db.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BACKEND_DIR="$(cd "${E2E_DIR}/backend" && pwd)"
RESET_SCRIPT="${BACKEND_DIR}/scripts/reset-dev-business-data.sh"
CONFIG_FILE="${BACKEND_DIR}/config.toml"
RESET_ONLY="${ERP_RESET_ONLY:-0}"

[[ "${E2E_RESET:-0}" == "1" ]] || {
    echo "错误: 数据库重置需要显式设置 E2E_RESET=1" >&2
    exit 2
}
[[ -f "${RESET_SCRIPT}" ]] || { echo "错误: 未找到 ${RESET_SCRIPT}" >&2; exit 2; }
[[ -f "${CONFIG_FILE}" ]] || { echo "错误: 未找到 ${CONFIG_FILE}" >&2; exit 2; }
[[ "${RESET_ONLY}" == "0" || "${RESET_ONLY}" == "1" ]] || {
    echo "错误: ERP_RESET_ONLY 只能是 0 或 1" >&2
    exit 2
}

DB_NAME="$(
    python3 - "${CONFIG_FILE}" <<'PY'
import sys, tomllib
with open(sys.argv[1], "rb") as f:
    cfg = tomllib.load(f)
print(cfg["database"]["db_name"])
PY
)"
MONGO_URI="$(
    python3 - "${CONFIG_FILE}" <<'PY'
import sys, tomllib
with open(sys.argv[1], "rb") as f:
    cfg = tomllib.load(f)
print(cfg["database"]["uri"])
PY
)"

# 从 URI 精确解析主机白名单（与官方脚本同一逻辑），禁止通配符。
TARGET_HOSTS="$(
    ERP_E2E_MONGO_URI="${MONGO_URI}" python3 - <<'PY'
import os, sys, urllib.parse
uri = os.environ["ERP_E2E_MONGO_URI"]
_scheme, rest = uri.split("://", 1)
authority = rest.split("/", 1)[0].rsplit("@", 1)[-1]
hosts = []
for endpoint in authority.split(","):
    endpoint = endpoint.strip()
    if not endpoint:
        continue
    if endpoint.startswith("["):
        host = endpoint[1:endpoint.find("]")]
    else:
        host = endpoint.rsplit(":", 1)[0] if endpoint.count(":") == 1 else endpoint
    host = urllib.parse.unquote(host).rstrip(".").lower()
    if not host or any(t in host for t in ("@", "://", "/", "*")):
        print("错误: 无法解析目标主机", file=sys.stderr)
        raise SystemExit(1)
    hosts.append(host)
if not hosts:
    print("错误: 无法解析目标主机", file=sys.stderr)
    raise SystemExit(1)
print(",".join(hosts))
PY
)"

IS_REMOTE=0
if ! ERP_E2E_MONGO_URI="${MONGO_URI}" python3 - <<'PY' 2>/dev/null
import ipaddress, os, urllib.parse
uri = os.environ["ERP_E2E_MONGO_URI"]
scheme, rest = uri.split("://", 1)
authority = rest.split("/", 1)[0].rsplit("@", 1)[-1]
hosts = authority.split(",")
def loopback(endpoint: str) -> bool:
    endpoint = endpoint.strip()
    if endpoint.startswith("["):
        host = endpoint[1:endpoint.find("]")]
    else:
        host = endpoint.rsplit(":", 1)[0] if endpoint.count(":") == 1 else endpoint
    host = urllib.parse.unquote(host).rstrip(".").lower()
    if host in {"localhost", "host.docker.internal"}:
        return True
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return False
raise SystemExit(0 if scheme == "mongodb" and hosts and all(loopback(h) for h in hosts) else 1)
PY
then
    IS_REMOTE=1
fi

if [[ "${IS_REMOTE}" -eq 1 ]]; then
    [[ "${E2E_ALLOW_REMOTE_RESET:-0}" == "1" ]] || {
        echo "错误: 目标是远程开发库（${TARGET_HOSTS}），必须显式设置 E2E_ALLOW_REMOTE_RESET=1" >&2
        exit 2
    }
    export ERP_RESET_ALLOWED_REMOTE_HOSTS="${TARGET_HOSTS}"
fi

API_BASE="${API_BASE:-http://127.0.0.1:10001}"
CLI_BIN="${HOME}/Development/rust-build-target/debug/cli"

# 超级管理员只能由 CLI init-admin 创建。开发密码无法登录时修复为 admin / 123456。
ensure_super_admin() {
    local login_json=""
    login_json="$(
        curl -sf --max-time 8 -X POST "${API_BASE}/login" \
            -H "Content-Type: application/json" \
            -d '{"account":"admin","password":"123456","account_kind":"admin"}' || true
    )"
    if printf '%s' "${login_json}" | python3 -c '
import json, sys
raw = sys.stdin.read().strip()
if not raw:
    raise SystemExit(1)
payload = json.loads(raw)
token = (payload.get("data") or {}).get("token")
raise SystemExit(0 if payload.get("success") and token else 1)
'; then
        echo "超级管理员 admin 可登录"
        return 0
    fi

    echo "超级管理员无法以开发密码登录，执行 init-admin 修复"
    if [[ ! -x "${CLI_BIN}" ]]; then
        echo "未找到 ${CLI_BIN}，先构建 cli..."
        (cd "${BACKEND_DIR}" && cargo build -p cli)
    fi
    [[ -x "${CLI_BIN}" ]] || { echo "错误: 构建后仍未找到 ${CLI_BIN}" >&2; exit 2; }
    (cd "${BACKEND_DIR}" && "${CLI_BIN}" --config-path "${CONFIG_FILE}" init-admin \
        --account admin --name "系统管理员" --password "123456")
}

echo "== 开发数据库重置 =="
echo "目标数据库: ${DB_NAME}"
echo "目标主机: ${TARGET_HOSTS}"
if [[ "${RESET_ONLY}" == "1" ]]; then
    echo "完成模式: 只清库（不填种子、不重启 web-api）"
else
    echo "完成模式: 清库 + 岗位账号 + 审批定义 + 基础种子（web-api 保持运行）"
fi
if [[ "${ERP_RESET_INCLUDE_CATALOG:-0}" == "1" ]]; then
    echo "保留项: 账号/RBAC、source_systems、file_assets、审计、计数器"
    echo "清理项: 业务单据及供应商/商品/SKU/供给/仓库/分类/品牌/单位"
else
    echo "保留项: 账号/RBAC、供应商/商品/仓库主数据、source_systems、file_assets、审计、计数器"
    echo "清理项: 客户/合同/销售单/采购单/票款/库存/审批实例/待办等业务数据"
fi

echo "-- 停止 web-api（停写） --"
bash "${SCRIPT_DIR}/stop-backend.sh"

# 1) preview：只读核对并输出集合摘要（同一目标与摘要贯穿 execute/verify）
echo "-- [1/3] preview --"
PREVIEW_OUTPUT="$("${RESET_SCRIPT}")"
echo "${PREVIEW_OUTPUT}" | grep -E "^(目标|集合摘要|运行模式|主数据范围)" || true
SCOPE_DIGEST="$(echo "${PREVIEW_OUTPUT}" | sed -n 's/^集合摘要: //p')"
[[ -n "${SCOPE_DIGEST}" ]] || { echo "错误: preview 未输出集合摘要" >&2; exit 2; }

# 2) execute：清理业务数据（远程库走 --allow-remote + 精确主机白名单）
echo "-- [2/3] execute --"
if [[ "${IS_REMOTE}" -eq 1 ]]; then
    "${RESET_SCRIPT}" \
        --execute --confirm-db "${DB_NAME}" --expect-summary "${SCOPE_DIGEST}" --allow-remote
else
    "${RESET_SCRIPT}" \
        --execute --confirm-db "${DB_NAME}" --expect-summary "${SCOPE_DIGEST}"
fi

# 3) verify：只读后置校验
echo "-- [3/3] verify --"
"${RESET_SCRIPT}" --verify --expect-summary "${SCOPE_DIGEST}"

if [[ "${RESET_ONLY}" == "1" ]]; then
    if [[ "${ERP_RESET_INCLUDE_CATALOG:-0}" == "1" ]]; then
        echo "== 只清库完成：业务数据与主数据已清空，web-api 保持停止 =="
    else
        echo "== 只清库完成：业务数据已清空，账号与主数据保留，web-api 保持停止 =="
    fi
    exit 0
fi

echo "-- 启动 web-api（写账号、审批定义与种子） --"
bash "${SCRIPT_DIR}/restart-backend.sh"

echo "-- 确保超级管理员 admin 可登录 --"
ensure_super_admin

echo "-- 填充基础种子：全部岗位账号 + 仓库 + 财务责任 + 客户 + 合同 --"
node "${SCRIPT_DIR}/seed-dev-foundation.mjs"

echo "-- 按文档发布需审批单据的审批定义（供应商付款不审批） --"
node "${SCRIPT_DIR}/publish-approval-definitions.mjs"

echo "-- 填充目录种子：供应商收款账户 + 商品 + 公司商品池 --"
node "${SCRIPT_DIR}/seed-dev-catalog.mjs"

echo "== 重置与种子填充完成：web-api 已运行，可直接使用 =="
