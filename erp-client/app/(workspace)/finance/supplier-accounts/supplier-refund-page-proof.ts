import { SUPPLIER_REFUND_APPROVAL_REQUIREMENT } from "@/features/supplier-payables/lib/supplier-refund-approval"

/**
 * 供应商往来页面对 SupplierRefund 的审批政策证明。
 *
 * SupplierRefund 为合同 PROCESS_REQUIRED 类型：未提交展示绑定卡，
 * 提交确认展示冻结路线，运行中/终态嵌入摘要与历史。
 */
export const SUPPLIER_ACCOUNTS_REFUND_APPROVAL_REQUIREMENT =
    SUPPLIER_REFUND_APPROVAL_REQUIREMENT

/** 供应商退款页面必须由服务端白名单驱动的审批动作文案。 */
export const SUPPLIER_ACCOUNTS_REFUND_SERVER_ACTIONS = [
    "更新审批流程版本",
    "通过",
    "驳回",
    "撤回审批",
    "改派当前审批人",
    "恢复当前审批人",
    "取消受阻审批",
] as const

/** 供应商退款页面禁止出现的流程选择/推导文案。 */
export const SUPPLIER_ACCOUNTS_REFUND_FORBIDDEN_ACTIONS = [
    "选择流程",
    "换人",
    "转交",
    "关闭任务",
] as const
