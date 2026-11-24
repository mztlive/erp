"use client"

import { useMutation } from "@tanstack/react-query"
import { downloadAcceptanceEvidence } from "@/features/sales-orders/lib/acceptance-mutations"

import {
    BanIcon,
    CircleAlertIcon,
    CircleCheckIcon,
    DownloadIcon,
    type LucideIcon,
    RotateCcwIcon,
    TriangleAlertIcon,
    UserRoundIcon,
} from "lucide-react"

import {
    BusinessStatusBadge,
    DetailWorkspaceSection,
    DocumentSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    statusToneMarkerClass,
    type StatusTone,
} from "@/components/ui/status-badge"
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

export function AcceptanceHistoryList({
    history,
    canReverse,
    onReverse,
    surface = "task",
    className,
    showGuidance = true,
}: {
    history: AcceptanceHistoryItem[]
    canReverse: boolean
    onReverse: (item: AcceptanceHistoryItem) => void
    surface?: "task" | "detail"
    className?: string
    showGuidance?: boolean
}) {
    const ordered = [...history].sort(compareHistoryNewestFirst)
    const byId = new Map(history.map((item) => [item.acceptanceId, item]))
    const guidance = showGuidance
        ? "已确认的验收记录不可修改；如有误录，可通过冲正新增一条反向记录。"
        : undefined

    const content =
        ordered.length === 0 ? (
            <p className="text-xs text-muted-foreground">还没有验收记录。</p>
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
        )

    if (surface === "detail") {
        return (
            <DetailWorkspaceSection
                width="form"
                title="验收记录"
                badge={
                    ordered.length > 0 ? (
                        <span className="num text-xs text-muted-foreground">
                            共 {ordered.length} 条
                        </span>
                    ) : undefined
                }
                subtitle={guidance}
                className={className}
            >
                {content}
            </DetailWorkspaceSection>
        )
    }

    return (
        <DocumentSection
            className={className ?? "py-0"}
            title="验收记录"
            description={guidance}
        >
            {content}
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
    const download = useMutation({
        mutationFn: () =>
            downloadAcceptanceEvidence(item.acceptanceId, item.acceptanceNo),
    })
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
    const hasActions = Boolean(item.evidenceAttachmentId) || canReverseThis

    return (
        <TimelineItem
            className={cn(item.status === "REVERSED" && "opacity-75")}
        >
            <TimelineMarker className={statusToneMarkerClass[tone]}>
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
            <TimelineDescription className="flex flex-col gap-2">
                <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
                    {recordedBy ? (
                        <span className="inline-flex items-center gap-1">
                            <UserRoundIcon
                                aria-hidden="true"
                                className="size-3.5 shrink-0"
                            />
                            <span>{recordedBy}</span>
                        </span>
                    ) : null}
                    {relatedOriginalNo ? (
                        <span>
                            {recordedBy ? "· " : ""}冲正 {relatedOriginalNo}
                        </span>
                    ) : null}
                    {showReversedBy ? (
                        <span>
                            {recordedBy || relatedOriginalNo ? "· " : ""}已被{" "}
                            {relatedReversalNo} 冲正
                        </span>
                    ) : null}
                    {!item.evidenceAttachmentId ? (
                        <span>
                            {recordedBy || relatedOriginalNo || showReversedBy
                                ? "· "
                                : ""}
                            未上传签收单凭证
                        </span>
                    ) : null}
                </div>

                {comment ? (
                    <p className="rounded-lg bg-muted/55 px-3 py-1.5 text-body-compact text-foreground">
                        {comment}
                    </p>
                ) : null}

                {item.lines.length > 0 ? (
                    <ul className="divide-y divide-border/50 rounded-lg bg-muted/30 px-3 py-1">
                        {item.lines.map((line) => (
                            <HistoryLineRow
                                key={`${line.salesOrderLineId}-${line.lineNo}`}
                                line={line}
                            />
                        ))}
                    </ul>
                ) : null}

                {showFactNotice ? (
                    <p className="text-xs text-warning-soft-foreground">
                        {FACT_ONLY_NOTICE}
                    </p>
                ) : null}

                {hasActions ? (
                    <div className="flex flex-wrap items-center gap-1.5 pt-0.5">
                        {item.evidenceAttachmentId ? (
                            <Button
                                id={`sales-orders-acceptance-history-${toAutomationIdSegment(item.acceptanceId)}-download`}
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={
                                    detailWorkspaceStyles.inlineLinkButton
                                }
                                disabled={download.isPending}
                                onClick={() => download.mutate()}
                            >
                                <DownloadIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                {download.isPending
                                    ? "下载中…"
                                    : "下载签收单凭证"}
                            </Button>
                        ) : null}
                        {canReverseThis ? (
                            <Button
                                id={`sales-orders-acceptance-history-${toAutomationIdSegment(item.acceptanceId)}-reverse`}
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={
                                    detailWorkspaceStyles.quietDangerButton
                                }
                                onClick={() => onReverse(item)}
                            >
                                <RotateCcwIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                冲正误录
                            </Button>
                        ) : null}
                    </div>
                ) : null}
            </TimelineDescription>
        </TimelineItem>
    )
}

function HistoryLineRow({
    line,
}: {
    line: AcceptanceHistoryItem["lines"][number]
}) {
    const hasPass = isPositiveQty(line.acceptedQuantity)
    const hasShort = isPositiveQty(line.shortQuantity)
    const hasReject = isPositiveQty(line.rejectedQuantity)
    return (
        <li className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 py-1.5 text-body-compact">
            <span className="min-w-0 text-foreground">
                <span className="num text-muted-foreground">
                    {line.lineNo} ·{" "}
                </span>
                {line.itemSnapshot}
            </span>
            <span className="num flex shrink-0 flex-wrap items-center gap-x-2 text-xs">
                {hasPass ? (
                    <span className="font-medium text-success-soft-foreground">
                        通过 {qtyWithUnit(line.acceptedQuantity, line.unitName)}
                    </span>
                ) : null}
                {hasShort ? (
                    <span className="font-medium text-warning-soft-foreground">
                        短少 {qtyWithUnit(line.shortQuantity, line.unitName)}
                    </span>
                ) : null}
                {hasReject ? (
                    <span className="font-medium text-destructive">
                        拒收 {qtyWithUnit(line.rejectedQuantity, line.unitName)}
                    </span>
                ) : null}
            </span>
        </li>
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
