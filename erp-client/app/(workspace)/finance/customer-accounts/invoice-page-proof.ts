import { INVOICE_APPROVAL_REQUIREMENT } from "@/features/customer-receivables/lib/invoice-no-approval"

/**
 * 客户往来页面对 Invoice 的审批政策证明。
 *
 * Invoice 为合同 NO_APPROVAL 类型：本页及其发票详情/预览/登记路径
 * 不渲染审批流程选择、决定弹窗、撤回或改派入口。
 */
export const CUSTOMER_ACCOUNTS_INVOICE_APPROVAL_REQUIREMENT =
    INVOICE_APPROVAL_REQUIREMENT

/** 发票页面禁止出现的审批动作文案。 */
export const CUSTOMER_ACCOUNTS_INVOICE_FORBIDDEN_ACTIONS = [
    "选择流程",
    "更新审批流程版本",
    "通过",
    "驳回",
    "撤回审批",
    "改派当前审批人",
    "恢复当前审批人",
    "取消受阻审批",
] as const
