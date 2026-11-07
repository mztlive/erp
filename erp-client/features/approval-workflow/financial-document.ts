/**
 * 资金类单据（回款、退款、付款冲正）审批区的公共投影。
 *
 * 这些单据的状态码、审批相位、工作项对象类型判断和提交请求结构一致；各业务
 * 模块只保留自己的文档类型常量、中文兜底名与幂等键规则。
 */

import {
    mapDocumentApprovalViewDto,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
} from "./types"
import { isUnsubmittedStatus } from "./status"

export type FinancialApprovalPhase = "draft" | "confirm" | "runtime"

/** 资金单据只使用这四种色调；比通用 StatusTone 更窄，可直接赋给各业务行类型。 */
export type FinancialStatusTone =
    | "success"
    | "destructive"
    | "warning"
    | "neutral"

/**
 * 判断资金单据是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端状态码；缺省视为未提交。
 */
export const isUnsubmittedFinancialStatus = isUnsubmittedStatus

/**
 * 把资金单据业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * 旧的待复核状态已收敛为审批中，不得再拆成独立复核节点。
 *
 * @param status 服务端状态码。
 * @param fallback 未知状态时使用的单据名。
 */
export const financialDocumentStatusLabel = (
    status: string | undefined,
    fallback: string,
): string => {
    switch (status) {
        case "DRAFT":
        case "draft":
            return "草稿"
        case "REJECTED":
        case "rejected":
            return "已驳回"
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
            return fallback
    }
}

/**
 * 资金单据状态对应的列表色调。未知码按进行中处理，不上屏枚举。
 *
 * @param status 服务端状态码。
 */
export const financialDocumentStatusTone = (
    status?: string,
): FinancialStatusTone => {
    switch (financialDocumentStatusLabel(status, "")) {
        case "已过账":
            return "success"
        case "审批中":
            return "warning"
        case "已驳回":
        case "已冲正":
            return "destructive"
        default:
            return "neutral"
    }
}

/** 资金单据行状态。未知码归草稿，不把枚举原值写进行上。 */
export type FinancialDocumentStatus =
    | "draft"
    | "rejected"
    | "in_approval"
    | "posted"
    | "reversed"

/**
 * 把资金单据服务端状态码收成行状态。
 *
 * 旧的待复核状态已收敛为审批中。未识别的码，包括草稿的两种大小写，都归草稿。
 *
 * @param status 服务端状态码。
 */
export function mapFinancialDocumentStatus(
    status: string,
): FinancialDocumentStatus {
    switch (status) {
        case "REJECTED":
        case "rejected":
            return "rejected"
        case "reversed":
        case "REVERSED":
            return "reversed"
        case "posted":
        case "POSTED":
            return "posted"
        case "IN_APPROVAL":
        case "in_approval":
        case "pending_review":
        case "PENDING_REVIEW":
            return "in_approval"
        default:
            return "draft"
    }
}

/**
 * 按单据生命周期选择审批区相位。提交确认由调用方显式传入。
 *
 * 有运行实例或已离开未提交阶段时进入 runtime，不得按复核或角色推导。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据状态码。
 */
export const financialDocumentApprovalPhase = (
    approval?: DocumentApprovalView,
    status?: string,
): Exclude<FinancialApprovalPhase, "confirm"> =>
    approval?.instance || !isUnsubmittedFinancialStatus(status)
        ? "runtime"
        : "draft"

/**
 * 把详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapFinancialDocumentApproval = (
    dto?: DocumentApprovalViewDto | null,
): DocumentApprovalView | undefined =>
    dto ? mapDocumentApprovalViewDto(dto) : undefined

/**
 * 判断当前任务是否属于指定单据；同时接受合同 DocumentType 与对象类型。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 * @param documentType 合同 DocumentType。
 * @param objectType 工作项对象类型。
 */
export const isFinancialDocumentWorkItem = (
    workItem: { businessObjectType?: string } | undefined,
    documentType: string,
    objectType: string,
): boolean =>
    workItem?.businessObjectType === documentType ||
    workItem?.businessObjectType === objectType

/**
 * 构造资金单据提交审批请求。包含版本、重提说明与幂等键。
 *
 * @param input 提交所需字段。
 */
export const buildFinancialSubmitRequest = (input: {
    expectedVersion: number
    idempotencyKey: string
    resubmissionReason?: string
}): Readonly<{
    expected_version: number
    idempotency_key: string
    resubmission_reason?: string
}> => ({
    resubmission_reason: input.resubmissionReason,
    expected_version: input.expectedVersion,
    idempotency_key: input.idempotencyKey,
})
