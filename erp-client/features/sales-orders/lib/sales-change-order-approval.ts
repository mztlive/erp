import {
    filterAllowedActions,
    mapDocumentApprovalViewDto,
    type ApprovalAllowedAction,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
} from "@/features/approval-workflow/types"

/** 销售变更单作为合同 DocumentType 的固定值。 */
export const SALES_CHANGE_ORDER_DOCUMENT_TYPE = "SalesChangeOrder" as const

export type SalesChangeOrderApprovalPhase = "draft" | "confirm" | "runtime"

const UNSUBMITTED_STATUS_CODES = new Set([
    "DRAFT",
    "draft",
    "REJECTED",
    "rejected",
])

/**
 * 判断销售变更单是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端状态码；缺省视为未提交。
 * @returns 未提交为 true。
 */
export const isUnsubmittedSalesChangeOrderStatus = (status?: string): boolean =>
    !status || UNSUBMITTED_STATUS_CODES.has(status)

/**
 * 把销售变更单业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * 旧的影响确认/财务复核状态已收敛为审批中，不得再拆成独立复核节点。
 *
 * @param status 服务端状态码。
 */
export const salesChangeOrderStatusLabel = (status?: string): string => {
    switch (status) {
        case "DRAFT":
        case "draft":
        case "REJECTED":
        case "rejected":
            return "草稿"
        case "IN_APPROVAL":
        case "PENDING_IMPACT_CONFIRMATION":
        case "PENDING_FINANCE_REVIEW":
            return "审批中"
        case "EFFECTIVE":
            return "已生效"
        case "VOIDED":
            return "已作废"
        default:
            return "改单中"
    }
}

/**
 * 按单据生命周期选择销售变更审批区相位。提交确认由调用方显式传入。
 *
 * 有运行实例或已离开未提交阶段时进入 runtime，不得按角色、影响路径或 BusinessType 推导。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据状态码。
 */
export const salesChangeOrderApprovalPhase = (
    approval?: DocumentApprovalView,
    status?: string,
): Exclude<SalesChangeOrderApprovalPhase, "confirm"> =>
    approval?.instance || !isUnsubmittedSalesChangeOrderStatus(status)
        ? "runtime"
        : "draft"

/**
 * 合并销售变更单与当前任务的服务端动作白名单。只做并集过滤，不补默认动作。
 *
 * @param documentActions 单据 `allowed_actions`。
 * @param workItemActions 当前任务 `allowed_actions`。
 */
export const mergeSalesChangeOrderAllowedActions = (
    documentActions?: readonly ApprovalAllowedAction[] | readonly string[],
    workItemActions?: readonly string[],
): readonly ApprovalAllowedAction[] =>
    filterAllowedActions([
        ...(documentActions ?? []),
        ...(workItemActions ?? []),
    ])

/**
 * 把销售变更单详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapSalesChangeOrderApproval = (
    dto?: DocumentApprovalViewDto | null,
): DocumentApprovalView | undefined =>
    dto ? mapDocumentApprovalViewDto(dto) : undefined

/**
 * 只读取实例投影上的当前节点与当前审批人。
 *
 * 缺失时省略，不得用定义首节点、影响路径或默认称谓补位。
 *
 * @param approval 只读审批投影。
 */
export const readSalesChangeOrderApprovalResponsibility = (
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
