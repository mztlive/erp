#!/usr/bin/env bash
# 销售、客户、合同及审批工作流开发数据重置入口。
#
# 默认只读取并输出待处理记录计数；只有同时提供 --execute 与正确的
# --confirm-db 才允许写入。远程 MongoDB 还必须额外提供 --allow-remote。
{ set +x; } 2>/dev/null
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEFAULT_CONFIG_FILE="${BACKEND_DIR}/config.toml"
MONGOSH_SCRIPT="${SCRIPT_DIR}/reset-dev-business-data.mongosh.js"

CONFIG_FILE="${DEFAULT_CONFIG_FILE}"
EXECUTE=0
VERIFY=0
CONFIRM_DB=""
ALLOW_REMOTE=0
EXPECT_SUMMARY=""

usage() {
    cat <<'EOF'
用法：
  backend/scripts/reset-dev-business-data.sh [选项]

默认行为：
  连接 backend/config.toml 指定的 MongoDB，仅预览目标主机、数据库名、
  集合 allowlist、集合摘要、各集合计数、审批 WorkItem、冲突索引、
  共享 Party 保护结果和悬挂引用，不执行任何写入。

选项：
  --config <path>              使用其他 TOML 配置文件；仍只读取 database.uri/db_name
  --execute                    启用清理；缺少此参数时永远只预览
  --verify                     只读校验后置条件；不得与 --execute 同时使用
  --confirm-db <name>          执行时必须与 database.db_name 完全一致
  --expect-summary <digest>    必须与预览输出的集合摘要完全一致
  --allow-remote               允许对非 localhost/loopback MongoDB 执行清理
  -h, --help                   显示帮助

执行合同：
  1. 执行前必须停止所有 API、worker、同步任务和其他写入方。
  2. 先运行默认预览，核对目标主机、数据库名、集合 allowlist、集合摘要、
     非零集合、审批 WorkItem、冲突索引和 Party 保护摘要。
  3. 本地执行：
       backend/scripts/reset-dev-business-data.sh \
         --execute --confirm-db <db_name> --expect-summary <集合摘要>
  4. 执行后校验：
       backend/scripts/reset-dev-business-data.sh \
         --verify --expect-summary <集合摘要>
  5. 远程开发库执行还必须显式追加 --allow-remote，且
     ERP_RESET_ALLOWED_REMOTE_HOSTS 精确列出 URI 中全部主机
     （逗号分隔，禁止通配符）。
  6. preview / execute / verify 必须使用同一目标与同一集合摘要；
     摘要由 database.db_name 与 mongosh 脚本内容计算，不一致则失败关闭。
  7. 本工具不得调用 dropDatabase()，只 drop 固定集合或按固定过滤删除。

本工具不会输出 MongoDB URI、用户名、密码、证书或 config.toml 中的其他密钥。
EOF
}

die() {
    echo "错误: $*" >&2
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --config)
            [[ $# -ge 2 ]] || die "--config 缺少路径"
            CONFIG_FILE="$2"
            shift 2
            ;;
        --execute)
            EXECUTE=1
            shift
            ;;
        --verify)
            VERIFY=1
            shift
            ;;
        --confirm-db)
            [[ $# -ge 2 ]] || die "--confirm-db 缺少数据库名"
            CONFIRM_DB="$2"
            shift 2
            ;;
        --expect-summary)
            [[ $# -ge 2 ]] || die "--expect-summary 缺少集合摘要"
            EXPECT_SUMMARY="$2"
            shift 2
            ;;
        --allow-remote)
            ALLOW_REMOTE=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            die "未知参数: $1；使用 --help 查看合同"
            ;;
    esac
done

command -v python3 >/dev/null 2>&1 || die "未找到 python3；需要 Python 3.11+ 的 tomllib"
command -v mongosh >/dev/null 2>&1 || die "未找到 mongosh；请先安装 MongoDB Shell"
[[ -f "${CONFIG_FILE}" ]] || die "配置文件不存在: ${CONFIG_FILE}"
[[ -f "${MONGOSH_SCRIPT}" ]] || die "MongoDB 重置脚本不存在: ${MONGOSH_SCRIPT}"

read_database_value() {
    local key="$1"
    python3 - "${CONFIG_FILE}" "${key}" <<'PY'
import pathlib
import re
import sys

try:
    import tomllib
except ImportError:
    print("错误: 当前 python3 不包含 tomllib，需要 Python 3.11+", file=sys.stderr)
    raise SystemExit(1)

path = pathlib.Path(sys.argv[1])
key = sys.argv[2]
try:
    with path.open("rb") as handle:
        value = tomllib.load(handle)["database"][key]
except Exception:
    print(f"错误: 无法从配置读取 database.{key}", file=sys.stderr)
    raise SystemExit(1)

if not isinstance(value, str) or not value or "\n" in value or "\r" in value:
    print(f"错误: database.{key} 必须是非空单行字符串", file=sys.stderr)
    raise SystemExit(1)
if key == "uri" and not (value.startswith("mongodb://") or value.startswith("mongodb+srv://")):
    print("错误: database.uri 必须使用 mongodb:// 或 mongodb+srv://", file=sys.stderr)
    raise SystemExit(1)
if key == "db_name" and not re.fullmatch(r"[A-Za-z0-9_.-]+", value):
    print("错误: database.db_name 含不受支持字符", file=sys.stderr)
    raise SystemExit(1)

sys.stdout.write(value)
PY
}

MONGO_URI="$(read_database_value uri)"
DB_NAME="$(read_database_value db_name)"
trap 'unset MONGO_URI DB_NAME' EXIT

if [[ "${EXECUTE}" -eq 1 && "${VERIFY}" -eq 1 ]]; then
    die "--execute 与 --verify 不能同时使用"
fi

case "${DB_NAME}" in
    admin|config|local)
        die "拒绝以 MongoDB 系统数据库 ${DB_NAME} 作为重置目标"
        ;;
esac

SCOPE_DIGEST="$(
    python3 - "${DB_NAME}" "${MONGOSH_SCRIPT}" <<'PY'
import hashlib
import pathlib
import sys

db_name = sys.argv[1]
script = pathlib.Path(sys.argv[2]).read_bytes()
sys.stdout.write(hashlib.sha256(db_name.encode("utf-8") + b"\n" + script).hexdigest())
PY
)"

TARGET_HOSTS="$(
    ERP_RESET_MONGO_URI="${MONGO_URI}" python3 - <<'PY'
import os
import sys
import urllib.parse

uri = os.environ["ERP_RESET_MONGO_URI"]
if "://" not in uri:
    print("错误: 无法解析目标主机（URI 已隐藏）", file=sys.stderr)
    raise SystemExit(1)
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
    if not host or any(token in host for token in ("@", "://", "/", "*")):
        print("错误: 无法解析目标主机（URI 已隐藏）", file=sys.stderr)
        raise SystemExit(1)
    hosts.append(host)
if not hosts:
    print("错误: 无法解析目标主机（URI 已隐藏）", file=sys.stderr)
    raise SystemExit(1)
sys.stdout.write(",".join(hosts))
PY
)"

IS_REMOTE=0
if ! ERP_RESET_MONGO_URI="${MONGO_URI}" python3 - <<'PY'
import ipaddress
import os
import urllib.parse

uri = os.environ["ERP_RESET_MONGO_URI"]
scheme, rest = uri.split("://", 1)
authority = rest.split("/", 1)[0].rsplit("@", 1)[-1]
hosts = authority.split(",")

def is_loopback(endpoint: str) -> bool:
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

raise SystemExit(0 if scheme == "mongodb" and hosts and all(is_loopback(host) for host in hosts) else 1)
PY
then
    IS_REMOTE=1
fi

if [[ "${EXECUTE}" -eq 1 ]]; then
    [[ -n "${CONFIRM_DB}" ]] || die "执行必须提供 --confirm-db ${DB_NAME}"
    [[ "${CONFIRM_DB}" == "${DB_NAME}" ]] || die "--confirm-db 与配置中的 database.db_name 不一致"
    if [[ "${IS_REMOTE}" -eq 1 && "${ALLOW_REMOTE}" -ne 1 ]]; then
        die "目标不是 loopback MongoDB；远程开发库执行必须显式提供 --allow-remote"
    fi
    if [[ "${IS_REMOTE}" -eq 1 ]]; then
        [[ -n "${ERP_RESET_ALLOWED_REMOTE_HOSTS:-}" ]] ||
            die "远程执行必须通过 ERP_RESET_ALLOWED_REMOTE_HOSTS 提供精确主机白名单"
        if ! ERP_RESET_MONGO_URI="${MONGO_URI}" \
            ERP_RESET_REMOTE_HOST_ALLOWLIST="${ERP_RESET_ALLOWED_REMOTE_HOSTS}" python3 - <<'PY'
import ipaddress
import os
import urllib.parse

uri = os.environ["ERP_RESET_MONGO_URI"]
allowlist = os.environ["ERP_RESET_REMOTE_HOST_ALLOWLIST"]
allowed = set()
for raw in allowlist.split(","):
    host = raw.strip().rstrip(".").lower()
    if not host or any(token in host for token in ("*", "/", "@", "://")):
        raise SystemExit(1)
    try:
        host = ipaddress.ip_address(host.strip("[]")).compressed
    except ValueError:
        pass
    allowed.add(host)

_scheme, rest = uri.split("://", 1)
authority = rest.split("/", 1)[0].rsplit("@", 1)[-1]
actual = set()
for endpoint in authority.split(","):
    endpoint = endpoint.strip()
    if endpoint.startswith("["):
        host = endpoint[1:endpoint.find("]")]
    else:
        host = endpoint.rsplit(":", 1)[0] if endpoint.count(":") == 1 else endpoint
    host = urllib.parse.unquote(host).rstrip(".").lower()
    try:
        host = ipaddress.ip_address(host).compressed
    except ValueError:
        pass
    actual.add(host)

raise SystemExit(0 if actual and actual.issubset(allowed) else 1)
PY
        then
            die "MongoDB URI 主机不在 ERP_RESET_ALLOWED_REMOTE_HOSTS 精确白名单中"
        fi
    fi
fi

if [[ "${EXECUTE}" -eq 1 || "${VERIFY}" -eq 1 ]]; then
    [[ -n "${EXPECT_SUMMARY}" ]] || die "执行或校验必须提供 --expect-summary <集合摘要>"
    if [[ "${EXPECT_SUMMARY}" != "${SCOPE_DIGEST}" ]]; then
        die "集合摘要与当前目标或脚本不一致；请重新预览并使用同一摘要"
    fi
fi

echo "目标数据库: ${DB_NAME}"
echo "目标主机: ${TARGET_HOSTS}"
echo "集合摘要: ${SCOPE_DIGEST}"
if [[ "${EXECUTE}" -eq 1 ]]; then
    echo "运行模式: EXECUTE（清理已授权）"
elif [[ "${VERIFY}" -eq 1 ]]; then
    echo "运行模式: VERIFY（只读后置校验，不执行写入）"
else
    echo "运行模式: PREVIEW（只读计数，不执行写入）"
fi
if [[ "${IS_REMOTE}" -eq 1 ]]; then
    echo "目标拓扑: 非 loopback（URI 已隐藏）"
else
    echo "目标拓扑: loopback（URI 已隐藏）"
fi

ERP_RESET_MONGO_URI="${MONGO_URI}" \
ERP_RESET_DB_NAME="${DB_NAME}" \
ERP_RESET_EXECUTE="${EXECUTE}" \
ERP_RESET_VERIFY="${VERIFY}" \
ERP_RESET_CONFIRMED_DB="${CONFIRM_DB}" \
    mongosh --nodb --norc --quiet --file "${MONGOSH_SCRIPT}"
