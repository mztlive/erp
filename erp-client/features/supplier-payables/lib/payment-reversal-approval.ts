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
import type { PaymentReversalRow } from "@/features/supplier-payables/types"

/** 付款冲正单作为合同 DocumentType 的固定值。 */
export const PAYMENT_REVERSAL_DOCUMENT_TYPE = "PaymentReversal" as const

/** 工作项上的付款冲正对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const PAYMENT_REVERSAL_OBJECT_TYPE = "payment_reversal" as const

export type PaymentReversalApprovalPhase = FinancialApprovalPhase

/**
 * 判断付款冲正单是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端状态码；缺省视为未提交。
 * @returns 未提交为 true。
 */
export const isUnsubmittedPaymentReversalStatus = isUnsubmittedFinancialStatus

/**
 * 把付款冲正业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * @param status 服务端状态码。
 */
export const paymentReversalStatusLabel = (status?: string): string =>
    financialDocumentStatusLabel(status, "冲正单")

/**
 * 付款冲正状态对应的列表色调。未知码按进行中处理，不上屏枚举。
 *
 * @param status 服务端状态码。
 */
export const paymentReversalStatusTone = (
    status?: string,
): PaymentReversalRow["statusTone"] => financialDocumentStatusTone(status)

/**
 * 按单据生命周期选择付款冲正审批区相位。提交确认由调用方显式传入。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据状态码。
 */
export const paymentReversalApprovalPhase = financialDocumentApprovalPhase

/**
 * 把付款冲正详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapPaymentReversalApproval = mapFinancialDocumentApproval

/**
 * 判断当前任务是否属于付款冲正单。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 */
export const isPaymentReversalWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    isFinancialDocumentWorkItem(
        workItem,
        PAYMENT_REVERSAL_DOCUMENT_TYPE,
        PAYMENT_REVERSAL_OBJECT_TYPE,
    )

/**
 * 构造付款冲正提交审批请求。包含版本、重提说明与幂等键。
 *
 * @param input 提交所需字段。
 */
export const buildPaymentReversalSubmitRequest = buildFinancialSubmitRequest

/** 付款冲正提交意图对应的幂等槽。 */
export type PaymentReversalIdempotencySlot = Readonly<{
    key: string
    fingerprint: string
    reversalId?: string
}>

/**
 * 计算付款冲正草稿意图指纹。
 *
 * 来源单据或原因变化即视为新意图，不得复用上一张冲正单的键。
 *
 * @param scopeId 原付款 ID 或已有冲正单 ID。
 * @param reason 冲正原因。
 */
export const paymentReversalIntentFingerprint = (
    scopeId: string,
    reason: string,
): string => `${scopeId}:${reason.trim()}`

/**
 * 按当前意图复用或轮换冲正幂等键。
 *
 * 同一来源且同一原因的重试保持原键；换单或改原因必须换新键。
 *
 * @param current 当前槽位。
 * @param scopeId 原付款 ID 或已有冲正单 ID。
 * @param reason 冲正原因。
 */
export const slotForPaymentReversalIntent = (
    current: PaymentReversalIdempotencySlot | null,
    scopeId: string,
    reason: string,
): PaymentReversalIdempotencySlot => {
    const fingerprint = paymentReversalIntentFingerprint(scopeId, reason)
    if (current && current.fingerprint === fingerprint) return current
    return {
        key: `w12-pr-${scopeId}-${crypto.randomUUID()}`,
        fingerprint,
    }
}
