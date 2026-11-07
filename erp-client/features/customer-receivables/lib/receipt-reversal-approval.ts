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
import type { ReceiptReversalRow } from "@/features/customer-receivables/types"

/** 回款冲正单作为合同 DocumentType 的固定值。 */
export const RECEIPT_REVERSAL_DOCUMENT_TYPE = "ReceiptReversal" as const

/** 工作项上的回款冲正对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const RECEIPT_REVERSAL_OBJECT_TYPE = "receipt_reversal" as const

export type ReceiptReversalApprovalPhase = FinancialApprovalPhase

/**
 * 判断回款冲正单是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端状态码；缺省视为未提交。
 * @returns 未提交为 true。
 */
export const isUnsubmittedReceiptReversalStatus = isUnsubmittedFinancialStatus

/**
 * 把回款冲正业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * @param status 服务端状态码。
 */
export const receiptReversalStatusLabel = (status?: string): string =>
    financialDocumentStatusLabel(status, "冲正单")

/**
 * 回款冲正状态对应的列表色调。未知码按进行中处理，不上屏枚举。
 *
 * @param status 服务端状态码。
 */
export const receiptReversalStatusTone = (
    status?: string,
): ReceiptReversalRow["statusTone"] => financialDocumentStatusTone(status)

/**
 * 按单据生命周期选择回款冲正审批区相位。提交确认由调用方显式传入。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据状态码。
 */
export const receiptReversalApprovalPhase = financialDocumentApprovalPhase

/**
 * 把回款冲正详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapReceiptReversalApproval = mapFinancialDocumentApproval

/**
 * 判断当前任务是否属于回款冲正单。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 */
export const isReceiptReversalWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    isFinancialDocumentWorkItem(
        workItem,
        RECEIPT_REVERSAL_DOCUMENT_TYPE,
        RECEIPT_REVERSAL_OBJECT_TYPE,
    )

/**
 * 构造回款冲正提交审批请求。包含版本、重提说明与幂等键。
 *
 * @param input 提交所需字段。
 */
export const buildReceiptReversalSubmitRequest = buildFinancialSubmitRequest

/** 回款冲正提交意图对应的幂等槽。 */
export type ReceiptReversalIdempotencySlot = Readonly<{
    key: string
    fingerprint: string
    reversalId?: string
}>

/**
 * 计算回款冲正草稿意图指纹。
 *
 * 来源单据或原因变化即视为新意图，不得复用上一张冲正单的键。
 *
 * @param scopeId 原回款 ID 或已有冲正单 ID。
 * @param reason 冲正原因。
 */
export const receiptReversalIntentFingerprint = (
    scopeId: string,
    reason: string,
): string => `${scopeId}:${reason.trim()}`

/**
 * 按当前意图复用或轮换冲正幂等键。
 *
 * 同一来源且同一原因的重试保持原键；换单或改原因必须换新键。
 *
 * @param current 当前槽位。
 * @param scopeId 原回款 ID 或已有冲正单 ID。
 * @param reason 冲正原因。
 */
export const slotForReceiptReversalIntent = (
    current: ReceiptReversalIdempotencySlot | null,
    scopeId: string,
    reason: string,
): ReceiptReversalIdempotencySlot => {
    const fingerprint = receiptReversalIntentFingerprint(scopeId, reason)
    if (current && current.fingerprint === fingerprint) return current
    return {
        key: `w11-rr-${scopeId}-${crypto.randomUUID()}`,
        fingerprint,
    }
}
