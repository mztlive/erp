"use client"

import {
    BanIcon,
    CircleAlertIcon,
    CircleCheckIcon,
    type LucideIcon,
    RotateCcwIcon,
    TriangleAlertIcon,
    UserRoundIcon,
} from "lucide-react"

import { BusinessStatusBadge, DocumentSection } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { StatusTone } from "@/components/ui/status-badge"
import {
    Timeline,
    TimelineDescription,
    TimelineHeader,
    TimelineItem,
    TimelineMarker,
    TimelineTime,
    TimelineTitle,
} from "@/components/ui/timeline"
import {
    formatOccurredAt,
    isPositiveQty,
    qtyWithUnit,
    visibleAcceptanceNo,
} from "@/features/sales-orders/lib/acceptance-model"
import {
    FACT_ONLY_NOTICE,
    OVERALL_RESULT_LABEL,
    type AcceptanceHistoryItem,
} from "@/features/sales-orders/lib/acceptance-types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

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

export function AcceptanceHistoryList({
    history,
    canReverse,
    onReverse,
    className,
}: {
    history: AcceptanceHistoryItem[]
    canReverse: boolean
    onReverse: (item: AcceptanceHistoryItem) => void
    className?: string
}) {
    const ordered = [...history].sort(compareHistoryNewestFirst)
    const byId = new Map(history.map((item) => [item.acceptanceId, item]))

    return (
        <DocumentSection
            className={className ?? "py-0"}
            title="验收记录"
            description="已经确认的不能改；记错了用冲正新增一条反向记录。"
        >
            {ordered.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                    还没有验收记录。
                </p>
            ) : (
                <Timeline aria-label="验收记录时间线">
                    {ordered.map((item) => (
                        <HistoryEntry
                            key={item.acceptanceId}
                            item={item}
                            relatedOriginal={
                                item.reversalOfAcceptanceId
                                    ? byId.get(item.reversalOfAcceptanceId)
                                    : undefined
                            }
                            relatedReversal={
                                item.reversedByAcceptanceId
                                    ? byId.get(item.reversedByAcceptanceId)
                                    : undefined
                            }
                            canReverse={canReverse}
                            onReverse={onReverse}
                        />
                    ))}
                </Timeline>
            )}
        </DocumentSection>
    )
}

function HistoryEntry({
    item,
    relatedOriginal,
    relatedReversal,
    canReverse,
    onReverse,
}: {
    item: AcceptanceHistoryItem
    relatedOriginal?: AcceptanceHistoryItem
    relatedReversal?: AcceptanceHistoryItem
    canReverse: boolean
    onReverse: (item: AcceptanceHistoryItem) => void
}) {
    const tone = historyTone(item)
    const Icon = historyMarkerIcon(item, tone)
    const occurredAt = item.acceptedAt || item.postedAt
    const recordedBy = item.recordedBy.trim()
    const comment = item.comment?.trim() ?? ""
    const acceptanceNo = visibleAcceptanceNo(item.acceptanceNo)
    const relatedOriginalNo = visibleAcceptanceNo(relatedOriginal?.acceptanceNo)
    const relatedReversalNo = visibleAcceptanceNo(relatedReversal?.acceptanceNo)
    const showFactNotice =
        (item.overallResult === "SHORT" ||
            item.overallResult === "REJECT" ||
            item.overallResult === "SERVICE_FAIL") &&
        !item.reversalOfAcceptanceId
    const canReverseThis =
        item.status === "POSTED" && !item.reversalOfAcceptanceId && canReverse
    const showReversedBy =
        item.status === "REVERSED" && Boolean(relatedReversalNo)
    const hasDescription =
        Boolean(recordedBy) ||
        Boolean(relatedOriginalNo) ||
        showReversedBy ||
        Boolean(comment) ||
        item.lines.length > 0 ||
        showFactNotice ||
        canReverseThis

    return (
        <TimelineItem
            className={cn(item.status === "REVERSED" && "opacity-80")}
        >
            <TimelineMarker className={TONE_MARKER_CLASS[tone]}>
                <Icon aria-hidden="true" />
            </TimelineMarker>
            <TimelineHeader className="items-center justify-between">
                <TimelineTitle className="flex min-w-0 flex-wrap items-center gap-2">
                    <span>{OVERALL_RESULT_LABEL[item.overallResult]}</span>
                    {acceptanceNo ? (
                        <span className="num font-mono font-normal text-muted-foreground">
                            {acceptanceNo}
                        </span>
                    ) : null}
                    <BusinessStatusBadge
                        context="preview"
                        label={historyStatusLabel(item)}
                        tone={tone}
                    />
                </TimelineTitle>
                {occurredAt ? (
                    <TimelineTime dateTime={occurredAt}>
                        {formatOccurredAt(occurredAt)}
                    </TimelineTime>
                ) : null}
            </TimelineHeader>
            {hasDescription ? (
                <TimelineDescription className="flex flex-col gap-2">
                    {recordedBy ? (
                        <p className="inline-flex items-center gap-1.5">
                            <UserRoundIcon
                                aria-hidden="true"
                                className="size-3.5 shrink-0"
                            />
                            <span>{recordedBy}</span>
                        </p>
                    ) : null}
                    {relatedOriginalNo ? <p>冲正 {relatedOriginalNo}</p> : null}
                    {showReversedBy ? (
                        <p>已被 {relatedReversalNo} 冲正</p>
                    ) : null}
                    {comment ? (
                        <p className="rounded-lg bg-muted px-2.5 py-1.5 text-sm text-foreground">
                            {comment}
                        </p>
                    ) : null}
                    {item.lines.length > 0 ? (
                        <ul className="flex flex-col gap-1 text-foreground">
                            {item.lines.map((line) => (
                                <li
                                    key={`${line.salesOrderLineId}-${line.lineNo}`}
                                >
                                    {formatHistoryLine(line)}
                                </li>
                            ))}
                        </ul>
                    ) : null}
                    {showFactNotice ? (
                        <p className="text-xs text-warning-soft-foreground">
                            {FACT_ONLY_NOTICE}
                        </p>
                    ) : null}
                    {canReverseThis ? (
                        <div>
                            <Button
                                id={`sales-orders-acceptance-history-${toAutomationIdSegment(item.acceptanceId)}-reverse`}
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={() => onReverse(item)}
                            >
                                <RotateCcwIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                冲正误录
                            </Button>
                        </div>
                    ) : null}
                </TimelineDescription>
            ) : null}
        </TimelineItem>
    )
}

function compareHistoryNewestFirst(
    left: AcceptanceHistoryItem,
    right: AcceptanceHistoryItem,
) {
    const rightTime = Date.parse(right.acceptedAt || right.postedAt)
    const leftTime = Date.parse(left.acceptedAt || left.postedAt)
    if (Number.isFinite(rightTime) && Number.isFinite(leftTime)) {
        return rightTime - leftTime
    }
    return right.acceptanceNo.localeCompare(left.acceptanceNo, "zh-CN")
}

function historyStatusLabel(item: AcceptanceHistoryItem) {
    if (item.reversalOfAcceptanceId) return "冲正记录"
    if (item.status === "REVERSED") return "已冲正"
    return "已确认"
}

function historyTone(item: AcceptanceHistoryItem): StatusTone {
    if (item.status === "REVERSED") return "void"
    if (item.reversalOfAcceptanceId) return "warning"
    if (item.overallResult === "PASS") return "success"
    if (item.overallResult === "SHORT") return "warning"
    return "destructive"
}

function historyMarkerIcon(
    item: AcceptanceHistoryItem,
    tone: StatusTone,
): LucideIcon {
    if (item.reversalOfAcceptanceId) return RotateCcwIcon
    if (tone === "success") return CircleCheckIcon
    if (tone === "warning") return CircleAlertIcon
    if (tone === "destructive") return TriangleAlertIcon
    return BanIcon
}

function formatHistoryLine(
    line: AcceptanceHistoryItem["lines"][number],
): string {
    const parts: string[] = []
    if (isPositiveQty(line.acceptedQuantity)) {
        parts.push(`通过 ${qtyWithUnit(line.acceptedQuantity, line.unitCode)}`)
    }
    if (isPositiveQty(line.shortQuantity)) {
        parts.push(`短少 ${qtyWithUnit(line.shortQuantity, line.unitCode)}`)
    }
    if (isPositiveQty(line.rejectedQuantity)) {
        parts.push(`拒收 ${qtyWithUnit(line.rejectedQuantity, line.unitCode)}`)
    }
    const qty = parts.length > 0 ? ` · ${parts.join("、")}` : ""
    return `${line.lineNo} · ${line.itemSnapshot}${qty}`
}
