#!/usr/bin/env bash
# 开发开单准备：清空业务数据与主数据后，写入可开单的真实风格底座。
#
# 编排：
#   1. 停止 web-api（停写）
#   2. 调用 reset-db.sh（清业务单据，并清供应商/商品/仓库/字典；保留账号）
#   3. 临时拉起 web-api（只为发布审批定义和写种子）
#   4. 发布审批定义
#   5. 创建仓储账号、仓库、财务三人、客户与合同
#   6. 创建供应商、分类/品牌/单位、商品与供给，使公司商品池可开单
#   7. 再次停止 web-api，交给用户自行启动
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

echo "-- 数据库 reset（含全部主数据） --"
E2E_RESET=1 ERP_RESET_INCLUDE_CATALOG=1 bash "${SCRIPT_DIR}/reset-db.sh"

echo "-- 临时启动 web-api（写种子） --"
bash "${SCRIPT_DIR}/restart-backend.sh"

echo "-- 发布审批定义 --"
node "${SCRIPT_DIR}/publish-approval-definitions.mjs"

echo "-- 种子：仓储账号 + 仓库 + 财务责任 + 客户 + 合同 --"
node "${SCRIPT_DIR}/seed-dev-foundation.mjs"

echo "-- 种子：供应商 + 商品 + 公司商品池 --"
node "${SCRIPT_DIR}/seed-dev-catalog.mjs"

echo "-- 停止 web-api --"
bash "${SCRIPT_DIR}/stop-backend.sh"

echo "== 准备完成：请自行启动后端后再开单 =="
