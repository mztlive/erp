"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    displayActorName,
    displayInstanceStatus,
    displayProcessVersion,
    displayReadableName,
    displayRound,
    instanceStatusTone,
    isOpenInstanceStatus,
} from "@/features/approval-workflow/display"
import type { ApprovalRuntimeInstance } from "@/features/approval-workflow/types"

/** 当前状态与流程资料分开呈现；历史驳回意见只在所属轮次展示。 */
export function SalesOrderApprovalSummary({
    instance,
    idPrefix = "sales-order-approval",
}: {
    instance: ApprovalRuntimeInstance
    idPrefix?: string
}) {
    const inProgress = isOpenInstanceStatus(instance.status)
    const node = displayReadableName(
        instance.currentNodeName,
        instance.currentNode,
    )
    const assignee = displayActorName(
        instance.currentAssigneeName,
        instance.currentAssignee,
    )
    return (
        <section
            aria-label="当前审批"
            className="space-y-3 border-b border-border/70 pb-5"
        >
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-2">
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                        <h3 className="text-body-compact font-semibold">
                            {inProgress ? node || "当前审批" : "审批结果"}
                        </h3>
                        <StatusBadge
                            tone={instanceStatusTone(instance.status)}
                            label={displayInstanceStatus(instance.status)}
                        />
                        {inProgress ? (
                            <span className="text-body-compact">
                                {assignee || "审批人待确认"}
                            </span>
                        ) : null}
                    </div>
                    <p className="text-xs text-muted-foreground">
                        {instance.status === "BLOCKED"
                            ? "当前审批无法继续，请查看下方受阻原因。"
                            : inProgress
                              ? "等待当前审批人处理，办理结果将在记录中更新。"
                              : "本次审批已结束，处理意见见下方记录。"}
                    </p>
                </div>
                <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-muted-foreground">
                    <span>{displayRound(instance.currentRoundNo)}</span>
                    <details className="group max-w-full">
                        <summary
                            id={`${idPrefix}-process-info`}
                            className="cursor-pointer rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        >
                            流程信息
                        </summary>
                        <p className="mt-2 break-words">
                            {displayProcessVersion({
                                name: instance.processName,
                                version: instance.processVersion,
                            })}
                        </p>
                    </details>
                </div>
            </div>
            {instance.status === "REJECTED" && instance.latestRejection ? (
                <Alert variant="destructive">
                    <AlertTitle>本次审批已驳回</AlertTitle>
                    <AlertDescription className="whitespace-pre-wrap break-words">
                        {instance.latestRejection}
                    </AlertDescription>
                </Alert>
            ) : null}
            {instance.status === "BLOCKED" ? (
                <Alert variant="destructive">
                    <AlertTitle>审批受阻</AlertTitle>
                    <AlertDescription>
                        {instance.blockerMessage ||
                            "当前审批无法继续，请按系统给出的恢复方式处理。"}
                    </AlertDescription>
                </Alert>
            ) : null}
        </section>
    )
}
