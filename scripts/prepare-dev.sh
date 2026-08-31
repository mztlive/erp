#!/usr/bin/env bash
# 开发开单准备：全量清理业务数据与目录主数据，再恢复可开单的完整基础种子。
#
# 本入口委托 reset-db.sh 执行统一编排，并设置 ERP_RESET_INCLUDE_CATALOG=1：
#   - 清理业务数据、供应商、商品、SKU、供给、仓库与字典；
#   - 必要时用 CLI init-admin 修复超级管理员 admin / 123456；
#   - 按 erp-phase-1.md §11 创建全部岗位账号（缺失则建、密码不对则重置）；
#   - 发布 11 个 PROCESS_REQUIRED 审批定义，供应商付款不配置审批；
#   - 重建仓库、财务责任、客户、合同、供应商收款账户和公司商品池；
#   - 执行完成后保持 web-api 运行。
#
# 岗位账号（密码均为 123456）：
#   admin        超级管理员
#   xiaoshou     销售
#   lisiyong     销售领导
#   caigou       采购
#   yunying      运营
#   cangchu      仓储
#   caiwu        财务总监
#   fukuan       出纳
#   kaipiao      开票人
#   guanli       管理层
#   xitong       系统管理员
#
# 财务三人分责：caiwu 只审批，不得提交回款/退款/冲正（ForbidSubmitterAsApprover）；
# fukuan 出纳经办回款、退款、冲正和付款任务；kaipiao 开票（发票无审批）。
#
# 审批链（文档有部门时序则照文档；未指定审批人时按岗位分离与资金内控设计）：
#   单据             提交人                         审批链
#   销售单           xiaoshou                       采购确认(caigou)
#   卡券销售单       xiaoshou                       销售领导 → 运营 → 财务总监
#   销售变更单       xiaoshou                       采购确认履约影响 → 财务复核
#   采购单           caigou                         财务总监审批
#   采购变更单       caigou                         仓储确认库存发货影响 → 财务复核
#   库存调整单       cangchu                        财务审批成本影响
#   客户回款单       fukuan                         财务总监审批入账
#   客户退款单       fukuan                         销售领导确认退款依据 → 财务总监
#   供应商退款单     fukuan                         采购确认退款依据 → 财务总监
#   回款冲正单       fukuan                         销售领导确认冲正依据 → 财务总监
#   付款冲正单       fukuan                         采购确认冲正依据 → 财务总监
#   供应商付款单     fukuan（付款任务）              不审批
#   销项发票         kaipiao                        不审批
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
echo "== 准备完成：web-api 已运行，可用上列账号登录并直接开单 =="
