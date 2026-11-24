"use client"

import { CircleAlertIcon } from "lucide-react"

import {
    displayActorName,
    displayReadableName,
    displayUnixSeconds,
} from "@/features/approval-workflow/display"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { cn } from "@/lib/utils"

export type ReworkRejectionContext = Readonly<{
    rejector?: string
    node?: string
    rejectedAtLabel?: string
    rejectedAtDateTime?: string
    reason?: string
}>

/** 从审批视图中提取兜底的上轮驳回上下文（供详情页或未显式透传时使用）。 */
export function rejectionContextFromApprovalView(
    approval?: DocumentApprovalView,
): ReworkRejectionContext | undefined {
    if (!approval) return undefined
    const rejectedHistory = approval.recentHistory.find(
        (entry) => entry.result === "REJECTED",
    )
    const rejector =
        displayActorName(
            rejectedHistory?.assigneeName,
            rejectedHistory?.decidedBy,
        ) ||
        displayActorName(
            approval.instance?.latestRejectionByName,
            approval.instance?.latestRejectionBy,
        )
    const node = displayReadableName(
        rejectedHistory?.nodeName,
        rejectedHistory?.nodeKey,
    )
    const rejectedAt = displayUnixSeconds(rejectedHistory?.decidedAt)
    const reason =
        rejectedHistory?.decisionReason?.trim() ||
        approval.instance?.latestRejection?.trim()

    if (!rejector && !reason && !node && !rejectedAt) return undefined
    return {
        rejector,
        node,
        rejectedAtLabel: rejectedAt?.label,
        rejectedAtDateTime: rejectedAt?.dateTime,
        reason,
    }
}

/** 弹窗与详情顶部轻量回显上轮驳回人及原话，避免用户在编辑时遗忘卡点。 */
export function ReworkRejectionEcho({
    rejection,
    className,
}: {
    rejection?: ReworkRejectionContext
    className?: string
}) {
    const reason = rejection?.reason?.trim()
    const rejector = rejection?.rejector?.trim()
    if (!rejection || (!reason && !rejector)) return null

    return (
        <section
            aria-label="上轮驳回意见"
            data-slot="rework-rejection-echo"
            className={cn(
                "rounded-r-md border-l-2 border-l-destructive bg-destructive-soft/40 px-3 py-2 text-xs",
                className,
            )}
        >
            <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                <span className="inline-flex items-center gap-1.5 font-semibold text-destructive-soft-foreground">
                    <CircleAlertIcon
                        aria-hidden="true"
                        className="size-3.5 shrink-0 text-destructive"
                    />
                    上轮驳回意见
                </span>
                {rejector ? (
                    <span className="font-medium text-foreground/85">
                        {rejector}
                    </span>
                ) : null}
                {rejection.node ? (
                    <span className="text-muted-foreground">
                        {rejection.node}
                    </span>
                ) : null}
                {rejection.rejectedAtLabel ? (
                    <time
                        dateTime={rejection.rejectedAtDateTime}
                        className="num text-muted-foreground"
                    >
                        {rejection.rejectedAtLabel}
                    </time>
                ) : null}
            </div>
            {reason ? (
                <p className="mt-1 text-body-compact font-medium whitespace-pre-wrap wrap-anywhere text-foreground">
                    「{reason}」
                </p>
            ) : null}
        </section>
    )
}
