import {
    mergeSalesOrderAllowedActions,
    salesOrderApprovalPhase,
    type SalesOrderApprovalPhase,
} from "@/features/sales-orders/lib/sales-order-approval"
import {
    filterAllowedActions,
    mapDocumentApprovalViewDto,
    type ApprovalAllowedAction,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
} from "@/features/approval-workflow/types"

/** 卡券销售单作为合同 DocumentType 的固定值。 */
export const VOUCHER_SALES_ORDER_DOCUMENT_TYPE = "VoucherSalesOrder" as const

export type VoucherSalesOrderApprovalPhase = SalesOrderApprovalPhase

/**
 * 按单据生命周期选择卡券销售单审批区相位。提交确认由调用方显式传入。
 *
 * 有运行实例或已离开未提交阶段时进入 runtime，不得按角色、运营节点或 BusinessType 推导。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据阶段码。
 */
export const voucherSalesOrderApprovalPhase = (
    approval?: DocumentApprovalView,
    status?: string,
): Exclude<VoucherSalesOrderApprovalPhase, "confirm"> =>
    salesOrderApprovalPhase(approval, status)

/**
 * 合并卡券销售单与当前任务的服务端动作白名单。只做并集过滤，不补默认动作。
 *
 * @param documentActions 单据 `allowed_actions`。
 * @param workItemActions 当前任务 `allowed_actions`。
 */
export const mergeVoucherSalesOrderAllowedActions = (
    documentActions?: readonly ApprovalAllowedAction[] | readonly string[],
    workItemActions?: readonly string[],
): readonly ApprovalAllowedAction[] =>
    mergeSalesOrderAllowedActions(documentActions, workItemActions)

/**
 * 把卡券销售单详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapVoucherSalesOrderApproval = (
    dto?: DocumentApprovalViewDto | null,
): DocumentApprovalView | undefined =>
    dto ? mapDocumentApprovalViewDto(dto) : undefined

/**
 * 只读取实例投影上的当前节点与当前审批人。
 *
 * 缺失时省略，不得用定义首节点、运营节点或默认称谓补位。
 *
 * @param approval 只读审批投影。
 */
export const readVoucherSalesOrderApprovalResponsibility = (
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
 * 过滤卡券销售单任务动作，只保留审批运行时允许的动作。
 *
 * @param actions 单据或任务上的原始动作。
 */
export const filterVoucherSalesOrderAllowedActions = (
    actions: readonly string[],
): readonly ApprovalAllowedAction[] => filterAllowedActions(actions)
