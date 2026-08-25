import {
    filterAllowedActions,
    mapDocumentApprovalViewDto,
    type ApprovalAllowedAction,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
} from "@/features/approval-workflow/types"

/** 采购单作为合同 DocumentType 的固定值。 */
export const PURCHASE_ORDER_DOCUMENT_TYPE = "PurchaseOrder" as const

export type PurchaseOrderApprovalPhase = "draft" | "confirm" | "runtime"

const UNSUBMITTED_STATUS_CODES = new Set([
    "DRAFT",
    "draft",
    "REJECTED",
    "rejected",
])

/**
 * 判断采购单是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端或前端状态码；缺省视为未提交。
 * @returns 未提交为 true。
 */
export const isUnsubmittedPurchaseOrderStatus = (status?: string): boolean =>
    !status || UNSUBMITTED_STATUS_CODES.has(status)

/**
 * 把采购单业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * 旧的待财务审核状态已收敛为审批中，不得再拆成独立复核节点。
 *
 * @param status 服务端或前端状态码。
 */
export const purchaseOrderStatusLabel = (status?: string): string => {
    switch (status) {
        case "DRAFT":
        case "draft":
        case "REJECTED":
        case "rejected":
            return "草稿"
        case "IN_APPROVAL":
        case "PENDING_FINANCE_REVIEW":
        case "PENDING_REVIEW":
            return "审批中"
        case "EFFECTIVE":
            return "已生效"
        case "PARTIALLY_EXECUTED":
        case "PARTIAL":
            return "部分执行"
        case "COMPLETED":
            return "已完成"
        case "VOIDED":
        case "VOID":
            return "已作废"
        default:
            return "采购单"
    }
}

/**
 * 按单据生命周期选择采购审批区相位。提交确认由调用方显式传入。
 *
 * 有运行实例或已离开未提交阶段时进入 runtime，不得按角色或节点名推导。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据状态码。
 */
export const purchaseOrderApprovalPhase = (
    approval?: DocumentApprovalView,
    status?: string,
): Exclude<PurchaseOrderApprovalPhase, "confirm"> =>
    approval?.instance || !isUnsubmittedPurchaseOrderStatus(status)
        ? "runtime"
        : "draft"

/**
 * 采购单审批是否仍在途。已通过/已撤回不得再占「进行中」。
 *
 * @param order 采购单中心视图的身份与审批投影。
 */
export const isPurchaseOrderApprovalInProgress = (order: {
    identity: { status: string }
    approval?: { allowedActions?: readonly string[] } | null
}): boolean => {
    if (order.identity.status === "PENDING_REVIEW") return true
    const actions = order.approval?.allowedActions ?? []
    return actions.includes("CANCEL") || actions.includes("CANCEL_APPROVAL")
}

/**
 * 合并采购单与当前任务的服务端动作白名单。只做并集过滤，不补默认动作。
 *
 * @param documentActions 单据 `allowed_actions`。
 * @param workItemActions 当前任务 `allowed_actions`。
 */
export const mergePurchaseOrderAllowedActions = (
    documentActions?: readonly ApprovalAllowedAction[] | readonly string[],
    workItemActions?: readonly string[],
): readonly ApprovalAllowedAction[] =>
    filterAllowedActions([
        ...(documentActions ?? []),
        ...(workItemActions ?? []),
    ])

/**
 * 把采购单详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapPurchaseOrderApproval = (
    dto?: DocumentApprovalViewDto | null,
): DocumentApprovalView | undefined =>
    dto ? mapDocumentApprovalViewDto(dto) : undefined

/**
 * 只读取实例投影上的当前节点与当前审批人。
 *
 * 缺失时省略，不得用定义首节点或默认称谓补位。
 *
 * @param approval 只读审批投影。
 */
export const readPurchaseOrderApprovalResponsibility = (
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
