"use client"

import type { ReactNode } from "react"

import {
    DetailInlineTable,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
} from "@/components/business"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    clampZeroFixed,
    compactFixed,
    compareDecimal,
    divideFixed,
    minFixed,
    multiplyFixed,
} from "@/lib/fixed-decimal"
import {
    isPositiveQty,
    qtyWithUnit,
    type AcceptanceLineProgress,
    type AcceptanceOrderProgress,
} from "@/features/sales-orders/lib/acceptance-model"
import { FULFILLMENT_TYPE_LABEL } from "@/features/sales-orders/lib/acceptance-types"
import { cn } from "@/lib/utils"

/** 详情工作面共用的交付验收 KPI 摘要，读写态保持完全一致。 */
export function AcceptanceSummaryMetrics({
    progress,
}: {
    progress: AcceptanceOrderProgress
}) {
    const unit = progress.unitName
    const singleUnit = Boolean(progress.unitCode)
    const totalLines = progress.lines.length
    const deliveredLines = progress.lines.filter(
        (line) =>
            compareDecimal(line.deliveredQuantity, line.requiredQuantity, 6) >=
                0 && isPositiveQty(line.requiredQuantity),
    ).length
    const doneLines = progress.lines.filter(
        (line) => line.stuckKind === "done",
    ).length
    const pendingLines = progress.lines.filter((line) =>
        isPositiveQty(line.pendingQuantity),
    ).length
    const hasPending = progress.pendingFactCount > 0
    const acceptedPct = singleUnit
        ? acceptancePercent(
              progress.acceptedQuantity,
              progress.requiredQuantity,
          )
        : linePercent(doneLines, totalLines)

    return (
        <DetailWorkspaceSection width="full">
            <DetailWorkspaceMetrics
                columns={3}
                variant="inline"
                ariaLabel="交付验收摘要"
            >
                <DetailWorkspaceMetricItem
                    emphasis={hasPending ? "primary" : "default"}
                    label="待验收数量"
                    value={
                        <span className={cn(hasPending && "text-primary")}>
                            {singleUnit
                                ? qtyWithUnit(progress.pendingQuantity, unit)
                                : `${progress.pendingFactCount} 批`}
                        </span>
                    }
                    detail={
                        hasPending ? (
                            <span className="num">
                                待确认 {progress.pendingFactCount} 批 ·{" "}
                                {pendingLines} 项明细
                            </span>
                        ) : (
                            "当前无待验批次"
                        )
                    }
                />
                <DetailWorkspaceMetricItem
                    label="已交付数量"
                    value={
                        singleUnit
                            ? qtyWithUnit(progress.deliveredQuantity, unit)
                            : `${deliveredLines} / ${totalLines} 项`
                    }
                    detail={
                        singleUnit ? (
                            <>
                                销售数量{" "}
                                <span className="num font-medium text-foreground">
                                    {qtyWithUnit(
                                        progress.requiredQuantity,
                                        unit,
                                    )}
                                </span>
                            </>
                        ) : (
                            "含多单位商品，按明细项汇总"
                        )
                    }
                />
                <DetailWorkspaceMetricItem
                    label="已通过验收"
                    value={
                        singleUnit
                            ? qtyWithUnit(progress.acceptedQuantity, unit)
                            : `${doneLines} / ${totalLines} 项`
                    }
                    detail={<span className="num">完成率 {acceptedPct}%</span>}
                />
            </DetailWorkspaceMetrics>
        </DetailWorkspaceSection>
    )
}

export function AcceptanceProgressTable({
    progress,
    pendingHint,
    actions,
    surface = "task",
    className,
    showSummary = true,
    compact = false,
}: {
    progress: AcceptanceOrderProgress
    pendingHint?: string
    actions?: ReactNode
    surface?: "task" | "detail"
    className?: string
    showSummary?: boolean
    compact?: boolean
}) {
    const unit = progress.unitName
    const summary = progress.unitCode
        ? `已通过 ${qtyWithUnit(progress.acceptedQuantity, unit)} · 已交付 ${qtyWithUnit(progress.deliveredQuantity, unit)} · 销售 ${qtyWithUnit(progress.requiredQuantity, unit)}`
        : "按明细分别统计；不同单位不合并。"

    const tableContent =
        progress.lines.length === 0 ? (
            <p className="text-xs text-muted-foreground">
                本单还没有销售明细。
            </p>
        ) : (
            <Table
                className={
                    compact
                        ? "table-fixed [&_td]:px-2 [&_td]:whitespace-normal [&_td]:wrap-anywhere [&_th]:bg-surface-sunken [&_th]:px-2 [&_th]:whitespace-normal [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md"
                        : "min-w-[36rem]"
                }
            >
                <TableHeader
                    className={
                        compact ? "bg-transparent [&_tr]:border-0" : undefined
                    }
                >
                    <TableRow>
                        <TableHead>明细</TableHead>
                        <TableHead data-align="end">销售</TableHead>
                        <TableHead data-align="end">已交付</TableHead>
                        <TableHead data-align="end">已通过</TableHead>
                        <TableHead data-align="end">待验收</TableHead>
                        <TableHead
                            className={compact ? "w-[25%]" : "w-56 max-w-56"}
                        >
                            当前卡在
                        </TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {progress.lines.map((line) => (
                        <ProgressRow
                            key={line.salesOrderLineId}
                            line={line}
                            compact={compact}
                        />
                    ))}
                </TableBody>
            </Table>
        )

    if (compact) {
        return (
            <div className={cn("min-w-0 pt-2", className)}>{tableContent}</div>
        )
    }

    if (surface === "detail") {
        return (
            <DetailWorkspaceSection
                width="full"
                title="验收进度"
                badge={
                    <span className="num text-xs text-muted-foreground">
                        共 {progress.lines.length} 项
                    </span>
                }
                subtitle={pendingHint ?? (showSummary ? summary : undefined)}
                actions={actions}
                className={className}
            >
                {progress.lines.length === 0 ? (
                    tableContent
                ) : (
                    <DetailInlineTable frame="scroll">
                        {tableContent}
                    </DetailInlineTable>
                )}
            </DetailWorkspaceSection>
        )
    }

    return (
        <DetailWorkspaceSection
            width="full"
            divider={false}
            className={className ?? "py-0"}
            title="验收进度"
            actions={actions}
            subtitle={
                !showSummary ? (
                    pendingHint
                ) : pendingHint ? (
                    <span className="inline-flex flex-wrap items-center gap-x-2">
                        <span>{summary}</span>
                        <span>{pendingHint}</span>
                    </span>
                ) : (
                    summary
                )
            }
        >
            {tableContent}
        </DetailWorkspaceSection>
    )
}

function ProgressRow({
    line,
    compact = false,
}: {
    line: AcceptanceLineProgress
    compact?: boolean
}) {
    const unit = line.unitName
    const hasPending = isPositiveQty(line.pendingQuantity)
    return (
        <TableRow>
            <TableCell className="min-w-0 whitespace-normal break-keep">
                <div className="font-medium">
                    {line.lineNo} · {line.itemSnapshot}
                </div>
            </TableCell>
            <TableCell data-align="end" className="num whitespace-nowrap">
                {qtyWithUnit(line.requiredQuantity, unit)}
            </TableCell>
            <TableCell data-align="end" className="num whitespace-nowrap">
                {qtyWithUnit(line.deliveredQuantity, unit)}
            </TableCell>
            <TableCell data-align="end" className="num whitespace-nowrap">
                {qtyWithUnit(line.acceptedQuantity, unit)}
            </TableCell>
            <TableCell
                data-align="end"
                className={cn(
                    "num whitespace-nowrap",
                    hasPending && "font-medium text-primary",
                )}
            >
                {qtyWithUnit(line.pendingQuantity, unit)}
            </TableCell>
            <TableCell
                className={compact ? "min-w-0" : "w-56 max-w-56 min-w-0"}
            >
                <StuckCell line={line} />
            </TableCell>
        </TableRow>
    )
}

function StuckCell({ line }: { line: AcceptanceLineProgress }) {
    if (line.stuckKind === "done") {
        return <StatusBadge tone="success" appearance="quiet" label="已完成" />
    }
    if (line.stuckKind === "accept" && line.pendingFacts.length === 1) {
        const fact = line.pendingFacts[0]
        if (!fact) {
            return <StatusBadge tone="info" label={line.stuckLabel} />
        }
        return (
            <div
                className="flex min-w-0 flex-col items-start gap-0.5"
                title={line.stuckLabel}
            >
                <StatusBadge
                    tone="info"
                    label={`${FULFILLMENT_TYPE_LABEL[fact.fulfillmentFactType]} · 待验 ${qtyWithUnit(fact.eligibleQuantity, fact.unitName)}`}
                />
                <span
                    className="num max-w-full truncate text-xs text-muted-foreground"
                    title={fact.fulfillmentNo}
                >
                    {fact.fulfillmentNo}
                </span>
            </div>
        )
    }
    if (line.stuckKind === "accept") {
        return <StatusBadge tone="info" label={line.stuckLabel} />
    }
    if (line.stuckKind === "exception") {
        return <StatusBadge tone="warning" label={line.stuckLabel} />
    }
    return (
        <StatusBadge
            tone="neutral"
            appearance="quiet"
            label={line.stuckLabel}
            title={line.stuckLabel}
        />
    )
}

function acceptancePercent(part: string, total: string): string {
    try {
        if (compareDecimal(total, "0", 6) <= 0) return "0"
        const clamped = clampZeroFixed(part, { maxScale: 6, outputScale: 6 })
        const ratio = divideFixed(clamped, total, {
            numeratorMaxScale: 6,
            denominatorMaxScale: 6,
            outputScale: 4,
        })
        const pct = multiplyFixed(ratio, "100", {
            leftMaxScale: 4,
            rightMaxScale: 0,
            outputScale: 0,
        })
        return compactFixed(
            minFixed(pct, "100", { maxScale: 0, outputScale: 0 }),
        )
    } catch {
        return "0"
    }
}

function linePercent(done: number, total: number): string {
    if (total <= 0) return "0"
    return String(Math.min(100, Math.round((done / total) * 100)))
}
