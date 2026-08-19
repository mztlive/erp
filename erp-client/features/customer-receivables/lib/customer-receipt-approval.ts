import {
    filterAllowedActions,
    mapDocumentApprovalViewDto,
    type ApprovalAllowedAction,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
} from "@/features/approval-workflow/types"
import type { ReceiptRow } from "@/features/customer-receivables/types"

/** 客户回款单作为合同 DocumentType 的固定值。 */
export const CUSTOMER_RECEIPT_DOCUMENT_TYPE = "CustomerReceipt" as const

/** 工作项上的客户回款对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const CUSTOMER_RECEIPT_OBJECT_TYPE = "customer_receipt" as const

export type CustomerReceiptApprovalPhase = "draft" | "confirm" | "runtime"

const UNSUBMITTED_STATUS_CODES = new Set([
    "DRAFT",
    "draft",
    "REJECTED",
    "rejected",
])

/**
 * 判断客户回款单是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端状态码；缺省视为未提交。
 * @returns 未提交为 true。
 */
export const isUnsubmittedCustomerReceiptStatus = (
    status?: string,
): boolean => !status || UNSUBMITTED_STATUS_CODES.has(status)

/**
 * 把客户回款业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * 旧的待复核状态已收敛为审批中，不得再拆成独立复核节点。
 *
 * @param status 服务端状态码。
 */
export const customerReceiptStatusLabel = (status?: string): string => {
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
            return "回款单"
    }
}

/**
 * 客户回款状态对应的列表色调。未知码按进行中处理，不上屏枚举。
 *
 * @param status 服务端状态码。
 */
export const customerReceiptStatusTone = (
    status?: string,
): ReceiptRow["statusTone"] => {
    switch (customerReceiptStatusLabel(status)) {
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
 * 按单据生命周期选择客户回款审批区相位。提交确认由调用方显式传入。
 *
 * 有运行实例或已离开未提交阶段时进入 runtime，不得按复核或角色推导。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据状态码。
 */
export const customerReceiptApprovalPhase = (
    approval?: DocumentApprovalView,
    status?: string,
): Exclude<CustomerReceiptApprovalPhase, "confirm"> =>
    approval?.instance || !isUnsubmittedCustomerReceiptStatus(status)
        ? "runtime"
        : "draft"

/**
 * 合并客户回款单与当前任务的服务端动作白名单。只做并集过滤，不补默认动作。
 *
 * @param documentActions 单据 `allowed_actions`。
 * @param workItemActions 当前任务 `allowed_actions`。
 */
export const mergeCustomerReceiptAllowedActions = (
    documentActions?: readonly ApprovalAllowedAction[] | readonly string[],
    workItemActions?: readonly string[],
): readonly ApprovalAllowedAction[] =>
    filterAllowedActions([
        ...(documentActions ?? []),
        ...(workItemActions ?? []),
    ])

/**
 * 把客户回款详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapCustomerReceiptApproval = (
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
export const readCustomerReceiptApprovalResponsibility = (
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
 * 判断当前任务是否属于客户回款单。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 */
export const isCustomerReceiptWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    workItem?.businessObjectType === CUSTOMER_RECEIPT_DOCUMENT_TYPE ||
    workItem?.businessObjectType === CUSTOMER_RECEIPT_OBJECT_TYPE

/**
 * 构造客户回款提交审批请求。只允许版本、幂等键与冻结分配。
 *
 * @param input 提交所需字段。
 */
export const buildCustomerReceiptSubmitRequest = (input: {
    expectedVersion: number
    idempotencyKey: string
    allocations: readonly {
        receivableEntryId: string
        allocatedAmount: string
    }[]
}): Readonly<{
    expected_version: number
    idempotency_key: string
    allocations: readonly {
        receivable_entry_id: string
        allocated_amount: string
    }[]
}> => ({
    expected_version: input.expectedVersion,
    idempotency_key: input.idempotencyKey,
    allocations: input.allocations.map((line) => ({
        receivable_entry_id: line.receivableEntryId,
        allocated_amount: line.allocatedAmount,
    })),
})
