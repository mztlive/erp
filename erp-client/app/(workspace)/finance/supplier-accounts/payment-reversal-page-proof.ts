import { PAYMENT_REVERSAL_APPROVAL_REQUIREMENT } from "@/features/supplier-payables/lib/payment-reversal-approval"

/**
 * 供应商往来页面对 PaymentReversal 的审批政策证明。
 *
 * PaymentReversal 为合同 PROCESS_REQUIRED 类型：未提交展示绑定卡，
 * 提交确认展示冻结路线，运行中/终态嵌入摘要与历史。
 */
export const SUPPLIER_ACCOUNTS_PAYMENT_REVERSAL_APPROVAL_REQUIREMENT =
    PAYMENT_REVERSAL_APPROVAL_REQUIREMENT

/** 付款冲正页面必须由服务端白名单驱动的审批动作文案。 */
export const SUPPLIER_ACCOUNTS_PAYMENT_REVERSAL_SERVER_ACTIONS = [
    "更新审批流程版本",
    "通过",
    "驳回",
    "撤回审批",
    "改派当前审批人",
    "恢复当前审批人",
    "取消受阻审批",
] as const

/** 付款冲正页面禁止出现的流程选择/推导文案。 */
export const SUPPLIER_ACCOUNTS_PAYMENT_REVERSAL_FORBIDDEN_ACTIONS = [
    "选择流程",
    "换人",
    "开始处理",
    "退回团队",
] as const
