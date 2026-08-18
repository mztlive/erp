"use client"

import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { ExecutionHistory } from "@/features/approval-workflow/components/execution-history"
import { RuntimeSummary } from "@/features/approval-workflow/components/runtime-summary"
import { useRecoveryOptionsQuery } from "@/features/approval-workflow/queries"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { Button } from "@/components/ui/button"

import { buildDocumentHref } from "../lib/destination"
import { isApprovalWorkbenchTask } from "../lib/navigation-eligibility"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"

/**
 * 工作台右侧详情。审批任务在页内提交决定；非审批任务只给打开单据。
 */
export function WorkspaceTaskDetail({
    item,
    canReadSensitive = true,
    onDecisionApplied,
}: {
    item: WorkspaceWorkItem
    canReadSensitive?: boolean
    onDecisionApplied?: (
        view: ApprovalCommandView,
        completedWorkItemId: string,
    ) => void
}) {
    const approvalTask = isApprovalWorkbenchTask(
        item.allowedActions,
        item.approvalProcessInstanceId,
        item.approvalNodeExecutionId,
    )
    const instanceId =
        item.approvalProcessInstanceId ?? item.approval?.instanceId
    const recoveryQuery = useRecoveryOptionsQuery(
        instanceId,
        approvalTask && isBlockedWorkItem(item),
    )
    const documentHref = buildDocumentHref(item)
    const instance = item.approval
        ? {
              id: item.approval.instanceId,
              status: item.approval.status,
              currentRoundNo: item.approval.currentRoundNo,
              currentNodeName: item.approval.currentNodeLabel,
              currentAssigneeName: item.approval.currentAssigneeLabel,
              latestRejection: item.approval.lastRejectReason,
              latestRejectionBy: item.approval.lastRejectorLabel,
              processName: item.approval.processName,
              processVersion: item.approval.processVersion,
          }
        : instanceId
          ? {
                id: instanceId,
                status: isBlockedWorkItem(item) ? "BLOCKED" : "RUNNING",
                currentRoundNo: 1,
            }
          : undefined

    return (
        <section className="space-y-4" aria-label="当前任务">
            <header className="space-y-1">
                <h2 className="text-lg font-medium">{item.objectTitle}</h2>
                {canReadSensitive ? (
                    <p className="text-sm text-muted-foreground">
                        {[item.counterpartyName, item.listSummary]
                            .filter(Boolean)
                            .join(" · ")}
                    </p>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        当前账号无权查看部分业务字段
                    </p>
                )}
            </header>

            {approvalTask ? (
                <>
                    <RuntimeSummary instance={instance} />
                    <ExecutionHistory items={[]} />
                    <ApprovalActionBar
                        allowedActions={item.allowedActions}
                        recoveryOptions={recoveryQuery.data?.actions ?? []}
                        workItemId={item.workItemId}
                        expectedTaskVersion={item.taskVersion}
                        instance={instance}
                        documentHref={documentHref}
                        canReadSensitive={canReadSensitive}
                        onDecisionApplied={(view) =>
                            onDecisionApplied?.(view, item.workItemId)
                        }
                    />
                </>
            ) : (
                <div className="space-y-3">
                    <p className="text-sm">{item.impactSummary}</p>
                    {item.actionBlockers.length > 0 ? (
                        <p className="text-sm text-muted-foreground">
                            {item.actionBlockers[0]?.message}
                        </p>
                    ) : null}
                    {documentHref ? (
                        <Button
                            type="button"
                            render={
                                <a href={documentHref} aria-label="打开单据" />
                            }
                        >
                            打开单据
                        </Button>
                    ) : null}
                </div>
            )}
        </section>
    )
}
