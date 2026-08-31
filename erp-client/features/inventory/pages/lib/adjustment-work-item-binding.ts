import type { ApprovalRuntimeInstance } from "@/features/approval-workflow/types"
import type { WorkItemProjection } from "@/features/work-items/types"

export type AdjustmentDecisionWorkItemBinding = Readonly<{
    workItemId: string
    expectedTaskVersion: string
    allowedActions: readonly string[]
}>

/**
 * 只有当 WorkItem 与当前库存调整详情的对象、实例和运行身份全部一致时，
 * 才向决定组件下发任务令牌。URL 中的任一身份不匹配都必须整组失效。
 */
export function bindAdjustmentDecisionWorkItem(
    workItem: WorkItemProjection | undefined,
    adjustmentId: string | null,
    runtime: ApprovalRuntimeInstance | undefined,
): AdjustmentDecisionWorkItemBinding | undefined {
    if (
        !workItem ||
        !adjustmentId ||
        !runtime ||
        workItem.workItemType !== "DOCUMENT_APPROVAL" ||
        workItem.status !== "OPEN" ||
        workItem.processingState !== "READY" ||
        runtime.status !== "RUNNING" ||
        workItem.businessObjectType !== "stock_adjustment" ||
        workItem.businessObjectId !== adjustmentId ||
        workItem.approvalProcessInstanceId !== runtime.id ||
        workItem.subjectVersion !== runtime.subjectVersion ||
        workItem.approvalNodeExecutionId !== runtime.currentExecutionId ||
        workItem.workItemId !== runtime.currentTaskId ||
        workItem.taskVersion !== runtime.currentTaskVersion ||
        !/^[1-9]\d*$/.test(workItem.taskVersion)
    ) {
        return undefined
    }

    return {
        workItemId: workItem.workItemId,
        expectedTaskVersion: workItem.taskVersion,
        allowedActions: workItem.allowedActions,
    }
}
