#!/usr/bin/env bash
# ERP 开发数据库重置封装（E2E 专用）。
#
# 复用 backend/scripts/reset-dev-business-data.sh 的 preview/execute/verify 合同：
#   - 只清理业务数据集合（客户/合同/销售单/采购单/票款/库存/审批实例等）；
#   - 默认保留：账号/RBAC、供应商/商品/仓库主数据、source_systems、file_assets、
#           对象存储、审计记录、编号计数器（即"不 reset 账号数据"）；
#   - ERP_RESET_INCLUDE_CATALOG=1 时额外清供应商/商品/SKU/供给、仓库、分类/品牌/单位及全部 Party；
#   - 不填充任何种子数据，流程从 0 开始跑。
#
# 安全门禁：
#   - 必须显式 E2E_RESET=1（防止误执行）；
#   - 目标为远程开发库时必须同时提供 E2E_ALLOW_REMOTE_RESET=1，
#     且主机白名单从 config.toml 的 database.uri 精确解析（禁止通配符）。
#
# 用法：
#   E2E_RESET=1 [E2E_ALLOW_REMOTE_RESET=1] bash scripts/reset-db.sh
#
# 本脚本只清库、不填种子。开发开单准备请用：
#   E2E_RESET=1 [E2E_ALLOW_REMOTE_RESET=1] bash scripts/prepare-dev.sh
#
# 前置条件（由 run-flow.sh / prepare-dev.sh 保证）：执行前已停止 web-api 等写入方；执行后需重启应用。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BACKEND_DIR="$(cd "${E2E_DIR}/backend" && pwd)"
RESET_SCRIPT="${BACKEND_DIR}/scripts/reset-dev-business-data.sh"
CONFIG_FILE="${BACKEND_DIR}/config.toml"

[[ "${E2E_RESET:-0}" == "1" ]] || {
    echo "错误: 数据库重置需要显式设置 E2E_RESET=1" >&2
    exit 2
}
[[ -f "${RESET_SCRIPT}" ]] || { echo "错误: 未找到 ${RESET_SCRIPT}" >&2; exit 2; }
[[ -f "${CONFIG_FILE}" ]] || { echo "错误: 未找到 ${CONFIG_FILE}" >&2; exit 2; }

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

echo "== E2E 数据库重置 =="
echo "目标数据库: ${DB_NAME}"
echo "目标主机: ${TARGET_HOSTS}"
if [[ "${ERP_RESET_INCLUDE_CATALOG:-0}" == "1" ]]; then
    echo "保留项: 账号/RBAC、source_systems、file_assets、审计、计数器"
    echo "清理项: 业务单据及供应商/商品/SKU/供给/仓库/分类/品牌/单位（不填充种子）"
else
    echo "保留项: 账号/RBAC、供应商/商品/仓库主数据、source_systems、file_assets、审计、计数器"
    echo "清理项: 客户/合同/销售单/采购单/票款/库存/审批实例/待办等业务数据（不填充种子）"
fi

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

if [[ "${ERP_RESET_INCLUDE_CATALOG:-0}" == "1" ]]; then
    echo "== 重置完成：业务数据与主数据已清空，账号保留 =="
else
    echo "== 重置完成：业务数据已清空，账号与主数据保留 =="
fi
