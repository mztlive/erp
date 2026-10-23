import type { WorkspaceWorkItem } from "../types"

/** 采购审批与只读审批跟踪共用单列核对工作面。 */
export function isPurchaseApprovalTask(item: WorkspaceWorkItem): boolean {
    return (
        item.businessObjectType === "purchase_order" &&
        Boolean(item.approvalProcessInstanceId ?? item.approval?.instanceId) &&
        (isApprovalWorkbenchTask(
            item.allowedActions,
            item.approvalProcessInstanceId,
            item.approvalNodeExecutionId,
        ) ||
            item.workItemType === "APPROVAL_INSTANCE" ||
            item.workItemType === "DOCUMENT_APPROVAL")
    )
}

/**
 * 工作台只把服务端动作当作打开单据或页内决定的资格。
 * 只接受查看、打开对象或正式领域处理动作。
 */
export function canOpenWorkItemHandler(
    allowedActions: readonly string[],
    hasProcessBlocker: boolean,
): boolean {
    return (
        !hasProcessBlocker &&
        (allowedActions.includes("OPEN_DOCUMENT") ||
            allowedActions.includes("PROCESS") ||
            allowedActions.includes("VIEW"))
    )
}

/**
 * 判断是否为可在详情内提交的审批任务。
 */
export function isApprovalWorkbenchTask(
    allowedActions: readonly string[],
    approvalProcessInstanceId?: string,
    approvalNodeExecutionId?: string,
): boolean {
    return (
        Boolean(approvalProcessInstanceId || approvalNodeExecutionId) ||
        allowedActions.includes("APPROVE") ||
        allowedActions.includes("REJECT")
    )
}
