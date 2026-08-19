import {
    filterAllowedActions,
    mapDocumentApprovalViewDto,
    type ApprovalAllowedAction,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
} from "@/features/approval-workflow/types"
import type { PaymentRow } from "@/features/supplier-payables/types"

/** 供应商付款单作为合同 DocumentType 的固定值。 */
export const SUPPLIER_PAYMENT_DOCUMENT_TYPE = "SupplierPayment" as const

/** 工作项上的供应商付款对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const SUPPLIER_PAYMENT_OBJECT_TYPE = "supplier_payment" as const

export type SupplierPaymentApprovalPhase = "draft" | "confirm" | "runtime"

const UNSUBMITTED_STATUS_CODES = new Set([
    "DRAFT",
    "draft",
    "REJECTED",
    "rejected",
])

/**
 * 判断供应商付款单是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端状态码；缺省视为未提交。
 * @returns 未提交为 true。
 */
export const isUnsubmittedSupplierPaymentStatus = (
    status?: string,
): boolean => !status || UNSUBMITTED_STATUS_CODES.has(status)

/**
 * 把供应商付款业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * 旧的待复核状态已收敛为审批中，不得再拆成独立复核节点。
 *
 * @param status 服务端状态码。
 */
export const supplierPaymentStatusLabel = (status?: string): string => {
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
            return "付款单"
    }
}

/**
 * 供应商付款状态对应的列表色调。未知码按进行中处理，不上屏枚举。
 *
 * @param status 服务端状态码。
 */
export const supplierPaymentStatusTone = (
    status?: string,
): PaymentRow["statusTone"] => {
    switch (supplierPaymentStatusLabel(status)) {
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
 * 按单据生命周期选择供应商付款审批区相位。提交确认由调用方显式传入。
 *
 * 有运行实例或已离开未提交阶段时进入 runtime，不得按复核或角色推导。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据状态码。
 */
export const supplierPaymentApprovalPhase = (
    approval?: DocumentApprovalView,
    status?: string,
): Exclude<SupplierPaymentApprovalPhase, "confirm"> =>
    approval?.instance || !isUnsubmittedSupplierPaymentStatus(status)
        ? "runtime"
        : "draft"

/**
 * 合并供应商付款单与当前任务的服务端动作白名单。只做并集过滤，不补默认动作。
 *
 * @param documentActions 单据 `allowed_actions`。
 * @param workItemActions 当前任务 `allowed_actions`。
 */
export const mergeSupplierPaymentAllowedActions = (
    documentActions?: readonly ApprovalAllowedAction[] | readonly string[],
    workItemActions?: readonly string[],
): readonly ApprovalAllowedAction[] =>
    filterAllowedActions([
        ...(documentActions ?? []),
        ...(workItemActions ?? []),
    ])

/**
 * 把供应商付款详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapSupplierPaymentApproval = (
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
export const readSupplierPaymentApprovalResponsibility = (
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
 * 判断当前任务是否属于供应商付款单。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 */
export const isSupplierPaymentWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    workItem?.businessObjectType === SUPPLIER_PAYMENT_DOCUMENT_TYPE ||
    workItem?.businessObjectType === SUPPLIER_PAYMENT_OBJECT_TYPE

/**
 * 构造供应商付款提交审批请求。只允许版本、幂等键与冻结分配。
 *
 * @param input 提交所需字段。
 */
export const buildSupplierPaymentSubmitRequest = (input: {
    expectedVersion: number
    idempotencyKey: string
    allocations: readonly {
        payableEntryId: string
        allocatedAmount: string
    }[]
}): Readonly<{
    expected_version: number
    idempotency_key: string
    allocations: readonly {
        payable_entry_id: string
        allocated_amount: string
    }[]
}> => ({
    expected_version: input.expectedVersion,
    idempotency_key: input.idempotencyKey,
    allocations: input.allocations.map((line) => ({
        payable_entry_id: line.payableEntryId,
        allocated_amount: line.allocatedAmount,
    })),
})
