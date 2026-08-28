#!/usr/bin/env bash
# 开发开单准备：全量清理业务数据与目录主数据，再恢复可开单的完整基础种子。
#
# 本入口委托 reset-db.sh 执行统一编排，并设置 ERP_RESET_INCLUDE_CATALOG=1：
#   - 清理业务数据、供应商、商品、SKU、供给、仓库与字典；
#   - 发布 PROCESS_REQUIRED 审批定义，供应商付款不配置审批；
#   - 重建账号职责、仓库、财务责任、客户、合同、供应商收款账户和公司商品池；
#   - 执行完成后保持 web-api 运行。
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
E2E_RESET=1 ERP_RESET_INCLUDE_CATALOG=1 bash "${SCRIPT_DIR}/reset-db.sh"
echo "== 准备完成：web-api 已运行，可直接开单 =="
