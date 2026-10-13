"use client"

import { CircleCheckIcon, PackageIcon, TriangleAlertIcon } from "lucide-react"
import { QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import type { SalesOrderProcurementCoverageLine } from "@/features/sales-orders/types"
import { SalesOrderProcurementSourceNotice } from "@/features/sales-orders/components/sales-order-procurement-source-notice"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

/** 概览与采购页共用缺口摘要；逐行数量仅来自销售详情的权威覆盖结果。 */
export function SalesOrderProcurementProgress({
    order,
    surface,
    statusSummary,
}: {
    order: {
        related: Pick<
            SalesOrderDetailView["related"],
            "purchaseOrders" | "procurementProgress"
        >
    }
    surface: "overview" | "purchase"
    statusSummary?: string
}) {
    const progress = order.related.procurementProgress
    const hasGap = compareDecimal(progress.remainingQuantity, "0", 6) > 0
    const hasExcess = compareDecimal(progress.excessQuantity, "0", 6) > 0
    const needsAttention = hasGap || hasExcess || progress.sourceChanged
    const uncoveredLines = progress.lines?.filter(
        (line) => compareDecimal(line.remainingQuantity, "0", 6) > 0,
    )
    const excessLines = progress.lines?.filter(
        (line) => compareDecimal(line.excessQuantity, "0", 6) > 0,
    )
    const label = progress.sourceChanged
        ? "采购依据待更新"
        : hasGap
          ? "供给未完全覆盖"
          : progress.status === "covered"
            ? "供给已覆盖"
            : progress.label
    const StatusIcon = needsAttention
        ? TriangleAlertIcon
        : progress.status === "covered"
          ? CircleCheckIcon
          : PackageIcon
    const headingId = `sales-order-${surface}-procurement-heading`

    return (
        <section aria-labelledby={headingId} className="space-y-3">
            <div
                className={cn(
                    "overflow-hidden rounded-xl border",
                    needsAttention
                        ? "border-warning-border"
                        : "border-border/70",
                )}
            >
                <div
                    className={cn(
                        "space-y-4 px-4 py-4 md:px-5",
                        needsAttention ? "bg-warning-soft" : "bg-muted/35",
                    )}
                    data-testid={
                        surface === "overview"
                            ? "sales-order-procurement-progress"
                            : "sales-order-purchase-progress"
                    }
                >
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        <h2 id={headingId} className="text-base font-semibold">
                            采购进度
                        </h2>
                        <Badge
                            variant={
                                needsAttention
                                    ? "warning"
                                    : progress.status === "covered"
                                      ? "success"
                                      : "secondary"
                            }
                            data-testid="sales-order-purchase-status"
                        >
                            <StatusIcon
                                aria-hidden="true"
                                className="size-3.5"
                            />
                            {label}
                        </Badge>
                    </div>
                    <div className="flex flex-wrap items-end justify-between gap-x-8 gap-y-3">
                        <div>
                            <div
                                className={cn(
                                    "text-xs",
                                    needsAttention
                                        ? "text-warning-soft-foreground"
                                        : "text-muted-foreground",
                                )}
                            >
                                {hasGap ? "待覆盖数量" : "已覆盖数量"}
                            </div>
                            <div
                                className={cn(
                                    "mt-1 text-2xl font-semibold",
                                    needsAttention &&
                                        "text-warning-soft-foreground",
                                )}
                            >
                                <QuantityValue
                                    unit={null}
                                    tone={
                                        needsAttention ? "warning" : "neutral"
                                    }
                                    value={
                                        hasGap
                                            ? progress.remainingQuantity
                                            : progress.coveredQuantity
                                    }
                                />
                                {hasGap && uncoveredLines?.length ? (
                                    <span className="ml-2 text-xs font-normal">
                                        涉及 {uncoveredLines.length} 项明细
                                    </span>
                                ) : null}
                            </div>
                        </div>
                        <div className="space-y-1 text-xs text-muted-foreground sm:text-right">
                            <p>
                                供给目标{" "}
                                <QuantityValue
                                    value={progress.salesQuantity}
                                    unit={null}
                                />{" "}
                                · 已覆盖{" "}
                                <QuantityValue
                                    value={progress.coveredQuantity}
                                    unit={null}
                                />
                            </p>
                            <p>
                                已创建采购单{" "}
                                <span className="num font-medium text-foreground">
                                    {order.related.purchaseOrders}
                                </span>{" "}
                                笔
                            </p>
                            {statusSummary ? <p>{statusSummary}</p> : null}
                        </div>
                    </div>
                </div>
                {hasGap ? (
                    <div className="px-4 py-4 md:px-5">
                        <h3 className="text-sm font-medium">待覆盖明细</h3>
                        {uncoveredLines?.length ? (
                            <ul className="mt-2 divide-y divide-border/60">
                                {uncoveredLines.map((line) => (
                                    <li
                                        key={line.id}
                                        className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 py-3 last:pb-0"
                                    >
                                        <div className="min-w-0 flex-1 basis-40">
                                            <div className="break-words text-sm font-medium">
                                                {line.name}
                                            </div>
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                第 {line.lineNo} 行
                                                {line.specification
                                                    ? ` · ${line.specification}`
                                                    : ""}
                                            </p>
                                        </div>
                                        <div className="shrink-0 space-y-1 text-right">
                                            <p className="text-sm font-semibold text-warning-soft-foreground">
                                                待覆盖{" "}
                                                <QuantityValue
                                                    value={
                                                        line.remainingQuantity
                                                    }
                                                    tone="warning"
                                                    unit={line.unit}
                                                />
                                            </p>
                                            <p className="text-xs text-muted-foreground">
                                                已覆盖{" "}
                                                <QuantityValue
                                                    value={line.coveredQuantity}
                                                    unit={null}
                                                />{" "}
                                                /{" "}
                                                <QuantityValue
                                                    value={line.quantity}
                                                    unit={line.unit}
                                                />
                                            </p>
                                        </div>
                                    </li>
                                ))}
                            </ul>
                        ) : (
                            <p className="mt-2 text-xs text-muted-foreground">
                                逐项覆盖明细暂不可用，请刷新后查看。
                            </p>
                        )}
                    </div>
                ) : null}
            </div>
            {hasExcess && excessLines?.length ? (
                <SalesOrderProcurementExcessLines
                    id={`sales-order-${surface}-procurement-excess`}
                    lines={excessLines}
                />
            ) : null}
            <SalesOrderProcurementSourceNotice
                id={`sales-order-${surface}-source-changed-notice`}
                progress={progress}
            />
        </section>
    )
}

/** 销售减量后覆盖超出目标的明细，等待采购调减或登记维持。 */
function SalesOrderProcurementExcessLines({
    id,
    lines,
}: {
    id: string
    lines: readonly SalesOrderProcurementCoverageLine[]
}) {
    return (
        <div
            id={id}
            className="rounded-xl border border-warning-border px-4 py-3 md:px-5"
        >
            <h3 className="text-sm font-medium">超出明细</h3>
            <ul className="mt-2 divide-y divide-border/60">
                {lines.map((line) => (
                    <li
                        key={line.id}
                        className="flex flex-wrap items-center justify-between gap-x-6 gap-y-1 py-2 last:pb-0"
                    >
                        <div className="min-w-0 flex-1 basis-40 text-sm">
                            {line.name}
                            <span className="ml-2 text-xs text-muted-foreground">
                                第 {line.lineNo} 行
                            </span>
                        </div>
                        <p className="shrink-0 text-sm text-warning-soft-foreground">
                            已采购超出{" "}
                            <QuantityValue
                                value={line.excessQuantity}
                                tone="warning"
                                unit={line.unit}
                            />
                            ，待采购调减
                        </p>
                    </li>
                ))}
            </ul>
        </div>
    )
}
