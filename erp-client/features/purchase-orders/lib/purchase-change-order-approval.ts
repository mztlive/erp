import type { StatusTone } from "@/components/ui/status-badge"
import {
    filterAllowedActions,
    mapDocumentApprovalViewDto,
    type ApprovalAllowedAction,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
} from "@/features/approval-workflow/types"

/** 采购变更单作为合同 DocumentType 的固定值。 */
export const PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE =
    "PurchaseChangeOrder" as const

/** 工作项上的采购变更对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const PURCHASE_CHANGE_ORDER_OBJECT_TYPE = "purchase_change_order" as const

export type PurchaseChangeOrderApprovalPhase = "draft" | "confirm" | "runtime"

const UNSUBMITTED_STATUS_CODES = new Set([
    "DRAFT",
    "draft",
    "REJECTED",
    "rejected",
])

/**
 * 判断采购变更单是否仍处于未提交审批的生命周期。
 *
 * @param status 服务端状态码；缺省视为未提交。
 * @returns 未提交为 true。
 */
export const isUnsubmittedPurchaseChangeOrderStatus = (
    status?: string,
): boolean => !status || UNSUBMITTED_STATUS_CODES.has(status)

/**
 * 把采购变更单业务状态映射为用户可见中文，不上屏枚举原值。
 *
 * 旧的仓配影响/财务复核状态已收敛为审批中，不得再拆成独立复核节点。
 *
 * @param status 服务端状态码。
 */
export const purchaseChangeOrderStatusLabel = (status?: string): string => {
    switch (status) {
        case "DRAFT":
        case "draft":
        case "REJECTED":
        case "rejected":
            return "草稿"
        case "IN_APPROVAL":
        case "PENDING_WAREHOUSE_IMPACT":
        case "PENDING_FINANCE_REVIEW":
            return "审批中"
        case "EFFECTIVE":
            return "已生效"
        case "VOIDED":
        case "VOID":
            return "已作废"
        default:
            return "改单中"
    }
}

/**
 * 采购变更状态对应的列表色调。未知码按进行中处理，不上屏枚举。
 *
 * @param status 服务端状态码。
 */
export const purchaseChangeOrderStatusTone = (status?: string): StatusTone => {
    switch (purchaseChangeOrderStatusLabel(status)) {
        case "已生效":
            return "success"
        case "审批中":
        case "改单中":
            return "warning"
        default:
            return "neutral"
    }
}

/**
 * 按单据生命周期选择采购变更审批区相位。提交确认由调用方显式传入。
 *
 * 有运行实例或已离开未提交阶段时进入 runtime，不得按仓配影响或财务复核推导。
 *
 * @param approval 服务端只读审批投影。
 * @param status 单据状态码。
 */
export const purchaseChangeOrderApprovalPhase = (
    approval?: DocumentApprovalView,
    status?: string,
): Exclude<PurchaseChangeOrderApprovalPhase, "confirm"> =>
    approval?.instance || !isUnsubmittedPurchaseChangeOrderStatus(status)
        ? "runtime"
        : "draft"

/**
 * 合并采购变更单与当前任务的服务端动作白名单。只做并集过滤，不补默认动作。
 *
 * @param documentActions 单据 `allowed_actions`。
 * @param workItemActions 当前任务 `allowed_actions`。
 */
export const mergePurchaseChangeOrderAllowedActions = (
    documentActions?: readonly ApprovalAllowedAction[] | readonly string[],
    workItemActions?: readonly string[],
): readonly ApprovalAllowedAction[] =>
    filterAllowedActions([
        ...(documentActions ?? []),
        ...(workItemActions ?? []),
    ])

/**
 * 把采购变更单详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省返回 undefined，禁止前端补默认审批人或节点。
 *
 * @param dto 详情内嵌的审批 DTO。
 */
export const mapPurchaseChangeOrderApproval = (
    dto?: DocumentApprovalViewDto | null,
): DocumentApprovalView | undefined =>
    dto ? mapDocumentApprovalViewDto(dto) : undefined

/**
 * 只读取实例投影上的当前节点与当前审批人。
 *
 * 缺失时省略，不得用定义首节点、仓配影响或默认称谓补位。
 *
 * @param approval 只读审批投影。
 */
export const readPurchaseChangeOrderApprovalResponsibility = (
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
 * 判断当前任务是否属于采购变更单，而不是原采购单。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 */
export const isPurchaseChangeOrderWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    workItem?.businessObjectType === PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE ||
    workItem?.businessObjectType === PURCHASE_CHANGE_ORDER_OBJECT_TYPE
