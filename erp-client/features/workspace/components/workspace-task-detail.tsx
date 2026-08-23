"use client"

import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { RuntimeSummary } from "@/features/approval-workflow/components/runtime-summary"
import { useRecoveryOptionsQuery } from "@/features/approval-workflow/queries"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { Button } from "@/components/ui/button"

import { useWorkspaceDocumentFacts } from "../hooks/use-workspace-document-facts"
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
    const documentFacts = useWorkspaceDocumentFacts(item)
    const facts = documentFacts.facts
    const summarySections = facts?.sections ?? item.summarySections
    const briefLines = facts?.lines ?? item.briefLines
    const briefMoreCount = facts?.moreCount ?? item.briefMoreCount
    const listSummary = facts?.listSummary ?? item.listSummary
    const counterpartyName = facts?.counterparty ?? item.counterpartyName
    const impactSummary =
        facts?.impact &&
        (item.impactSummary === "不审批则卡券销售不能生效" ||
            !item.summarySections?.length)
            ? facts.impact
            : item.impactSummary
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
        : undefined

    return (
        <section className="space-y-4" aria-label="当前任务">
            <header className="space-y-1">
                <h2 className="text-lg font-medium">{item.objectTitle}</h2>
                {canReadSensitive ? (
                    <p className="text-sm text-muted-foreground">
                        {[counterpartyName, listSummary]
                            .filter(Boolean)
                            .join(" · ")}
                    </p>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        当前账号无权查看部分业务字段
                    </p>
                )}
            </header>

            {summarySections && summarySections.length > 0 ? (
                <dl className="grid grid-cols-2 gap-x-3 gap-y-2 text-sm">
                    {summarySections.map((section) => (
                        <div key={section.label} className="min-w-0">
                            <dt className="text-xs text-muted-foreground">
                                {section.label}
                            </dt>
                            <dd
                                className={
                                    section.numeric
                                        ? "num truncate"
                                        : "truncate"
                                }
                            >
                                {section.value}
                            </dd>
                        </div>
                    ))}
                </dl>
            ) : documentFacts.isPending ? (
                <p className="text-sm text-muted-foreground">正在读取单据事实…</p>
            ) : impactSummary ? (
                <p className="text-sm">{impactSummary}</p>
            ) : null}

            {briefLines && briefLines.length > 0 ? (
                <ul className="space-y-1 text-sm">
                    {briefLines.map((line) => (
                        <li key={line.title} className="flex justify-between gap-2">
                            <span className="min-w-0 truncate">{line.title}</span>
                            {line.quantity ? (
                                <span className="num shrink-0 text-muted-foreground">
                                    {line.quantity}
                                </span>
                            ) : null}
                        </li>
                    ))}
                    {briefMoreCount ? (
                        <li className="text-xs text-muted-foreground">
                            另有 {briefMoreCount} 行
                        </li>
                    ) : null}
                </ul>
            ) : null}

            {item.actionBlockers.length > 0 ? (
                <p className="text-sm text-muted-foreground">
                    {item.actionBlockers[0]?.message}
                </p>
            ) : null}

            {approvalTask ? (
                <>
                    <RuntimeSummary instance={instance} />
                    <ApprovalActionBar
                        allowedActions={item.allowedActions}
                        recoveryOptions={recoveryQuery.data?.actions ?? []}
                        workItemId={item.workItemId}
                        expectedTaskVersion={item.taskVersion}
                        instance={instance}
                        documentHref={documentHref}
                        canReadSensitive={canReadSensitive}
                        approveWithoutDialog
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
