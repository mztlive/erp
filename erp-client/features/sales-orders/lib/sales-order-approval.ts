import {
    filterAllowedActions,
    mapDocumentApprovalViewDto,
    type ApprovalAllowedAction,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
} from "@/features/approval-workflow/types"

/** 实物及服务销售单作为合同 DocumentType 的固定值。 */
export const SALES_ORDER_DOCUMENT_TYPE = "SalesOrder" as const

export type SalesOrderApprovalPhase = "draft" | "confirm" | "runtime"

const UNSUBMITTED_STATUS_CODES = new Set([
    "draft",
    "not_submitted",
    "awaiting_sales",
])

/**
 * 判断销售单是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端阶段码；缺省视为未提交。
 * @returns 未提交为 true。
 */
export const isUnsubmittedSalesOrderStatus = (status?: string): boolean =>
    !status || UNSUBMITTED_STATUS_CODES.has(status)

/**
 * 按单据生命周期选择审批区相位。提交确认由调用方显式传入。
 *
 * 有运行实例或已离开未提交阶段时进入 runtime，不得按角色或节点名推导。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据阶段码。
 */
export const salesOrderApprovalPhase = (
    approval?: DocumentApprovalView,
    status?: string,
): Exclude<SalesOrderApprovalPhase, "confirm"> =>
    approval?.instance || !isUnsubmittedSalesOrderStatus(status)
        ? "runtime"
        : "draft"

/**
 * 合并单据与当前任务的服务端动作白名单。只做并集过滤，不补默认动作。
 *
 * @param documentActions 单据 `allowed_actions`。
 * @param workItemActions 当前任务 `allowed_actions`。
 */
export const mergeSalesOrderAllowedActions = (
    documentActions?: readonly ApprovalAllowedAction[] | readonly string[],
    workItemActions?: readonly string[],
): readonly ApprovalAllowedAction[] =>
    filterAllowedActions([
        ...(documentActions ?? []),
        ...(workItemActions ?? []),
    ])

/**
 * 把销售单详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapSalesOrderApproval = (
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
export const readSalesOrderApprovalResponsibility = (
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
