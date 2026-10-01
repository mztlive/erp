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
