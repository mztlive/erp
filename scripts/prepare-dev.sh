#!/usr/bin/env bash
# 开发开单准备：清业务数据后补齐开单底座（审批定义 + 客户 + 合同）。
#
# 编排：
#   1. 停止 web-api（停写）
#   2. 调用 reset-db.sh（清业务数据，保留账号/主数据，不填种子）
#   3. 重启 web-api 并确保前端可用
#   4. 发布审批定义
#   5. 创建开发客户与合同（幂等）
#
# 不创建销售单、采购单、库存或票款。E2E（run-flow.sh）不得调用本脚本。
#
# 用法:
#   E2E_RESET=1 [E2E_ALLOW_REMOTE_RESET=1] bash scripts/prepare-dev.sh
#
# 远程开发库必须同时提供 E2E_ALLOW_REMOTE_RESET=1（与 reset-db.sh 同一门禁）。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

[[ "${E2E_RESET:-0}" == "1" ]] || {
    echo "错误: 开发开单准备会清业务数据，需要显式设置 E2E_RESET=1" >&2
    echo "用法: E2E_RESET=1 [E2E_ALLOW_REMOTE_RESET=1] bash scripts/prepare-dev.sh" >&2
    exit 2
}

echo "== 开发开单准备 =="

echo "-- 停止 web-api（停写） --"
bash "${SCRIPT_DIR}/stop-backend.sh"

echo "-- 数据库 reset --"
E2E_RESET=1 bash "${SCRIPT_DIR}/reset-db.sh"

echo "-- 重启 web-api --"
bash "${SCRIPT_DIR}/restart-backend.sh"

echo "-- 确保前后端可用 --"
bash "${SCRIPT_DIR}/ensure-services.sh"

echo "-- 发布审批定义 --"
node "${SCRIPT_DIR}/publish-approval-definitions.mjs"

echo "-- 种子：客户 + 合同 --"
node "${SCRIPT_DIR}/seed-dev-foundation.mjs"

echo "== 准备完成：可从开单开始 =="
