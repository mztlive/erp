import {
    financialDocumentApprovalPhase,
    financialDocumentStatusLabel,
    financialDocumentStatusTone,
    isFinancialDocumentWorkItem,
    isUnsubmittedFinancialStatus,
    mapFinancialDocumentApproval,
    type FinancialApprovalPhase,
} from "@/features/approval-workflow/financial-document"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type { ReceiptRow } from "@/features/customer-receivables/types"

/** 客户回款单作为合同 DocumentType 的固定值。 */
export const CUSTOMER_RECEIPT_DOCUMENT_TYPE = "CustomerReceipt" as const

/** 工作项上的客户回款对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const CUSTOMER_RECEIPT_OBJECT_TYPE = "customer_receipt" as const

export type CustomerReceiptApprovalPhase = FinancialApprovalPhase

/**
 * 判断客户回款单是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端状态码；缺省视为未提交。
 * @returns 未提交为 true。
 */
export const isUnsubmittedCustomerReceiptStatus = isUnsubmittedFinancialStatus

/**
 * 把客户回款业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * @param status 服务端状态码。
 */
export const customerReceiptStatusLabel = (status?: string): string =>
    financialDocumentStatusLabel(status, "回款单")

/**
 * 客户回款状态对应的列表色调。未知码按进行中处理，不上屏枚举。
 *
 * @param status 服务端状态码。
 */
export const customerReceiptStatusTone = (
    status?: string,
): ReceiptRow["statusTone"] => financialDocumentStatusTone(status)

/**
 * 按单据生命周期选择客户回款审批区相位。提交确认由调用方显式传入。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据状态码。
 */
export const customerReceiptApprovalPhase = financialDocumentApprovalPhase

/**
 * 把客户回款详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapCustomerReceiptApproval = mapFinancialDocumentApproval

/**
 * 只读取实例投影上的当前节点与当前审批人。
 *
 * 缺失时省略，不得用定义首节点、待复核或默认称谓补位。
 *
 * @param approval 只读审批投影。
 */
export const readCustomerReceiptApprovalResponsibility = (
    approval?: DocumentApprovalView,
): {
    nextResponsible?: string
    currentNodeLabel?: string
} => ({
    nextResponsible: responsibilityName(
        approval?.instance?.currentAssigneeName,
        approval?.instance?.currentAssignee,
    ),
    currentNodeLabel: responsibilityName(
        approval?.instance?.currentNodeName,
        approval?.instance?.currentNode,
    ),
})

function responsibilityName(
    value?: string,
    internalId?: string,
): string | undefined {
    const name = value?.trim()
    return name && name !== internalId?.trim() ? name : undefined
}

/**
 * 判断当前任务是否属于客户回款单。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 */
export const isCustomerReceiptWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    isFinancialDocumentWorkItem(
        workItem,
        CUSTOMER_RECEIPT_DOCUMENT_TYPE,
        CUSTOMER_RECEIPT_OBJECT_TYPE,
    )
