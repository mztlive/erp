import { PURCHASE_RETURN_ORDER_APPROVAL_REQUIREMENT } from "@/features/purchase-orders/lib/purchase-return-order-no-approval"

/**
 * 采购页面对 PurchaseReturnOrder 的审批政策证明。
 *
 * PurchaseReturnOrder 为合同 NO_APPROVAL 类型：本页及其采购退货
 * 创建结果、详情、提交确认不渲染审批流程选择、决定弹窗、撤回或改派。
 * PENDING_EXECUTION 是待执行分工态，不得渲染为审批复核。
 */
export const PROCUREMENT_PURCHASE_RETURN_APPROVAL_REQUIREMENT =
    PURCHASE_RETURN_ORDER_APPROVAL_REQUIREMENT

/** 采购退货页面禁止出现的审批动作文案。 */
export const PROCUREMENT_PURCHASE_RETURN_FORBIDDEN_ACTIONS = [
    "选择流程",
    "更新审批流程版本",
    "通过",
    "驳回",
    "撤回审批",
    "改派当前审批人",
    "恢复当前审批人",
    "取消受阻审批",
] as const
