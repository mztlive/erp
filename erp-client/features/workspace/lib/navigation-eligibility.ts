/**
 * 工作台只把服务端动作当作打开单据或页内决定的资格。
 * 不得把领取、开始处理视为本页动作。
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
