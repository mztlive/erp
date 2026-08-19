import { DELIVERY_APPROVAL_REQUIREMENT } from "@/features/fulfillment-operations/lib/delivery-no-approval"

/**
 * 履约页面对 Delivery 的审批政策证明。
 *
 * Delivery 为合同 NO_APPROVAL 类型：本页仓发/直发创建结果、详情、
 * 提交确认不渲染审批流程选择、决定弹窗、撤回、改派或审批历史。
 */
export const FULFILLMENT_DELIVERY_APPROVAL_REQUIREMENT =
    DELIVERY_APPROVAL_REQUIREMENT

/** 仓发页面禁止出现的审批动作文案。 */
export const FULFILLMENT_DELIVERY_FORBIDDEN_ACTIONS = [
    "选择流程",
    "更新审批流程版本",
    "通过",
    "驳回",
    "撤回审批",
    "改派当前审批人",
    "恢复当前审批人",
    "取消受阻审批",
] as const
