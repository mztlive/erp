"use client"

import type * as React from "react"
import {
    BanIcon,
    CircleAlertIcon,
    CircleCheckIcon,
    CircleDashedIcon,
    CircleDotIcon,
    type LucideIcon,
    TriangleAlertIcon,
    UserRoundIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    Timeline,
    TimelineDescription,
    TimelineHeader,
    TimelineItem,
    TimelineMarker,
    TimelineTime,
    TimelineTitle,
} from "@/components/ui/timeline"
import { cn } from "@/lib/utils"

import {
    displayActorName,
    displayExecutionStatus,
    displayRound,
    displayUnixSeconds,
    executionStatusTone,
} from "../display"
import type { ApprovalHistoryItem } from "../types"

const TONE_ICON: Record<StatusTone, LucideIcon> = {
    neutral: CircleDashedIcon,
    info: CircleDotIcon,
    success: CircleCheckIcon,
    warning: CircleAlertIcon,
    destructive: TriangleAlertIcon,
    void: BanIcon,
}

const TONE_MARKER_CLASS: Record<StatusTone, string> = {
    neutral:
        "border-neutral-border bg-neutral-soft text-neutral-soft-foreground",
    info: "border-info-border bg-info-soft text-info-soft-foreground",
    success:
        "border-success-border bg-success-soft text-success-soft-foreground",
    warning:
        "border-warning-border bg-warning-soft text-warning-soft-foreground",
    destructive:
        "border-destructive-border bg-destructive-soft text-destructive-soft-foreground",
    void: "border-neutral-border bg-neutral-soft text-neutral-soft-foreground",
}

function HistoryHeading({
    compact,
    children,
}: {
    compact?: boolean
    children: React.ReactNode
}) {
    if (compact) {
        return <h2 className="text-sm font-medium">{children}</h2>
    }
    return <CardTitle>{children}</CardTitle>
}

function HistoryEntry({ item }: { item: ApprovalHistoryItem }) {
    const tone = executionStatusTone(item.result)
    const Icon = TONE_ICON[tone]
    const actor =
        displayActorName(item.decidedBy) ?? displayActorName(item.assigneeName)
    const decidedAt = displayUnixSeconds(item.decidedAt)
    const rejected = item.result === "REJECTED"
    const current = item.result === "ACTIVE"

    return (
        <TimelineItem>
            <TimelineMarker className={TONE_MARKER_CLASS[tone]}>
                <Icon aria-hidden="true" />
            </TimelineMarker>
            <TimelineHeader>
                <TimelineTitle className="flex min-w-0 flex-wrap items-center gap-2">
                    <span>{item.nodeName}</span>
                    <StatusBadge
                        tone={tone}
                        label={displayExecutionStatus(item.result)}
                    />
                </TimelineTitle>
                {decidedAt ? (
                    <TimelineTime dateTime={decidedAt.dateTime}>
                        {decidedAt.label}
                    </TimelineTime>
                ) : null}
            </TimelineHeader>
            <TimelineDescription>
                {actor ? (
                    <p className="inline-flex items-center gap-1.5">
                        <UserRoundIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0"
                        />
                        <span>{actor}</span>
                    </p>
                ) : current ? (
                    <p>等待办理</p>
                ) : null}
                {item.decisionReason ? (
                    <p
                        className={cn(
                            "mt-2 rounded-lg px-2.5 py-1.5 text-sm",
                            rejected
                                ? "border border-destructive-border bg-destructive-soft text-destructive-soft-foreground"
                                : "bg-muted text-foreground",
                        )}
                    >
                        {item.decisionReason}
                    </p>
                ) : null}
            </TimelineDescription>
        </TimelineItem>
    )
}

/**
 * 按轮次分组、按执行序号排序的审批历史。
 *
 * 同一节点跨轮次必须显示多条记录，不得按 node_key 去重。
 * 首屏只消费受控 recent_history，更多记录由游标接口加载。
 */
export function ExecutionHistory({
    items,
    hasMore = false,
    loadingMore = false,
    onLoadMore,
    compact = false,
    id = "governance-approval-execution-history",
}: {
    items: readonly ApprovalHistoryItem[]
    hasMore?: boolean
    loadingMore?: boolean
    onLoadMore?: () => void
    /** 对象中心 tab 内使用：标题与概览 text-sm 对齐。 */
    compact?: boolean
    id?: string
}) {
    const rounds = groupByRound(items)

    return (
        <Card size="sm" className="border border-border shadow-sm">
            <CardHeader className="border-b">
                <HistoryHeading compact={compact}>审批历史</HistoryHeading>
            </CardHeader>
            <CardContent className="flex flex-col gap-5">
                {rounds.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        暂无审批记录
                    </p>
                ) : (
                    rounds.map((round) => (
                        <section
                            key={round.roundNo}
                            className="flex flex-col gap-3"
                        >
                            <h3 className="text-xs font-medium tracking-wide text-muted-foreground">
                                {displayRound(round.roundNo)}
                            </h3>
                            <Timeline>
                                {round.items.map((item) => (
                                    <HistoryEntry
                                        key={item.executionId}
                                        item={item}
                                    />
                                ))}
                            </Timeline>
                        </section>
                    ))
                )}
                {hasMore && onLoadMore ? (
                    <Button
                        id={`${id}-load-more`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={loadingMore}
                        onClick={onLoadMore}
                    >
                        {loadingMore ? "加载中" : "加载更多"}
                    </Button>
                ) : null}
            </CardContent>
        </Card>
    )
}

/**
 * 按轮次分组并按 execution_no 排序。保留跨轮次同一节点的多条记录。
 */
export const groupByRound = (
    items: readonly ApprovalHistoryItem[],
): readonly { roundNo: number; items: ApprovalHistoryItem[] }[] => {
    const grouped = new Map<number, ApprovalHistoryItem[]>()
    for (const item of items) {
        const bucket = grouped.get(item.roundNo) ?? []
        bucket.push(item)
        grouped.set(item.roundNo, bucket)
    }
    return [...grouped.entries()]
        .sort(([left], [right]) => left - right)
        .map(([roundNo, roundItems]) => ({
            roundNo,
            items: [...roundItems].sort(
                (left, right) => left.executionNo - right.executionNo,
            ),
        }))
}
