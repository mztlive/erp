"use client"

import * as React from "react"
import {
    AlertTriangleIcon,
    ArrowUpRightIcon,
    CalendarDaysIcon,
    ChevronDownIcon,
    ChevronRightIcon,
    FileTextIcon,
    MapPinIcon,
    RefreshCwIcon,
    TriangleAlertIcon,
    WarehouseIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceMeta,
    FulfillmentRouteBadge,
    MoneyValue,
    QuantityValue,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { useCreationBasesQuery } from "@/features/purchase-orders/hooks/queries"
import { sourcingDraftKey } from "@/features/purchase-orders/hooks/use-sourcing-command"
import type { PurchaseOrderCreateFormValues } from "@/features/purchase-orders/lib/purchase-order-create-form-types"
import {
    buildDefaultSourcingLines,
    buildPurchaseOrderPreviews,
    buildSourcingWorkspace,
    buildStockAllocationPreviews,
    findSourcingOption,
    sourcingFormLinesReady,
    sumPreviewTotals,
    type SourcingLineInput,
    type SourcingProductLine,
    type SourcingSupplierOption,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import {
    deliverySummary,
    sourcingRouteLabel,
} from "@/features/purchase-orders/lib/sourcing/delivery-terms"
import {
    buildSourcingEditorRows,
    type SourcingEditorRow,
} from "@/features/purchase-orders/lib/sourcing/editor-rows"
import {
    comparePurchaseCostToSalesPrice,
    earliestOrderDeliveryDeadline,
    findLowestPurchaseCost,
    priceDeltaFromLowest,
    summarizeRowPricing,
} from "@/features/purchase-orders/lib/sourcing/pricing"
import { SalesOrderPaperPreviewDialog } from "@/features/sales-orders/components/sales-order-paper-preview-dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import { readInquiryLocalValue } from "@/lib/inquiry-local"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskResponsibilityActions } from "./workspace-task-responsibility-actions"

export type WorkspaceProcurementInspectorProps = Readonly<{
    item: WorkspaceWorkItem
    onOpenFullPage: () => void
    onCompleted?: (completedId: string) => void
    presentation: "pane" | "sheet"
    userId?: string
    permissions?: readonly string[]
}>

type InspectorProductFilter = "all" | "attention" | "inverted"

function isPositiveQuantity(value: string | undefined): boolean {
    if (!value?.trim()) return false
    try {
        return compareDecimal(value, "0", 6) > 0
    } catch {
        return false
    }
}

function rowStatusTone(status: string): StatusTone {
    if (status === "已就绪") return "success"
    if (status === "缺目标仓" || status === "部分分配") return "warning"
    if (status === "待调整") return "destructive"
    return "neutral"
}

function optionDisplayName(option: SourcingSupplierOption): string {
    if (option.sourceType === "EXISTING_STOCK") {
        return option.warehouseName || option.supplierName || "现有库存"
    }
    return option.supplierName || "未命名供应商"
}

function sortCandidateOptions(
    options: readonly SourcingSupplierOption[],
): SourcingSupplierOption[] {
    return [...options].sort((left, right) => {
        if (left.sourceType !== right.sourceType) {
            return left.sourceType === "EXISTING_STOCK" ? -1 : 1
        }
        if (left.sourceType === "PURCHASE") {
            try {
                const costCmp = compareDecimal(
                    left.unitCostGross,
                    right.unitCostGross,
                    4,
                )
                if (costCmp !== 0) return costCmp
            } catch {
                // 非法金额回落名称排序
            }
        }
        return optionDisplayName(left).localeCompare(
            optionDisplayName(right),
            "zh-CN",
        )
    })
}

function resolveEffectiveLines(
    order: NonNullable<ReturnType<typeof buildSourcingWorkspace>[number]>,
    userId?: string,
): { lines: SourcingLineInput[]; usingLocalDraft: boolean } {
    if (userId) {
        const draftKey = sourcingDraftKey(
            userId,
            order.salesOrderId,
            order.workItemId,
        )
        const saved =
            readInquiryLocalValue<PurchaseOrderCreateFormValues>(draftKey)
        if (
            saved?.salesOrderId === order.salesOrderId &&
            Array.isArray(saved.lines) &&
            sourcingFormLinesReady(saved.lines, order)
        ) {
            const optionsValid = saved.lines.every((line) => {
                if (!line.basisId) return true
                const product = order.lines.find(
                    (candidate) =>
                        candidate.salesOrderLineId === line.salesOrderLineId,
                )
                return Boolean(findSourcingOption(product, line.basisId))
            })
            if (optionsValid) {
                return { lines: saved.lines, usingLocalDraft: true }
            }
        }
    }
    return {
        lines: buildDefaultSourcingLines(order),
        usingLocalDraft: false,
    }
}

/**
 * 供给分配非全屏紧凑快览检查器（方案 A）：
 * - 专面向分栏右窗格与 768px `TaskActionSheet`，零横向滚动；
 * - 复用 `useCreationBasesQuery` 缓存，与完整作业面零重复拉取；
 * - 展示待分配商品、系统推荐/暂存方案、采售价对比与倒挂预警、候选来源展开和预计拆单结果。
 */
export function WorkspaceProcurementInspector({
    item,
    onOpenFullPage,
    onCompleted,
    presentation,
    userId,
    permissions = [],
}: WorkspaceProcurementInspectorProps) {
    const [filter, setFilter] = React.useState<InspectorProductFilter>("all")
    const [expandedIds, setExpandedIds] = React.useState<ReadonlySet<string>>(
        () => new Set(),
    )
    const [salesPreviewOpen, setSalesPreviewOpen] = React.useState(false)
    const [draftRevision, setDraftRevision] = React.useState(0)

    React.useEffect(() => {
        setFilter("all")
        setExpandedIds(new Set())
        setSalesPreviewOpen(false)
    }, [item.workItemId])

    const basesQuery = useCreationBasesQuery({
        salesOrderId: item.businessObjectId,
        workItemId: item.workItemId,
    })

    const workspaceOrders = React.useMemo(
        () => buildSourcingWorkspace(basesQuery.data ?? []),
        [basesQuery.data],
    )
    const sourcingOrder = React.useMemo(
        () =>
            workspaceOrders.find(
                (candidate) => candidate.salesOrderId === item.businessObjectId,
            ),
        [item.businessObjectId, workspaceOrders],
    )
    const basis = React.useMemo(
        () =>
            basesQuery.data?.find(
                (entry) => entry.salesOrderId === item.businessObjectId,
            ),
        [basesQuery.data, item.businessObjectId],
    )

    const watchedDraftKey =
        userId && sourcingOrder
            ? sourcingDraftKey(
                  userId,
                  sourcingOrder.salesOrderId,
                  sourcingOrder.workItemId,
              )
            : undefined

    React.useEffect(() => {
        if (!watchedDraftKey || typeof window === "undefined") return
        const onStorage = (event: StorageEvent) => {
            if (event.key === null || event.key === watchedDraftKey) {
                setDraftRevision((value) => value + 1)
            }
        }
        window.addEventListener("storage", onStorage)
        return () => window.removeEventListener("storage", onStorage)
    }, [watchedDraftKey])

    const { lines: effectiveLines, usingLocalDraft } = React.useMemo(() => {
        void draftRevision
        return sourcingOrder
            ? resolveEffectiveLines(sourcingOrder, userId)
            : { lines: [] as SourcingLineInput[], usingLocalDraft: false }
    }, [draftRevision, sourcingOrder, userId])

    const editorRows = React.useMemo(
        () =>
            sourcingOrder
                ? buildSourcingEditorRows(sourcingOrder, effectiveLines)
                : [],
        [effectiveLines, sourcingOrder],
    )
    const rowPricings = React.useMemo(
        () =>
            new Map(
                editorRows.map((row) => [
                    row.product.salesOrderLineId,
                    summarizeRowPricing(row),
                ]),
            ),
        [editorRows],
    )
    const previews = React.useMemo(
        () => buildPurchaseOrderPreviews(sourcingOrder, effectiveLines),
        [effectiveLines, sourcingOrder],
    )
    const stockPreviews = React.useMemo(
        () => buildStockAllocationPreviews(sourcingOrder, effectiveLines),
        [effectiveLines, sourcingOrder],
    )
    const previewTotals = React.useMemo(
        () =>
            sumPreviewTotals(previews.flatMap((preview) => [...preview.lines])),
        [previews],
    )

    const earliestDeadline = earliestOrderDeliveryDeadline(sourcingOrder)
    const canReadSalesOrder = hasPermission(permissions, "sales_order:detail")
    const segment = toAutomationIdSegment(item.workItemId)
    const padClass = presentation === "sheet" ? "px-sheet-inline" : "px-5"
    const blocked = isBlockedWorkItem(item)
    const overdue = item.status === "OPEN" && item.dueBucket === "overdue"
    const salesOrderNo =
        sourcingOrder?.salesOrderNo ||
        stripDocumentNumberPrefix(item.stableNumber)

    const metrics = React.useMemo(() => {
        let attentionCount = 0
        let invertedCount = 0
        let missingWarehouseCount = 0
        let noOptionCount = 0
        let coveredLineCount = 0
        for (const row of editorRows) {
            if (row.needsAttention) attentionCount += 1
            if (row.status === "缺目标仓") missingWarehouseCount += 1
            if (row.product.options.length === 0) noOptionCount += 1
            if (isPositiveQuantity(row.product.coveredQuantity)) {
                coveredLineCount += 1
            }
            const pricing = rowPricings.get(row.product.salesOrderLineId)
            if (
                (row.selected || row.partiallySelected) &&
                pricing?.hasInvertedSource
            ) {
                invertedCount += 1
            }
        }
        return {
            attentionCount,
            invertedCount,
            missingWarehouseCount,
            noOptionCount,
            coveredLineCount,
        }
    }, [editorRows, rowPricings])

    const visibleRows = React.useMemo(() => {
        if (filter === "attention") {
            return editorRows.filter((row) => row.needsAttention)
        }
        if (filter === "inverted") {
            return editorRows.filter((row) => {
                const pricing = rowPricings.get(row.product.salesOrderLineId)
                return (
                    (row.selected || row.partiallySelected) &&
                    Boolean(pricing?.hasInvertedSource)
                )
            })
        }
        return editorRows
    }, [editorRows, filter, rowPricings])

    const splitSummaryText = React.useMemo(() => {
        const parts: string[] = []
        if (stockPreviews.length > 0) {
            parts.push(`${stockPreviews.length} 项现货`)
        }
        if (previews.length > 0) {
            parts.push(`${previews.length} 张采购单`)
        }
        return parts.length > 0 ? parts.join(" · ") : "待选源"
    }, [previews.length, stockPreviews.length])

    const riskHints = React.useMemo(() => {
        const hints: string[] = []
        if (metrics.invertedCount > 0) {
            hints.push(`${metrics.invertedCount} 项价格倒挂`)
        }
        if (metrics.noOptionCount > 0) {
            hints.push(`${metrics.noOptionCount} 项暂无可用供给`)
        }
        if (metrics.missingWarehouseCount > 0) {
            hints.push(`${metrics.missingWarehouseCount} 项需指定入库目标仓`)
        }
        const otherAttention = Math.max(
            0,
            metrics.attentionCount -
                metrics.missingWarehouseCount -
                metrics.noOptionCount,
        )
        if (otherAttention > 0) {
            hints.push(`${otherAttention} 项待补充交期或数量`)
        }
        return hints
    }, [metrics])

    const toggleExpanded = (lineId: string) => {
        setExpandedIds((current) => {
            const next = new Set(current)
            if (next.has(lineId)) next.delete(lineId)
            else next.add(lineId)
            return next
        })
    }

    const footer = (
        <footer
            data-slot="workspace-inspector-footer"
            className={cn(
                "flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-border/60 bg-card py-3 shadow-footer",
                padClass,
            )}
        >
            <div className="flex flex-wrap items-center gap-2">
                <WorkspaceTaskResponsibilityActions
                    item={item}
                    onCompleted={onCompleted}
                />
            </div>
            <Button
                id={`workspace-inspector-enter-full-${segment}`}
                type="button"
                size="sm"
                onClick={onOpenFullPage}
            >
                进入完整作业面调整
                <ArrowUpRightIcon data-icon="inline-end" aria-hidden="true" />
            </Button>
        </footer>
    )

    return (
        <div
            data-slot="workspace-procurement-inspector"
            className="@container/procurement-inspector flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-card"
        >
            <div
                data-slot="workspace-task-inspector-controls"
                className={cn(
                    "flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken/60 py-1.5 text-xs",
                    padClass,
                )}
            >
                <div className="flex min-w-0 items-center gap-2">
                    <span className="truncate text-muted-foreground">
                        {usingLocalDraft
                            ? "供给分配快览（含未提交草稿）"
                            : "供给分配快览（系统推荐方案）"}
                    </span>
                    {usingLocalDraft ? (
                        <StatusBadge
                            label="已暂存"
                            tone="info"
                            appearance="quiet"
                        />
                    ) : null}
                </div>
                <div className="flex shrink-0 items-center gap-1">
                    {canReadSalesOrder ? (
                        <Button
                            id={`workspace-procurement-inspector-sales-${segment}`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className="h-control-xs px-2 text-xs text-muted-foreground hover:text-foreground"
                            onClick={() => setSalesPreviewOpen(true)}
                        >
                            <FileTextIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            查看销售单
                        </Button>
                    ) : null}
                    <Button
                        id={`workspace-procurement-inspector-refresh-${segment}`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        className="h-control-xs px-2 text-xs text-muted-foreground hover:text-foreground"
                        disabled={basesQuery.isFetching}
                        onClick={() => {
                            setDraftRevision((value) => value + 1)
                            void basesQuery.refetch()
                        }}
                    >
                        <RefreshCwIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        刷新依据
                    </Button>
                </div>
            </div>

            {basesQuery.isPending ? (
                <div
                    aria-busy="true"
                    className={cn(
                        "flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto py-5",
                        padClass,
                    )}
                >
                    <span role="status" className="sr-only">
                        正在读取供给分配方案…
                    </span>
                    <Skeleton className="h-4 w-72" />
                    <div className="grid grid-cols-2 gap-3 border-y border-border/60 py-3 @[560px]/procurement-inspector:grid-cols-4">
                        <Skeleton className="h-11 w-full" />
                        <Skeleton className="h-11 w-full" />
                        <Skeleton className="h-11 w-full" />
                        <Skeleton className="h-11 w-full" />
                    </div>
                    <Skeleton className="h-20 w-full" />
                    <Skeleton className="h-20 w-full" />
                    <Skeleton className="h-20 w-full" />
                </div>
            ) : basesQuery.isError ? (
                <div
                    className={cn(
                        "flex min-h-0 flex-1 flex-col justify-center py-6",
                        padClass,
                    )}
                >
                    <BusinessFailureState
                        idPrefix={`workspace-procurement-inspector-failure-${segment}`}
                        title="供给分配依据读取失败"
                        error={basesQuery.error}
                        onRetry={() => void basesQuery.refetch()}
                        action={
                            <Button
                                id={`workspace-procurement-inspector-error-full-${segment}`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={onOpenFullPage}
                            >
                                在完整页面打开
                                <ArrowUpRightIcon
                                    data-icon="inline-end"
                                    aria-hidden="true"
                                />
                            </Button>
                        }
                    />
                </div>
            ) : !sourcingOrder ? (
                <EmptySourcingFallback
                    item={item}
                    padClass={padClass}
                    blocked={blocked}
                    overdue={overdue}
                />
            ) : (
                <div
                    className={cn(
                        "min-h-0 flex-1 space-y-4 overflow-y-auto overscroll-contain py-4",
                        padClass,
                    )}
                >
                    <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5">
                        {overdue ? (
                            <StatusBadge label="已超期" tone="destructive" />
                        ) : null}
                        <DetailWorkspaceMeta
                            items={[
                                basis?.contractNumber
                                    ? `合同 ${basis.contractNumber}`
                                    : null,
                                earliestDeadline ? (
                                    <span key="deadline" className="num">
                                        最晚交付 {earliestDeadline}
                                    </span>
                                ) : null,
                                basis?.salesOwnerName
                                    ? `销售 ${basis.salesOwnerName}`
                                    : null,
                                item.ownerUserLabel
                                    ? `采购 ${item.ownerUserLabel}`
                                    : null,
                                `进入 ${item.enteredAtLabel}`,
                                item.dueAt ? `截止 ${item.dueAtLabel}` : null,
                            ]}
                        />
                    </div>

                    {blocked && item.actionBlockers.length > 0 ? (
                        <Alert variant="warning">
                            <AlertTriangleIcon aria-hidden="true" />
                            <AlertTitle>当前存在办理限制</AlertTitle>
                            <AlertDescription>
                                {item.actionBlockers
                                    .map((blocker) => blocker.message)
                                    .join("；")}
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {riskHints.length > 0 ? (
                        <Alert
                            variant={
                                metrics.invertedCount > 0 ||
                                metrics.noOptionCount > 0
                                    ? "destructive"
                                    : "warning"
                            }
                        >
                            <TriangleAlertIcon aria-hidden="true" />
                            <AlertTitle>提交前需在完整作业面核对</AlertTitle>
                            <AlertDescription>
                                {riskHints.join("；")}
                                。点击右下角「进入完整作业面调整」可换源、指定目标仓或拆单提交。
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    <dl
                        aria-label="供给分配汇总"
                        className="grid grid-cols-2 gap-x-4 gap-y-2.5 border-y border-border/60 py-3 @[560px]/procurement-inspector:grid-cols-4"
                    >
                        <div className="min-w-0 space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                待分配商品
                            </dt>
                            <dd className="num text-body-compact font-semibold text-foreground">
                                {sourcingOrder.lines.length} 项
                            </dd>
                            {metrics.coveredLineCount > 0 ? (
                                <p className="num text-tiny text-muted-foreground">
                                    含 {metrics.coveredLineCount} 项已部分覆盖
                                </p>
                            ) : null}
                        </div>

                        <div className="min-w-0 space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                预计拆单
                            </dt>
                            <dd className="num text-body-compact font-semibold text-foreground">
                                {splitSummaryText}
                            </dd>
                        </div>

                        <div className="min-w-0 space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                预估采购含税
                            </dt>
                            <dd className="num text-body-compact font-semibold text-foreground">
                                {previews.length > 0 ? (
                                    <MoneyValue
                                        value={previewTotals.gross}
                                        display="exact"
                                    />
                                ) : (
                                    <span className="font-medium text-info-soft-foreground">
                                        无需采购
                                    </span>
                                )}
                            </dd>
                        </div>

                        <div className="min-w-0 space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                当前方案状态
                            </dt>
                            <dd className="pt-0.5">
                                {metrics.attentionCount > 0 ? (
                                    <StatusBadge
                                        label={`${metrics.attentionCount} 项待调整`}
                                        tone={
                                            metrics.noOptionCount > 0
                                                ? "destructive"
                                                : "warning"
                                        }
                                    />
                                ) : metrics.invertedCount > 0 ? (
                                    <StatusBadge
                                        label={`${metrics.invertedCount} 项倒挂`}
                                        tone="destructive"
                                    />
                                ) : (
                                    <StatusBadge
                                        label="已就绪"
                                        tone="success"
                                    />
                                )}
                            </dd>
                        </div>
                    </dl>

                    <section
                        aria-label="待分配商品明细"
                        className="space-y-2.5"
                    >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <h3 className="text-xs font-semibold text-muted-foreground">
                                商品供给方案（{visibleRows.length}/
                                {editorRows.length}）
                            </h3>
                            {metrics.attentionCount > 0 ||
                            metrics.invertedCount > 0 ? (
                                <div
                                    role="group"
                                    aria-label="按状态筛选供给商品"
                                    className="flex flex-wrap items-center gap-1"
                                >
                                    <Button
                                        id={`workspace-procurement-inspector-filter-all-${segment}`}
                                        type="button"
                                        size="xs"
                                        variant={
                                            filter === "all"
                                                ? "secondary"
                                                : "ghost"
                                        }
                                        aria-pressed={filter === "all"}
                                        onClick={() => setFilter("all")}
                                    >
                                        全部{" "}
                                        <span className="num text-muted-foreground">
                                            {editorRows.length}
                                        </span>
                                    </Button>
                                    {metrics.attentionCount > 0 ||
                                    filter === "attention" ? (
                                        <Button
                                            id={`workspace-procurement-inspector-filter-attention-${segment}`}
                                            type="button"
                                            size="xs"
                                            variant={
                                                filter === "attention"
                                                    ? "secondary"
                                                    : "ghost"
                                            }
                                            aria-pressed={
                                                filter === "attention"
                                            }
                                            className="text-warning-soft-foreground hover:text-warning-soft-foreground"
                                            onClick={() =>
                                                setFilter("attention")
                                            }
                                        >
                                            待调整{" "}
                                            <span className="num font-medium">
                                                {metrics.attentionCount}
                                            </span>
                                        </Button>
                                    ) : null}
                                    {metrics.invertedCount > 0 ||
                                    filter === "inverted" ? (
                                        <Button
                                            id={`workspace-procurement-inspector-filter-inverted-${segment}`}
                                            type="button"
                                            size="xs"
                                            variant={
                                                filter === "inverted"
                                                    ? "secondary"
                                                    : "ghost"
                                            }
                                            aria-pressed={filter === "inverted"}
                                            className="text-destructive hover:text-destructive"
                                            onClick={() =>
                                                setFilter("inverted")
                                            }
                                        >
                                            倒挂{" "}
                                            <span className="num font-medium">
                                                {metrics.invertedCount}
                                            </span>
                                        </Button>
                                    ) : null}
                                </div>
                            ) : null}
                        </div>

                        {visibleRows.length === 0 ? (
                            <p className="py-6 text-center text-xs text-muted-foreground">
                                当前筛选条件下没有商品。
                            </p>
                        ) : (
                            <ul className="divide-y divide-border/60 border-t border-border/60">
                                {visibleRows.map((row) => (
                                    <SourcingInspectorProductRow
                                        key={row.product.salesOrderLineId}
                                        row={row}
                                        pricing={
                                            rowPricings.get(
                                                row.product.salesOrderLineId,
                                            ) ?? summarizeRowPricing(row)
                                        }
                                        expanded={expandedIds.has(
                                            row.product.salesOrderLineId,
                                        )}
                                        onToggleExpanded={() =>
                                            toggleExpanded(
                                                row.product.salesOrderLineId,
                                            )
                                        }
                                        workItemSegment={segment}
                                    />
                                ))}
                            </ul>
                        )}
                    </section>

                    {stockPreviews.length > 0 || previews.length > 0 ? (
                        <section
                            aria-label="预计生成单据"
                            className="space-y-2.5 border-t border-border/60 pt-4"
                        >
                            <h3 className="text-xs font-semibold text-muted-foreground">
                                预计生成单据
                            </h3>
                            <ul className="divide-y divide-border/60 border-t border-border/60">
                                {stockPreviews.map((stock, idx) => (
                                    <li
                                        key={`${stock.salesOrderLineId}:${stock.warehouseName}:${idx}`}
                                        className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1 py-2 text-body-compact"
                                    >
                                        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                                            <FulfillmentRouteBadge
                                                route="stock"
                                                label="库存预留"
                                                size="compact"
                                            />
                                            <span className="truncate font-medium text-foreground">
                                                {stock.warehouseName}
                                            </span>
                                            <span className="truncate text-xs text-muted-foreground">
                                                {stock.itemName}
                                            </span>
                                        </div>
                                        <QuantityValue
                                            value={stock.quantity}
                                            unit={stock.unit}
                                            className="shrink-0 text-xs"
                                        />
                                    </li>
                                ))}
                                {previews.map((preview) => {
                                    const needsTargetWarehouse =
                                        preview.fulfillmentResponsibility ===
                                        "WAREHOUSE"
                                    return (
                                        <li
                                            key={preview.key}
                                            className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1 py-2 text-body-compact"
                                        >
                                            <div className="min-w-0 space-y-0.5">
                                                <div className="flex flex-wrap items-center gap-1.5">
                                                    <FulfillmentRouteBadge
                                                        route={
                                                            preview.fulfillmentResponsibility ===
                                                            "WAREHOUSE"
                                                                ? "warehouse"
                                                                : preview.routeLabel.includes(
                                                                        "集采",
                                                                    )
                                                                  ? "direct_bulk"
                                                                  : preview.fulfillmentResponsibility ===
                                                                      "SUPPLIER_DIRECT"
                                                                    ? "direct_dropship"
                                                                    : "other"
                                                        }
                                                        label={
                                                            preview.routeLabel
                                                        }
                                                        size="compact"
                                                    />
                                                    <span className="truncate font-medium text-foreground">
                                                        {preview.supplierName}
                                                    </span>
                                                    <span className="num text-xs text-muted-foreground">
                                                        {preview.lines.length}{" "}
                                                        项
                                                    </span>
                                                </div>
                                                <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                                                    {preview.paymentTermLabel ? (
                                                        <span>
                                                            {
                                                                preview.paymentTermLabel
                                                            }
                                                        </span>
                                                    ) : null}
                                                    {needsTargetWarehouse ? (
                                                        preview.targetWarehouseName ? (
                                                            <span className="inline-flex items-center gap-0.5">
                                                                <WarehouseIcon
                                                                    aria-hidden="true"
                                                                    className="size-3 shrink-0"
                                                                />
                                                                入{" "}
                                                                {
                                                                    preview.targetWarehouseName
                                                                }
                                                            </span>
                                                        ) : (
                                                            <span className="text-warning-soft-foreground">
                                                                待选入库目标仓
                                                            </span>
                                                        )
                                                    ) : null}
                                                </div>
                                            </div>
                                            <span className="shrink-0 font-semibold">
                                                <MoneyValue
                                                    value={preview.totals.gross}
                                                    display="exact"
                                                />
                                            </span>
                                        </li>
                                    )
                                })}
                            </ul>
                        </section>
                    ) : null}
                </div>
            )}

            {footer}

            {canReadSalesOrder ? (
                <SalesOrderPaperPreviewDialog
                    salesOrderId={item.businessObjectId}
                    title={salesOrderNo}
                    open={salesPreviewOpen}
                    onOpenChange={setSalesPreviewOpen}
                />
            ) : null}
        </div>
    )
}

function SourcingInspectorProductRow({
    row,
    pricing,
    expanded,
    onToggleExpanded,
    workItemSegment,
}: {
    row: SourcingEditorRow
    pricing: ReturnType<typeof summarizeRowPricing>
    expanded: boolean
    onToggleExpanded: () => void
    workItemSegment: string
}) {
    const product = row.product
    const unit = product.unitName ?? ""
    const included = row.selected || row.partiallySelected
    const lineSeg = toAutomationIdSegment(product.salesOrderLineId)
    const addressCount = product.deliveryAllocations.length
    const hasCovered = isPositiveQuantity(product.coveredQuantity)
    const canExpand = product.options.length > 1 || addressCount > 0

    const activeAllocations = React.useMemo(() => {
        const selected = row.allocations.filter(({ line }) => line.selected)
        return selected.length > 0 ? selected : row.allocations
    }, [row.allocations])

    const selectedBasisIds = React.useMemo(
        () =>
            new Set(
                activeAllocations
                    .map(({ line }) => line.basisId)
                    .filter(Boolean),
            ),
        [activeAllocations],
    )

    return (
        <li className={cn("space-y-2 py-3", !included && "opacity-65")}>
            {/* 第一层：商品身份、交付约束、数量与行状态 */}
            <div className="flex items-start justify-between gap-3">
                <div className="min-w-0 flex-1 space-y-0.5">
                    <p
                        className="truncate text-body-compact font-medium text-foreground"
                        title={product.itemName}
                    >
                        {product.itemName}
                    </p>
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                        {product.itemSku ? (
                            <span
                                className="num max-w-44 truncate"
                                title={product.itemSku}
                            >
                                {product.itemSku}
                            </span>
                        ) : null}
                        {product.deliveryDeadline ? (
                            <span className="num inline-flex items-center gap-0.5">
                                <CalendarDaysIcon
                                    aria-hidden="true"
                                    className="size-3 shrink-0 opacity-75"
                                />
                                限 {product.deliveryDeadline}
                            </span>
                        ) : null}
                        {addressCount > 0 ? (
                            <span className="num inline-flex items-center gap-0.5">
                                <MapPinIcon
                                    aria-hidden="true"
                                    className="size-3 shrink-0 opacity-75"
                                />
                                {addressCount} 个地址
                            </span>
                        ) : null}
                        {hasCovered ? (
                            <span className="num">
                                已覆盖 {product.coveredQuantity}
                                {unit ? ` ${unit}` : ""}
                            </span>
                        ) : null}
                    </div>
                </div>

                <div className="flex shrink-0 items-center gap-2">
                    <div className="num text-right text-xs whitespace-nowrap">
                        <span
                            className={cn(
                                "text-body-compact font-semibold",
                                row.status === "部分分配"
                                    ? "text-warning-soft-foreground"
                                    : "text-foreground",
                            )}
                        >
                            {included ? row.quantity : "—"}
                        </span>
                        <span className="mx-1 text-muted-foreground">/</span>
                        <QuantityValue
                            value={product.remainingQuantity}
                            unit={unit}
                            className="inline-flex text-xs [&_.num]:text-muted-foreground"
                        />
                    </div>
                    <StatusBadge
                        label={row.status}
                        tone={rowStatusTone(row.status)}
                        appearance={row.status === "已就绪" ? "quiet" : "badge"}
                    />
                </div>
            </div>

            {/* 第二层：供给来源方案 vs 采购价与售价对比 */}
            <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-1.5">
                <div className="min-w-0 flex-1 space-y-1">
                    {activeAllocations.map(({ line }) => {
                        const option = findSourcingOption(product, line.basisId)
                        if (!option) {
                            return (
                                <div
                                    key={line.rowKey}
                                    className="flex flex-wrap items-center gap-1.5 text-xs text-destructive"
                                >
                                    <TriangleAlertIcon
                                        aria-hidden="true"
                                        className="size-3.5 shrink-0"
                                    />
                                    <span>
                                        {product.options.length === 0
                                            ? "暂无可用库存或合格供应商供给"
                                            : "尚未选定供给来源"}
                                    </span>
                                </div>
                            )
                        }
                        const isStock = option.sourceType === "EXISTING_STOCK"
                        const needsWarehouse =
                            !isStock &&
                            option.fulfillmentResponsibility === "WAREHOUSE"
                        const termNote = deliverySummary(option, product)
                        const hasTermWarning =
                            termNote.includes("运费另计") ||
                            termNote.includes("不配送")
                        return (
                            <div
                                key={line.rowKey}
                                className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs"
                            >
                                <FulfillmentRouteBadge
                                    option={option}
                                    label={sourcingRouteLabel(option)}
                                    size="compact"
                                />
                                <span
                                    className="max-w-64 truncate font-medium text-foreground"
                                    title={optionDisplayName(option)}
                                >
                                    {optionDisplayName(option)}
                                </span>
                                {activeAllocations.length > 1 ? (
                                    <span className="num text-muted-foreground">
                                        × {line.quantity}
                                        {unit ? ` ${unit}` : ""}
                                        {!isStock && option.unitCostGross ? (
                                            <>
                                                {" · "}
                                                <MoneyValue
                                                    display="exact"
                                                    value={option.unitCostGross}
                                                    inheritColor
                                                />
                                            </>
                                        ) : null}
                                    </span>
                                ) : null}
                                {needsWarehouse ? (
                                    line.targetWarehouseName ? (
                                        <span className="inline-flex items-center gap-0.5 text-muted-foreground">
                                            <WarehouseIcon
                                                aria-hidden="true"
                                                className="size-3 shrink-0"
                                            />
                                            {line.targetWarehouseName}
                                        </span>
                                    ) : (
                                        <span className="text-warning-soft-foreground">
                                            待选目标仓
                                        </span>
                                    )
                                ) : null}
                                <span className="num text-muted-foreground">
                                    {isStock
                                        ? "库存出库"
                                        : line.expectedDeliveryDate
                                          ? `预计 ${line.expectedDeliveryDate}`
                                          : "待填交期"}
                                </span>
                                {hasTermWarning ? (
                                    <span className="text-warning-soft-foreground">
                                        {termNote}
                                    </span>
                                ) : null}
                            </div>
                        )
                    })}
                </div>

                <div className="flex shrink-0 flex-col items-end gap-0.5 text-right">
                    <div className="flex flex-wrap items-center justify-end gap-1.5">
                        {pricing.kind === "purchase" &&
                        pricing.unitCostGross ? (
                            <span
                                className={cn(
                                    "text-body-compact font-semibold",
                                    pricing.hasInvertedSource &&
                                        "text-destructive",
                                )}
                            >
                                <MoneyValue
                                    display="exact"
                                    value={pricing.unitCostGross}
                                    inheritColor={pricing.hasInvertedSource}
                                />
                            </span>
                        ) : pricing.kind === "stock" ? (
                            <span className="text-xs font-medium text-info-soft-foreground">
                                库存核算
                            </span>
                        ) : pricing.kind === "multiple" ? (
                            <span className="text-xs font-medium text-foreground">
                                多个价格
                            </span>
                        ) : (
                            <span className="text-xs text-muted-foreground">
                                —
                            </span>
                        )}

                        {pricing.hasInvertedSource ? (
                            <StatusBadge
                                tone="destructive"
                                label={`倒挂${pricing.margin?.marginRateText ? ` ${pricing.margin.marginRateText}` : ""}`}
                                className="h-4.5 px-1.5 text-tiny"
                            />
                        ) : pricing.margin?.marginRateText ? (
                            <span className="num text-tiny font-medium text-success-soft-foreground">
                                {pricing.margin.marginRateText}
                            </span>
                        ) : null}
                    </div>
                    {pricing.salesUnitPriceGross ? (
                        <span className="num text-xs text-muted-foreground">
                            售{" "}
                            <MoneyValue
                                display="exact"
                                value={pricing.salesUnitPriceGross}
                                inheritColor
                            />
                        </span>
                    ) : null}
                </div>
            </div>

            {/* 第三层：行级问题与按需展开候选方案/地址 */}
            {row.issues.length > 0 ? (
                <p className="text-xs text-destructive">
                    {row.issues.join("；")}
                </p>
            ) : null}

            {canExpand ? (
                <div>
                    <Button
                        id={`workspace-procurement-inspector-expand-${workItemSegment}-${lineSeg}`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        aria-expanded={expanded}
                        className="-ml-2 h-control-xs px-2 text-xs text-muted-foreground hover:text-foreground"
                        onClick={onToggleExpanded}
                    >
                        {expanded ? (
                            <ChevronDownIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        ) : (
                            <ChevronRightIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        )}
                        {[
                            product.options.length > 1
                                ? `全部 ${product.options.length} 个候选方案`
                                : null,
                            addressCount > 0
                                ? `${addressCount} 个收货地址`
                                : null,
                        ]
                            .filter(Boolean)
                            .join(" · ")}
                    </Button>

                    {expanded ? (
                        <ProductExpandedDetails
                            product={product}
                            selectedBasisIds={selectedBasisIds}
                            salesUnitPriceGross={pricing.salesUnitPriceGross}
                        />
                    ) : null}
                </div>
            ) : null}
        </li>
    )
}

function ProductExpandedDetails({
    product,
    selectedBasisIds,
    salesUnitPriceGross,
}: {
    product: SourcingProductLine
    selectedBasisIds: ReadonlySet<string>
    salesUnitPriceGross?: string
}) {
    const sortedOptions = React.useMemo(
        () => sortCandidateOptions(product.options),
        [product.options],
    )
    const lowestCost = React.useMemo(
        () => findLowestPurchaseCost(product.options),
        [product.options],
    )
    const unit = product.unitName ?? ""

    return (
        <div className="mt-2 space-y-3 rounded-md border border-border/60 bg-muted/25 px-3 py-2.5">
            {sortedOptions.length > 1 ? (
                <div className="space-y-1.5">
                    <p className="text-tiny font-semibold text-muted-foreground">
                        候选供给对比（共 {sortedOptions.length} 个）
                    </p>
                    <ul className="divide-y divide-border/50">
                        {sortedOptions.map((option) => {
                            const isSelected = selectedBasisIds.has(
                                option.basisId,
                            )
                            const isStock =
                                option.sourceType === "EXISTING_STOCK"
                            const margin = isStock
                                ? null
                                : comparePurchaseCostToSalesPrice(
                                      option.unitCostGross,
                                      salesUnitPriceGross,
                                  )
                            const delta = isStock
                                ? null
                                : priceDeltaFromLowest(
                                      option.unitCostGross,
                                      lowestCost,
                                  )
                            return (
                                <li
                                    key={option.basisId}
                                    className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1 py-1.5 text-xs first:pt-0 last:pb-0"
                                >
                                    <div className="min-w-0 flex-1 space-y-0.5">
                                        <div className="flex flex-wrap items-center gap-1.5">
                                            <FulfillmentRouteBadge
                                                option={option}
                                                label={sourcingRouteLabel(
                                                    option,
                                                )}
                                                size="compact"
                                            />
                                            <span className="truncate font-medium text-foreground">
                                                {optionDisplayName(option)}
                                            </span>
                                            {isSelected ? (
                                                <StatusBadge
                                                    label="当前方案"
                                                    tone="info"
                                                    appearance="quiet"
                                                />
                                            ) : null}
                                            {delta?.isLowest ? (
                                                <Badge
                                                    variant="secondary"
                                                    className="h-4.5 px-1.5 text-tiny"
                                                >
                                                    最低价
                                                </Badge>
                                            ) : null}
                                        </div>
                                        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-tiny text-muted-foreground">
                                            <span className="num">
                                                可供 {option.maxCreateQuantity}
                                                {unit ? ` ${unit}` : ""}
                                            </span>
                                            {!isStock &&
                                            option.paymentTermLabel ? (
                                                <span>
                                                    {option.paymentTermLabel}
                                                </span>
                                            ) : null}
                                            {!isStock &&
                                            option.expectedDeliveryDate ? (
                                                <span className="num">
                                                    预计{" "}
                                                    {
                                                        option.expectedDeliveryDate
                                                    }
                                                </span>
                                            ) : null}
                                        </div>
                                    </div>

                                    <div className="flex shrink-0 items-baseline gap-1.5 text-right">
                                        {isStock ? (
                                            <span className="text-xs font-medium text-info-soft-foreground">
                                                库存核算
                                            </span>
                                        ) : (
                                            <>
                                                <span
                                                    className={cn(
                                                        "text-xs font-semibold",
                                                        margin?.inverted &&
                                                            "text-destructive",
                                                    )}
                                                >
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            option.unitCostGross
                                                        }
                                                        inheritColor={
                                                            margin?.inverted
                                                        }
                                                    />
                                                </span>
                                                {margin?.marginRateText ? (
                                                    <span
                                                        className={cn(
                                                            "num text-tiny font-medium",
                                                            margin.inverted
                                                                ? "text-destructive"
                                                                : "text-success-soft-foreground",
                                                        )}
                                                    >
                                                        {margin.marginRateText}
                                                    </span>
                                                ) : null}
                                            </>
                                        )}
                                    </div>
                                </li>
                            )
                        })}
                    </ul>
                </div>
            ) : null}

            {product.deliveryAllocations.length > 0 ? (
                <div
                    className={cn(
                        "space-y-1.5",
                        sortedOptions.length > 1 &&
                            "border-t border-border/60 pt-2.5",
                    )}
                >
                    <p className="text-tiny font-semibold text-muted-foreground">
                        收货地址分配（{product.deliveryAllocations.length} 个）
                    </p>
                    <ul className="divide-y divide-border/50">
                        {product.deliveryAllocations.map((addr, index) => (
                            <li
                                key={`${addr.region_code}:${addr.recipient}:${addr.address}:${index}`}
                                className="flex items-baseline justify-between gap-3 py-1 text-xs first:pt-0 last:pb-0"
                            >
                                <span className="min-w-0 truncate text-muted-foreground">
                                    <span className="font-medium text-foreground">
                                        {addr.recipient}
                                    </span>
                                    {" · "}
                                    {[addr.region_name, addr.address]
                                        .filter(Boolean)
                                        .join(" ")}
                                </span>
                                <QuantityValue
                                    value={addr.quantity}
                                    unit={unit}
                                    className="shrink-0 text-xs"
                                />
                            </li>
                        ))}
                    </ul>
                </div>
            ) : null}
        </div>
    )
}

function EmptySourcingFallback({
    item,
    padClass,
    blocked,
    overdue,
}: {
    item: WorkspaceWorkItem
    padClass: string
    blocked: boolean
    overdue: boolean
}) {
    const sections = (item.summarySections ?? []).filter(
        (section) => section.value && section.value !== "—",
    )
    const briefLines = item.briefLines ?? []

    return (
        <div
            className={cn(
                "min-h-0 flex-1 space-y-4 overflow-y-auto overscroll-contain py-5",
                padClass,
            )}
        >
            {overdue ? (
                <div>
                    <StatusBadge label="已超期" tone="destructive" />
                </div>
            ) : null}

            {blocked && item.actionBlockers.length > 0 ? (
                <Alert variant="warning">
                    <AlertTriangleIcon aria-hidden="true" />
                    <AlertTitle>当前存在办理限制</AlertTitle>
                    <AlertDescription>
                        {item.actionBlockers
                            .map((blocker) => blocker.message)
                            .join("；")}
                    </AlertDescription>
                </Alert>
            ) : null}

            <Alert variant="info">
                <AlertTitle>当前暂无可分配供给</AlertTitle>
                <AlertDescription>
                    该销售单可能已完成覆盖，或尚未维护可用库存与合格供应商供给，可进入完整作业面进一步核对。
                </AlertDescription>
            </Alert>

            <DetailWorkspaceFacts columns={2} ariaLabel="核心事项摘要">
                <DetailWorkspaceFact
                    label="当前责任"
                    value={
                        [item.ownerRoleLabel, item.ownerUserLabel]
                            .filter(Boolean)
                            .join(" · ") || "—"
                    }
                />
                <DetailWorkspaceFact
                    label="进入时间"
                    numeric
                    value={item.enteredAtLabel}
                />
                <DetailWorkspaceFact
                    label="截止时间"
                    numeric
                    value={item.dueAtLabel || "未设截止时间"}
                />
            </DetailWorkspaceFacts>

            {sections.length > 0 ? (
                <div className="space-y-3 border-t border-border/60 pt-4">
                    <h3 className="text-xs font-semibold text-muted-foreground">
                        关键业务信息
                    </h3>
                    <DetailWorkspaceFacts columns={2}>
                        {sections.slice(0, 8).map((section) => (
                            <DetailWorkspaceFact
                                key={`${section.label}:${section.value}`}
                                label={section.label}
                                numeric={section.numeric}
                                value={section.value}
                            />
                        ))}
                    </DetailWorkspaceFacts>
                </div>
            ) : null}

            {briefLines.length > 0 ? (
                <div className="space-y-2.5 border-t border-border/60 pt-4">
                    <h3 className="text-xs font-semibold text-muted-foreground">
                        销售明细摘要
                    </h3>
                    <ul className="divide-y divide-border/60">
                        {briefLines.slice(0, 5).map((line, index) => (
                            <li
                                key={`${line.title}:${index}`}
                                className="flex items-baseline justify-between gap-3 py-2 text-body-compact first:pt-0 last:pb-0"
                            >
                                <span className="min-w-0 truncate text-foreground">
                                    {line.title}
                                </span>
                                {line.quantity ? (
                                    <span className="num shrink-0 text-xs text-muted-foreground">
                                        {line.quantity}
                                    </span>
                                ) : null}
                            </li>
                        ))}
                    </ul>
                    {(item.briefMoreCount ?? 0) > 0 ? (
                        <p className="num text-xs text-muted-foreground">
                            另有 {item.briefMoreCount}{" "}
                            行明细，请在完整作业面查看与编辑。
                        </p>
                    ) : null}
                </div>
            ) : null}

            {item.nextActionHint || item.impactSummary ? (
                <div className="space-y-1.5 border-t border-border/60 pt-4 text-xs text-muted-foreground">
                    {item.nextActionHint ? <p>{item.nextActionHint}</p> : null}
                    {item.impactSummary ? <p>{item.impactSummary}</p> : null}
                </div>
            ) : null}
        </div>
    )
}
