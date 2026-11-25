"use client"

import { useState } from "react"
import { ChevronRightIcon } from "lucide-react"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Timeline,
    TimelineHeader,
    TimelineItem,
    TimelineMarker,
    TimelineTime,
    TimelineTitle,
} from "@/components/ui/timeline"
import { groupByRound } from "@/features/approval-workflow/components/execution-history"
import {
    displayActorName,
    displayExecutionStatus,
    displayInstanceStatus,
    displayReadableName,
    displayRound,
    displayUnixSeconds,
    executionStatusTone,
    instanceStatusTone,
} from "@/features/approval-workflow/display"
import type {
    ApprovalHistoryItem,
    ApprovalRuntimeInstance,
} from "@/features/approval-workflow/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function SalesOrderApprovalHistory({
    instance,
    items,
    hasMore,
    idPrefix = "sales-order-approval",
}: {
    instance: ApprovalRuntimeInstance
    items: readonly ApprovalHistoryItem[]
    hasMore: boolean
    idPrefix?: string
}) {
    const rounds = [...groupByRound(items)].reverse()
    return (
        <section aria-label="审批记录" className="space-y-3">
            <h3 className="font-medium">审批记录</h3>
            <div className="divide-y divide-border/70">
                {rounds.map((round, index) => (
                    <ApprovalRound
                        key={round.roundNo}
                        instance={instance}
                        idPrefix={idPrefix}
                        roundNo={round.roundNo}
                        items={round.items}
                        defaultExpanded={index === 0}
                        incomplete={hasMore && index === rounds.length - 1}
                    />
                ))}
            </div>
        </section>
    )
}

function ApprovalRound({
    instance,
    roundNo,
    items,
    defaultExpanded,
    incomplete,
    idPrefix,
}: {
    instance: ApprovalRuntimeInstance
    roundNo: number
    items: readonly ApprovalHistoryItem[]
    defaultExpanded: boolean
    incomplete: boolean
    idPrefix: string
}) {
    const [expanded, setExpanded] = useState(defaultExpanded)
    const current = roundNo === instance.currentRoundNo
    const rejection = items.some((item) => item.result === "REJECTED")
    const cancelled = items.some((item) => item.result === "CANCELLED")
    const label = current
        ? displayInstanceStatus(instance.status)
        : rejection
          ? "已驳回"
          : cancelled
            ? "已撤回"
            : "已处理"
    const tone = current
        ? instanceStatusTone(instance.status)
        : rejection
          ? "destructive"
          : "neutral"
    const lastDecision = items.findLast((item) => item.decidedAt)
    const time = displayUnixSeconds(lastDecision?.decidedAt)
    return (
        <details
            className="group/round py-1"
            open={expanded}
            onToggle={(event) => setExpanded(event.currentTarget.open)}
        >
            <summary
                id={`${idPrefix}-round-${roundNo}`}
                className="flex cursor-pointer list-none flex-wrap items-center gap-x-3 gap-y-2 rounded-md py-3 text-body-compact focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden"
            >
                <ChevronRightIcon
                    aria-hidden="true"
                    className="size-4 shrink-0 text-muted-foreground transition-transform group-open/round:rotate-90"
                />
                <span className="font-medium">{displayRound(roundNo)}</span>
                <StatusBadge tone={tone} label={label} appearance="text" />
                {time ? (
                    <time
                        dateTime={time.dateTime}
                        className="num text-xs text-muted-foreground sm:ml-auto"
                    >
                        {time.label}
                    </time>
                ) : null}
            </summary>
            <div className="pb-4 pl-3 sm:pl-7">
                {incomplete ? (
                    <p className="mb-4 text-xs text-muted-foreground">
                        此处仅展示已加载记录，更多记录可继续加载。
                    </p>
                ) : null}
                <Timeline aria-label={`${displayRound(roundNo)}审批记录`}>
                    {items.map((item) => (
                        <ApprovalEntry
                            key={item.executionId}
                            item={item}
                            idPrefix={idPrefix}
                        />
                    ))}
                </Timeline>
            </div>
        </details>
    )
}

function ApprovalEntry({
    item,
    idPrefix,
}: {
    item: ApprovalHistoryItem
    idPrefix: string
}) {
    const actor = displayActorName(item.assigneeName, item.decidedBy)
    const time = displayUnixSeconds(item.decidedAt)
    const reason = item.decisionReason?.trim()
    const longReason =
        reason && (reason.length > 160 || reason.split("\n").length > 3)
    return (
        <TimelineItem className="min-w-0 pb-5">
            <TimelineMarker aria-hidden="true">
                <span className="size-1.5 rounded-full bg-muted-foreground" />
            </TimelineMarker>
            <TimelineHeader className="gap-x-3">
                <TimelineTitle className="break-words">
                    {displayReadableName(item.nodeName, item.nodeKey) ||
                        "审批节点未标注"}
                </TimelineTitle>
                <StatusBadge
                    tone={executionStatusTone(item.result)}
                    label={displayExecutionStatus(item.result)}
                    appearance="text"
                />
                {actor ? (
                    <span className="text-body-compact text-muted-foreground">
                        {actor}
                    </span>
                ) : null}
                {time ? (
                    <TimelineTime dateTime={time.dateTime}>
                        {time.label}
                    </TimelineTime>
                ) : null}
            </TimelineHeader>
            {reason ? (
                longReason ? (
                    <details className="group/reason mt-2 text-body-compact text-muted-foreground">
                        <summary
                            id={`${idPrefix}-reason-${toAutomationIdSegment(item.executionId)}`}
                            className="cursor-pointer rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        >
                            查看完整处理意见
                            <span className="mt-1 line-clamp-2 whitespace-pre-wrap break-words group-open/reason:hidden">
                                {reason}
                            </span>
                        </summary>
                        <p className="mt-2 whitespace-pre-wrap break-words">
                            {reason}
                        </p>
                    </details>
                ) : (
                    <p className="mt-2 whitespace-pre-wrap break-words text-body-compact text-muted-foreground">
                        {reason}
                    </p>
                )
            ) : null}
        </TimelineItem>
    )
}
