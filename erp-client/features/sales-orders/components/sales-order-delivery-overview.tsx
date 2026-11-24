"use client"

import type { ReactNode } from "react"
import { ArrowUpRightIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    MoneyValue,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
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
    subtractFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { SalesOrderDetailView } from "../api/sales-orders"
import type { useSalesOrderDeliveryProgressQuery } from "../hooks/queries"
import {
    isPositiveQty,
    qtyWithUnit,
    type AcceptanceLineProgress,
    type AcceptanceOrderProgress,
} from "../lib/acceptance-model"
import { getProcurementAttention } from "../lib/procurement-attention"
import { SalesOrderDeliverySummary } from "./sales-order-delivery-summary"
import { SalesOrderLineBatchesSheet } from "./sales-order-line-batches-sheet"

export type DeliveryProgressQuery = ReturnType<
    typeof useSalesOrderDeliveryProgressQuery
>

/** 商业明细与交付进度按稳定销售行匹配；缺失进度保持待确认。 */
export function SalesOrderDeliveryOverview({
    order,
    query,
    canRead,
    canAccept,
    onStartAcceptance,
    notice,
    actions,
    footer,
}: {
    order: SalesOrderDetailView
    query: DeliveryProgressQuery
    canRead: boolean
    canAccept?: boolean
    onStartAcceptance?: () => void
    notice?: ReactNode
    actions?: ReactNode
    footer?: ReactNode
}) {
    const effective = order.currentRevisionNo != null
    const orderProgress =
        canRead && effective && !query.isError ? query.data : undefined
    const lines = new Map(
        orderProgress?.lines.map((line) => [line.salesOrderLineId, line]) ?? [],
    )
    const procurementLines = new Map(
        (order.related.procurementProgress.lines ?? []).map((line) => [
            line.id,
            line,
        ]),
    )
    const pendingFactCount = orderProgress?.pendingFactCount ?? 0

    return (
        <>
            {orderProgress ? (
                <DeliveryOverviewMetrics
                    order={order}
                    progress={orderProgress}
                />
            ) : null}

            <DetailWorkspaceSection
                width="full"
                title="销售明细与交付"
                headingId="sales-order-delivery-overview-heading"
                badge={
                    <span className="num text-xs text-muted-foreground">
                        共 {order.lineItems.length} 项
                    </span>
                }
                subtitle="已交付按实际发货或已完成服务统计；货到后请及时办理客户验收。"
                actions={
                    canAccept && onStartAcceptance ? (
                        <>
                            {actions}
                            <Button
                                id="sales-order-delivery-go-acceptance"
                                type="button"
                                size="sm"
                                className="px-3.5"
                                onClick={onStartAcceptance}
                            >
                                去登记验收
                                {pendingFactCount > 0 ? (
                                    <span className="num">
                                        （{pendingFactCount} 批）
                                    </span>
                                ) : null}
                                <ArrowUpRightIcon
                                    aria-hidden="true"
                                    data-icon="inline-end"
                                />
                            </Button>
                        </>
                    ) : (
                        actions
                    )
                }
            >
                {notice}
                <DeliveryProgressNotice
                    order={order}
                    query={query}
                    canRead={canRead}
                />
                <div
                    data-slot="detail-inline-table"
                    className={detailWorkspaceStyles.dataTableWrapper}
                >
                    <Table data-density="comfortable" className="min-w-[46rem]">
                        <TableHeader>
                            <TableRow>
                                <TableHead>商品与交付要求</TableHead>
                                <TableHead data-align="end">销售数量</TableHead>
                                <TableHead data-align="end">含税金额</TableHead>
                                <TableHead>交付与验收进度</TableHead>
                                <TableHead>状态</TableHead>
                                <TableHead data-align="end">交付批次</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {order.lineItems.map((line) => {
                                const progress = canRead
                                    ? lines.get(line.id)
                                    : undefined
                                const procurementLine = effective
                                    ? procurementLines.get(line.id)
                                    : undefined
                                const hasProcurementExcess = Boolean(
                                    procurementLine &&
                                    compareDecimal(
                                        procurementLine.excessQuantity,
                                        "0",
                                        6,
                                    ) > 0,
                                )
                                const hasProcurementGap = Boolean(
                                    procurementLine &&
                                    compareDecimal(
                                        procurementLine.remainingQuantity,
                                        "0",
                                        6,
                                    ) > 0,
                                )
                                const showSku = Boolean(
                                    line.sku && line.sku !== "无规格",
                                )

                                return (
                                    <TableRow key={line.id}>
                                        <TableCell className="min-w-48 whitespace-normal">
                                            <div className="flex flex-wrap items-center gap-1.5 font-medium text-foreground">
                                                <span>{line.name}</span>
                                            </div>
                                            {showSku || line.dueDate ? (
                                                <p className="num mt-0.5 flex flex-wrap items-center gap-x-1.5 text-xs text-muted-foreground">
                                                    {showSku ? (
                                                        <span>{line.sku}</span>
                                                    ) : null}
                                                    {showSku && line.dueDate ? (
                                                        <span
                                                            aria-hidden="true"
                                                            className="text-muted-foreground/45"
                                                        >
                                                            ·
                                                        </span>
                                                    ) : null}
                                                    {line.dueDate ? (
                                                        <span>
                                                            期望交付{" "}
                                                            {line.dueDate}
                                                        </span>
                                                    ) : null}
                                                </p>
                                            ) : null}
                                            <SalesOrderDeliverySummary
                                                line={line}
                                            />
                                            {hasProcurementExcess &&
                                            procurementLine ? (
                                                <p className="num mt-1 text-xs text-warning-soft-foreground">
                                                    采购超出{" "}
                                                    {qtyWithUnit(
                                                        procurementLine.excessQuantity,
                                                        procurementLine.unitName ??
                                                            line.unitName ??
                                                            "",
                                                    )}
                                                    ，待调减
                                                </p>
                                            ) : hasProcurementGap &&
                                              procurementLine ? (
                                                <p className="num mt-1 text-xs text-warning-soft-foreground">
                                                    尚余{" "}
                                                    {qtyWithUnit(
                                                        procurementLine.remainingQuantity,
                                                        procurementLine.unitName ??
                                                            line.unitName ??
                                                            "",
                                                    )}{" "}
                                                    待安排采购或出库
                                                </p>
                                            ) : null}
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            className="whitespace-nowrap"
                                        >
                                            <QuantityValue
                                                value={line.quantity}
                                                unit={line.unitName ?? ""}
                                            />
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            className="whitespace-nowrap"
                                        >
                                            <MoneyValue
                                                value={line.amountGross}
                                                className="font-medium"
                                            />
                                            <div className="mt-0.5 text-xs text-muted-foreground">
                                                单价{" "}
                                                <MoneyValue
                                                    display="exact"
                                                    value={line.unitPriceGross}
                                                />
                                            </div>
                                            {line.procurementSupplyPrice ? (
                                                <div className="text-xs text-muted-foreground">
                                                    供给价{" "}
                                                    <MoneyValue
                                                        value={
                                                            line.procurementSupplyPrice
                                                        }
                                                    />
                                                </div>
                                            ) : null}
                                        </TableCell>
                                        <TableCell className="min-w-44 whitespace-nowrap">
                                            {!effective ? (
                                                <span className="text-xs text-muted-foreground">
                                                    生效后统计
                                                </span>
                                            ) : progress ? (
                                                <DeliveryLineProgressCell
                                                    progress={progress}
                                                    unit={line.unitName ?? ""}
                                                />
                                            ) : (
                                                <span className="text-xs text-muted-foreground">
                                                    —
                                                </span>
                                            )}
                                        </TableCell>
                                        <TableCell className="whitespace-nowrap">
                                            {!effective ? (
                                                <StatusBadge
                                                    tone="neutral"
                                                    appearance="quiet"
                                                    label="尚未生效"
                                                />
                                            ) : (
                                                <DeliveryLineStatus
                                                    progress={progress}
                                                />
                                            )}
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            className="whitespace-nowrap"
                                        >
                                            {effective ? (
                                                <SalesOrderLineBatchesSheet
                                                    line={line}
                                                    progress={progress}
                                                    canAccept={canAccept}
                                                    onStartAcceptance={
                                                        onStartAcceptance
                                                    }
                                                />
                                            ) : (
                                                <span className="text-xs text-muted-foreground">
                                                    —
                                                </span>
                                            )}
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                    </Table>
                </div>
                <div className="flex items-baseline justify-end gap-4 pt-1 text-body-compact">
                    <span className="text-xs text-muted-foreground">
                        合计（含税）
                    </span>
                    <MoneyValue
                        value={order.amountGross}
                        className="text-value-compact font-semibold tracking-tight"
                    />
                </div>
                {footer}
            </DetailWorkspaceSection>
        </>
    )
}

function DeliveryOverviewMetrics({
    order,
    progress,
}: {
    order: SalesOrderDetailView
    progress: AcceptanceOrderProgress
}) {
    const procurement = order.related.procurementProgress
    const attention = getProcurementAttention(procurement)
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

    const unit = progress.unitName
    const singleUnit = Boolean(progress.unitCode)
    const remainingQty = singleUnit
        ? clampZeroFixed(
              subtractFixed(
                  progress.requiredQuantity,
                  progress.deliveredQuantity,
                  { maxScale: 6, outputScale: 6 },
              ),
              { maxScale: 6, outputScale: 6 },
          )
        : "0"
    const deliveredPct = singleUnit
        ? progressPercent(progress.deliveredQuantity, progress.requiredQuantity)
        : linePercent(deliveredLines, totalLines)
    const acceptedPct = singleUnit
        ? progressPercent(progress.acceptedQuantity, progress.requiredQuantity)
        : linePercent(doneLines, totalLines)
    const hasPending = progress.pendingFactCount > 0

    return (
        <DetailWorkspaceSection width="full">
            <DetailWorkspaceMetrics
                columns={4}
                variant="inline"
                ariaLabel="交付与验收概览"
            >
                <DetailWorkspaceMetricItem
                    emphasis="primary"
                    label="已交付"
                    value={
                        singleUnit ? (
                            qtyWithUnit(progress.deliveredQuantity, unit)
                        ) : (
                            <span>
                                {deliveredLines} / {totalLines} 项
                            </span>
                        )
                    }
                    detail={
                        <div className="w-full space-y-1.5">
                            <ProgressBar
                                deliveredPct={deliveredPct}
                                acceptedPct={acceptedPct}
                            />
                            <span className="num block">
                                {singleUnit
                                    ? isPositiveQty(remainingQty)
                                        ? `销售 ${qtyWithUnit(progress.requiredQuantity, unit)} · 未交 ${qtyWithUnit(remainingQty, unit)}`
                                        : `共 ${qtyWithUnit(progress.requiredQuantity, unit)} · 已全部交付`
                                    : `已全部交付 ${deliveredLines} 项（多单位按项汇总）`}
                            </span>
                        </div>
                    }
                />
                <DetailWorkspaceMetricItem
                    label="待客户验收"
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
                                {progress.pendingFactCount} 批 · {pendingLines}{" "}
                                项待确认
                            </span>
                        ) : (
                            "当前无待验批次"
                        )
                    }
                />
                <DetailWorkspaceMetricItem
                    label="已验收通过"
                    value={
                        singleUnit ? (
                            qtyWithUnit(progress.acceptedQuantity, unit)
                        ) : (
                            <span>
                                {doneLines} / {totalLines} 项
                            </span>
                        )
                    }
                    detail={<span className="num">完成率 {acceptedPct}%</span>}
                />
                <DetailWorkspaceMetricItem
                    label="采购覆盖"
                    value={
                        <StatusBadge
                            tone={
                                attention.needsAttention
                                    ? "warning"
                                    : procurement.tone
                            }
                            label={
                                attention.needsAttention
                                    ? "待处理"
                                    : procurement.label
                            }
                            appearance="text"
                            className="text-value-compact font-semibold"
                        />
                    }
                    detail={
                        attention.hasExcess
                            ? "存在采购超出，待调减"
                            : attention.hasGap
                              ? "部分商品待安排采购或出库"
                              : procurement.sourceChanged
                                ? `${procurement.affectedPurchaseCount} 张采购单待更新`
                                : "按销售明细正常推进"
                    }
                />
            </DetailWorkspaceMetrics>
        </DetailWorkspaceSection>
    )
}

function DeliveryLineProgressCell({
    progress,
    unit,
}: {
    progress: AcceptanceLineProgress
    unit: string
}) {
    const remaining = clampZeroFixed(
        subtractFixed(progress.requiredQuantity, progress.deliveredQuantity, {
            maxScale: 6,
            outputScale: 6,
        }),
        { maxScale: 6, outputScale: 6 },
    )
    const deliveredPct = progressPercent(
        progress.deliveredQuantity,
        progress.requiredQuantity,
    )
    const acceptedPct = progressPercent(
        progress.acceptedQuantity,
        progress.requiredQuantity,
    )
    const hasPending = isPositiveQty(progress.pendingQuantity)
    const hasRemaining = isPositiveQty(remaining)

    return (
        <div className="space-y-1.5">
            <ProgressBar
                deliveredPct={deliveredPct}
                acceptedPct={acceptedPct}
            />
            <div className="num flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                <span>
                    已交{" "}
                    <span className="font-medium text-foreground">
                        {qtyWithUnit(progress.deliveredQuantity, unit)}
                    </span>
                </span>
                {hasRemaining ? (
                    <span>未交 {qtyWithUnit(remaining, unit)}</span>
                ) : null}
            </div>
            <div className="num flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                <span>
                    已验{" "}
                    <span className="font-medium text-foreground">
                        {qtyWithUnit(progress.acceptedQuantity, unit)}
                    </span>
                </span>
                {hasPending ? (
                    <span className="font-medium text-primary">
                        待验 {qtyWithUnit(progress.pendingQuantity, unit)}
                    </span>
                ) : null}
            </div>
        </div>
    )
}

function ProgressBar({
    deliveredPct,
    acceptedPct,
}: {
    deliveredPct: string
    acceptedPct: string
}) {
    return (
        <div
            aria-hidden="true"
            className="relative h-1.5 w-full max-w-40 overflow-hidden rounded-full bg-muted"
        >
            <div
                className="absolute inset-y-0 left-0 rounded-full bg-primary/35 transition-all"
                style={{ width: `${deliveredPct}%` }}
            />
            <div
                className="absolute inset-y-0 left-0 rounded-full bg-success transition-all"
                style={{ width: `${acceptedPct}%` }}
            />
        </div>
    )
}

/** 销售行按交付与验收数量展示状态，缺失进度保持待确认。 */
function DeliveryLineStatus({
    progress,
}: {
    progress?: AcceptanceLineProgress
}) {
    if (!progress) {
        return <span className="text-xs text-muted-foreground">待确认</span>
    }
    const partiallyDelivered =
        compareDecimal(progress.deliveredQuantity, "0", 6) > 0 &&
        compareDecimal(
            progress.deliveredQuantity,
            progress.requiredQuantity,
            6,
        ) < 0

    if (progress.stuckKind === "done") {
        return <StatusBadge tone="success" appearance="quiet" label="已验收" />
    }
    if (progress.stuckKind === "exception") {
        return <StatusBadge tone="warning" label="待处理" />
    }
    if (progress.stuckKind === "accept") {
        return (
            <StatusBadge
                tone="info"
                label={partiallyDelivered ? "部分交付 · 待验收" : "待验收"}
            />
        )
    }
    return (
        <StatusBadge
            tone={partiallyDelivered ? "info" : "neutral"}
            appearance={partiallyDelivered ? "badge" : "quiet"}
            label={partiallyDelivered ? "部分交付" : "待交付"}
        />
    )
}

/** 交付数据不可用时说明原因，不以零数量或完成状态替代。 */
function DeliveryProgressNotice({
    order,
    query,
    canRead,
}: {
    order: SalesOrderDetailView
    query: DeliveryProgressQuery
    canRead: boolean
}) {
    return (
        <>
            {order.currentRevisionNo == null ? (
                <p className="text-xs text-muted-foreground">
                    销售单生效后显示交付与验收进度。
                </p>
            ) : !canRead ? (
                <p className="text-xs text-muted-foreground">
                    当前无法读取交付进度；商品约定仍可查阅。
                </p>
            ) : query.isError ? (
                <BusinessFailureState
                    id="sales-order-delivery-overview-failure"
                    title="交付进度读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : query.isPending ? (
                <p role="status" className="text-xs text-muted-foreground">
                    正在读取交付进度…
                </p>
            ) : null}
        </>
    )
}

function progressPercent(part: string, total: string): string {
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
