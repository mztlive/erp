import {
    filterAllowedActions,
    mapDocumentApprovalViewDto,
    type ApprovalAllowedAction,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
} from "@/features/approval-workflow/types"
import type { CustomerRefundRow } from "@/features/customer-receivables/types"

/** 客户退款单作为合同 DocumentType 的固定值。 */
export const CUSTOMER_REFUND_DOCUMENT_TYPE = "CustomerRefund" as const

/** 工作项上的客户退款对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const CUSTOMER_REFUND_OBJECT_TYPE = "customer_refund" as const

/** 合同 §4.3 对 CustomerRefund 的固定政策。 */
export const CUSTOMER_REFUND_APPROVAL_REQUIREMENT = "PROCESS_REQUIRED" as const

export type CustomerRefundApprovalPhase = "draft" | "confirm" | "runtime"

const UNSUBMITTED_STATUS_CODES = new Set([
    "DRAFT",
    "draft",
    "REJECTED",
    "rejected",
])

/**
 * 判断客户退款单是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端状态码；缺省视为未提交。
 * @returns 未提交为 true。
 */
export const isUnsubmittedCustomerRefundStatus = (status?: string): boolean =>
    !status || UNSUBMITTED_STATUS_CODES.has(status)

/**
 * 把客户退款业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * 旧的待复核状态已收敛为审批中，不得再拆成独立复核节点。
 *
 * @param status 服务端状态码。
 */
export const customerRefundStatusLabel = (status?: string): string => {
    switch (status) {
        case "DRAFT":
        case "draft":
        case "REJECTED":
        case "rejected":
            return "草稿"
        case "IN_APPROVAL":
        case "PENDING_REVIEW":
        case "pending_review":
        case "in_approval":
            return "审批中"
        case "POSTED":
        case "posted":
            return "已过账"
        case "REVERSED":
        case "reversed":
            return "已冲正"
        default:
            return "退款单"
    }
}

/**
 * 客户退款状态对应的列表色调。未知码按进行中处理，不上屏枚举。
 *
 * @param status 服务端状态码。
 */
export const customerRefundStatusTone = (
    status?: string,
): CustomerRefundRow["statusTone"] => {
    switch (customerRefundStatusLabel(status)) {
        case "已过账":
            return "success"
        case "审批中":
            return "warning"
        case "已冲正":
            return "destructive"
        default:
            return "neutral"
    }
}

/**
 * 按单据生命周期选择客户退款审批区相位。提交确认由调用方显式传入。
 *
 * 有运行实例或已离开未提交阶段时进入 runtime，不得按复核或角色推导。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据状态码。
 */
export const customerRefundApprovalPhase = (
    approval?: DocumentApprovalView,
    status?: string,
): Exclude<CustomerRefundApprovalPhase, "confirm"> =>
    approval?.instance || !isUnsubmittedCustomerRefundStatus(status)
        ? "runtime"
        : "draft"

/**
 * 合并客户退款单与当前任务的服务端动作白名单。只做并集过滤，不补默认动作。
 *
 * @param documentActions 单据 `allowed_actions`。
 * @param workItemActions 当前任务 `allowed_actions`。
 */
export const mergeCustomerRefundAllowedActions = (
    documentActions?: readonly ApprovalAllowedAction[] | readonly string[],
    workItemActions?: readonly string[],
): readonly ApprovalAllowedAction[] =>
    filterAllowedActions([
        ...(documentActions ?? []),
        ...(workItemActions ?? []),
    ])

/**
 * 把客户退款详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapCustomerRefundApproval = (
    dto?: DocumentApprovalViewDto | null,
): DocumentApprovalView | undefined =>
    dto ? mapDocumentApprovalViewDto(dto) : undefined

/**
 * 只读取实例投影上的当前节点与当前审批人。
 *
 * 缺失时省略，不得用定义首节点、待复核或默认称谓补位。
 *
 * @param approval 只读审批投影。
 */
export const readCustomerRefundApprovalResponsibility = (
    approval?: DocumentApprovalView,
): {
    nextResponsible?: string
    currentNodeLabel?: string
} => ({
    nextResponsible:
        approval?.instance?.currentAssigneeName ??
        approval?.instance?.currentAssignee,
    currentNodeLabel:
        approval?.instance?.currentNodeName ?? approval?.instance?.currentNode,
})

/**
 * 判断当前任务是否属于客户退款单。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 */
export const isCustomerRefundWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    workItem?.businessObjectType === CUSTOMER_REFUND_DOCUMENT_TYPE ||
    workItem?.businessObjectType === CUSTOMER_REFUND_OBJECT_TYPE

/**
 * 构造客户退款提交审批请求。只允许版本与幂等键。
 *
 * @param input 提交所需字段。
 */
export const buildCustomerRefundSubmitRequest = (input: {
    expectedVersion: number
    idempotencyKey: string
}): Readonly<{
    expected_version: number
    idempotency_key: string
}> => ({
    expected_version: input.expectedVersion,
    idempotency_key: input.idempotencyKey,
})

/** 客户退款提交意图对应的幂等槽。 */
export type CustomerRefundIdempotencySlot = Readonly<{
    key: string
    fingerprint: string
    refundId?: string
}>

/**
 * 计算客户退款草稿意图指纹。
 *
 * 来源单据或原因变化即视为新意图，不得复用上一张退款单的键。
 *
 * @param scopeId 原回款 ID 或已有退款单 ID。
 * @param reason 退款原因。
 */
export const customerRefundIntentFingerprint = (
    scopeId: string,
    reason: string,
): string => `${scopeId}:${reason.trim()}`

/**
 * 按当前意图复用或轮换退款幂等键。
 *
 * 同一来源且同一原因的重试保持原键；换单或改原因必须换新键。
 *
 * @param current 当前槽位。
 * @param scopeId 原回款 ID 或已有退款单 ID。
 * @param reason 退款原因。
 */
export const slotForCustomerRefundIntent = (
    current: CustomerRefundIdempotencySlot | null,
    scopeId: string,
    reason: string,
): CustomerRefundIdempotencySlot => {
    const fingerprint = customerRefundIntentFingerprint(scopeId, reason)
    if (current && current.fingerprint === fingerprint) return current
    return {
        key: `w11-rev-${scopeId}-${crypto.randomUUID()}`,
        fingerprint,
    }
}
