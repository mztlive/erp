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
import type { SupplierRefundRow } from "@/features/supplier-payables/types"

/** 供应商退款单作为合同 DocumentType 的固定值。 */
export const SUPPLIER_REFUND_DOCUMENT_TYPE = "SupplierRefund" as const

/** 工作项上的供应商退款对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const SUPPLIER_REFUND_OBJECT_TYPE = "supplier_refund" as const

export type SupplierRefundApprovalPhase = FinancialApprovalPhase

/**
 * 判断供应商退款单是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端状态码；缺省视为未提交。
 * @returns 未提交为 true。
 */
export const isUnsubmittedSupplierRefundStatus = isUnsubmittedFinancialStatus

/**
 * 把供应商退款业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * @param status 服务端状态码。
 */
export const supplierRefundStatusLabel = (status?: string): string =>
    financialDocumentStatusLabel(status, "退款单")

/**
 * 供应商退款状态对应的列表色调。未知码按进行中处理，不上屏枚举。
 *
 * @param status 服务端状态码。
 */
export const supplierRefundStatusTone = (
    status?: string,
): SupplierRefundRow["statusTone"] => financialDocumentStatusTone(status)

/**
 * 按单据生命周期选择供应商退款审批区相位。提交确认由调用方显式传入。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据状态码。
 */
export const supplierRefundApprovalPhase = financialDocumentApprovalPhase

/**
 * 把供应商退款详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapSupplierRefundApproval = mapFinancialDocumentApproval

/**
 * 判断当前任务是否属于供应商退款单。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 */
export const isSupplierRefundWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    isFinancialDocumentWorkItem(
        workItem,
        SUPPLIER_REFUND_DOCUMENT_TYPE,
        SUPPLIER_REFUND_OBJECT_TYPE,
    )

/**
 * 构造供应商退款提交审批请求。包含版本、重提说明与幂等键。
 *
 * @param input 提交所需字段。
 */
export const buildSupplierRefundSubmitRequest = buildFinancialSubmitRequest

/** 供应商退款提交意图对应的幂等槽。 */
export type SupplierRefundIdempotencySlot = Readonly<{
    key: string
    fingerprint: string
    refundId?: string
}>

/**
 * 计算供应商退款草稿意图指纹。
 *
 * 来源单据、原因或金额变化即视为新意图，不得复用上一张退款单的键。
 *
 * @param scopeId 原付款 ID 或已有退款单 ID。
 * @param reason 退款原因。
 * @param amount 退款金额。
 */
export const supplierRefundIntentFingerprint = (
    scopeId: string,
    reason: string,
    amount?: string,
): string => JSON.stringify([scopeId, reason.trim(), amount ?? null])

/**
 * 按当前意图复用或轮换退款幂等键。
 *
 * 同一来源且同一原因的重试保持原键；换单或改原因必须换新键。
 *
 * @param current 当前槽位。
 * @param scopeId 原付款 ID 或已有退款单 ID。
 * @param reason 退款原因。
 * @param amount 退款金额。
 */
export const slotForSupplierRefundIntent = (
    current: SupplierRefundIdempotencySlot | null,
    scopeId: string,
    reason: string,
    amount?: string,
): SupplierRefundIdempotencySlot => {
    const fingerprint = supplierRefundIntentFingerprint(scopeId, reason, amount)
    if (current && current.fingerprint === fingerprint) return current
    return {
        key: `w12-rev-${scopeId}-${crypto.randomUUID()}`,
        fingerprint,
    }
}
