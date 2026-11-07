import {
    buildFinancialSubmitRequest,
    financialDocumentApprovalPhase,
    financialDocumentStatusLabel,
    financialDocumentStatusTone,
    isFinancialDocumentWorkItem,
    isUnsubmittedFinancialStatus,
    mapFinancialDocumentApproval,
    type FinancialApprovalPhase,
} from "@/features/approval-workflow/financial-document"
import type { CustomerRefundRow } from "@/features/customer-receivables/types"

/** 客户退款单作为合同 DocumentType 的固定值。 */
export const CUSTOMER_REFUND_DOCUMENT_TYPE = "CustomerRefund" as const

/** 工作项上的客户退款对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const CUSTOMER_REFUND_OBJECT_TYPE = "customer_refund" as const

export type CustomerRefundApprovalPhase = FinancialApprovalPhase

/**
 * 判断客户退款单是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端状态码；缺省视为未提交。
 * @returns 未提交为 true。
 */
export const isUnsubmittedCustomerRefundStatus = isUnsubmittedFinancialStatus

/**
 * 把客户退款业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * @param status 服务端状态码。
 */
export const customerRefundStatusLabel = (status?: string): string =>
    financialDocumentStatusLabel(status, "退款单")

/**
 * 客户退款状态对应的列表色调。未知码按进行中处理，不上屏枚举。
 *
 * @param status 服务端状态码。
 */
export const customerRefundStatusTone = (
    status?: string,
): CustomerRefundRow["statusTone"] => financialDocumentStatusTone(status)

/**
 * 按单据生命周期选择客户退款审批区相位。提交确认由调用方显式传入。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据状态码。
 */
export const customerRefundApprovalPhase = financialDocumentApprovalPhase

/**
 * 把客户退款详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapCustomerRefundApproval = mapFinancialDocumentApproval

/**
 * 判断当前任务是否属于客户退款单。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 */
export const isCustomerRefundWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    isFinancialDocumentWorkItem(
        workItem,
        CUSTOMER_REFUND_DOCUMENT_TYPE,
        CUSTOMER_REFUND_OBJECT_TYPE,
    )

/**
 * 构造客户退款提交审批请求。包含版本、重提说明与幂等键。
 *
 * @param input 提交所需字段。
 */
export const buildCustomerRefundSubmitRequest = buildFinancialSubmitRequest

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
