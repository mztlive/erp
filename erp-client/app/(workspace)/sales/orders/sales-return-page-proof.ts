import { SALES_RETURN_CASE_APPROVAL_REQUIREMENT } from "@/features/sales-orders/lib/sales-return-no-approval"

/**
 * 销售页面对 SalesReturnCase 的审批政策证明。
 *
 * SalesReturnCase 为合同 NO_APPROVAL 类型：本页及其销售单详情
 * 不渲染销售退货的审批流程选择、绑定卡、运行摘要或决定弹窗。
 * `PENDING_WAREHOUSE_ACCEPTANCE` / `PENDING_PROCUREMENT` /
 * `PENDING_FINANCE` 是履约与执行分工态，不是审批复核。
 */
export const SALES_ORDERS_SALES_RETURN_APPROVAL_REQUIREMENT =
    SALES_RETURN_CASE_APPROVAL_REQUIREMENT

/** 销售退货页面禁止出现的审批动作文案。 */
export const SALES_ORDERS_SALES_RETURN_FORBIDDEN_ACTIONS = [
    "选择流程",
    "更新审批流程版本",
    "通过",
    "驳回",
    "撤回审批",
    "改派当前审批人",
    "恢复当前审批人",
    "取消受阻审批",
] as const
