"use client"

import { useImperativeHandle, useMemo, useState, type RefObject } from "react"

import {
    MoneyValue,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import {
    Table,
    TableBody,
    TableCaption,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import {
    formatLineDeliveryHint,
    type DeliveryAllocationSummary,
} from "@/lib/delivery-allocation-summary"
import { cn } from "@/lib/utils"
import {
    APPROVAL_TABLE_DEFAULT_PAGE_SIZE,
    ApprovalLineFilterBar,
    ApprovalLinePagination,
} from "./workspace-approval-table-controls"
import {
    buildPurchaseApprovalMetrics,
    evaluatePurchaseLines,
    type EvaluatedPurchaseRow,
    type PurchaseApprovalMetrics,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"
import { PurchaseComparisonRow } from "./workspace-purchase-comparison-row"
import type { PurchaseSourceComparisonTarget } from "./workspace-purchase-source-comparison"

export interface PurchaseComparisonTableHandle {
    revealLine: (lineId: string) => void
    filterToRisks: () => void
}

/** 搜索、提示计数和分页均基于本次审批返回的全部冻结明细。 */
export function WorkspacePurchaseComparisonTable({
    lines,
    evaluatedRows: externalRows,
    metrics: externalMetrics,
    sources,
    deliverySummary,
    prefix,
    riskOnly: controlledRiskOnly,
    onRiskOnlyChange,
    highlightedLineId,
    tableHandleRef,
    onPreview,
    onCompareSource,
    onFocusDelivery,
}: {
    lines: ApprovalPurchaseLine[]
    evaluatedRows?: readonly EvaluatedPurchaseRow[]
    metrics?: PurchaseApprovalMetrics
    sources: PurchaseSource[]
    deliverySummary: DeliveryAllocationSummary
    prefix: string
    riskOnly?: boolean
    onRiskOnlyChange?: (next: boolean) => void
    highlightedLineId?: string | null
    tableHandleRef?: RefObject<PurchaseComparisonTableHandle | null>
    onPreview: (revisionId: string) => void
    onCompareSource?: (target: PurchaseSourceComparisonTarget) => void
    onFocusDelivery: (lineId: string) => void
}) {
    const [search, setSearch] = useState("")
    const [internalRiskOnly, setInternalRiskOnly] = useState(false)
    const [page, setPage] = useState(1)
    const [pageSize, setPageSize] = useState(APPROVAL_TABLE_DEFAULT_PAGE_SIZE)

    const riskOnly = controlledRiskOnly ?? internalRiskOnly
    const setRiskOnly = (next: boolean) => {
        if (controlledRiskOnly === undefined) {
            setInternalRiskOnly(next)
        }
        onRiskOnlyChange?.(next)
    }

    const deliveryItemById = useMemo(
        () =>
            new Map(
                deliverySummary.itemGroups.map((group) => [
                    group.lineId,
                    group,
                ]),
            ),
        [deliverySummary.itemGroups],
    )

    const rows = useMemo(
        () => externalRows ?? evaluatePurchaseLines(lines),
        [externalRows, lines],
    )
    const metrics = useMemo(
        () => externalMetrics ?? buildPurchaseApprovalMetrics(rows, []),
        [externalMetrics, rows],
    )

    const needle = search.trim().toLocaleLowerCase()
    const filtered = useMemo(
        () =>
            rows.filter(({ line, hasActionableRisk }) => {
                if (riskOnly && !hasActionableRisk) return false
                if (!needle) return true
                const textMatched = [
                    String(line.line_no),
                    line.title,
                    line.specification,
                    line.source?.title,
                    line.source?.specification,
                ]
                    .filter(Boolean)
                    .join(" ")
                    .toLocaleLowerCase()
                    .includes(needle)
                if (textMatched) return true
                return (line.delivery_allocations ?? []).some((row) =>
                    `${row.recipient} ${row.phone} ${fullDeliveryAddress(row)}`
                        .toLocaleLowerCase()
                        .includes(needle),
                )
            }),
        [needle, riskOnly, rows],
    )

    const pageCount = Math.max(1, Math.ceil(filtered.length / pageSize))
    const currentPage = Math.min(page, pageCount)
    const visible = filtered.slice(
        (currentPage - 1) * pageSize,
        currentPage * pageSize,
    )

    const clearFilters = () => {
        setSearch("")
        setRiskOnly(false)
        setPage(1)
    }

    useImperativeHandle(tableHandleRef, () => ({
        revealLine: (lineId: string) => {
            const index = filtered.findIndex((row) => row.line.id === lineId)
            if (index >= 0) {
                const targetPage = Math.floor(index / pageSize) + 1
                if (targetPage !== currentPage) setPage(targetPage)
                return
            }
            setSearch("")
            setRiskOnly(false)
            const rawIndex = rows.findIndex((row) => row.line.id === lineId)
            if (rawIndex >= 0) {
                setPage(Math.floor(rawIndex / pageSize) + 1)
            }
        },
        filterToRisks: () => {
            setSearch("")
            setRiskOnly(true)
            setPage(1)
        },
    }))

    return (
        <div className="space-y-3">
            <ApprovalLineFilterBar
                idPrefix={prefix}
                totalCount={lines.length}
                filteredCount={filtered.length}
                riskCount={metrics.actionableRiskLineCount}
                hasHighRisk={metrics.highRiskLineCount > 0}
                riskOnly={riskOnly}
                onRiskOnlyChange={(next) => {
                    setRiskOnly(next)
                    setPage(1)
                }}
                search={search}
                onSearchChange={(next) => {
                    setSearch(next)
                    setPage(1)
                }}
                onClearFilters={clearFilters}
                ariaLabel="采购明细筛选"
            />

            <div
                id={`${prefix}-table-scroll`}
                data-slot="detail-inline-table"
                role="region"
                aria-label="采购与销售明细对照"
                className={detailWorkspaceStyles.inlineTableWrapper}
            >
                <Table
                    data-density="comfortable"
                    aria-label="采购明细"
                    className="w-full [&_:is(th,td):first-child]:ps-0"
                >
                    <TableCaption className="sr-only">
                        销售数量为来源行全部需求，供给价为销售填写的预计采购含税单价。审批适用于整份采购单。
                    </TableCaption>
                    <TableHeader>
                        <TableRow>
                            <TableHead scope="col" className="w-8">
                                #
                            </TableHead>
                            <TableHead scope="col" className="min-w-40">
                                商品 / 规格与交期
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-24"
                            >
                                采购数量
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-28"
                            >
                                采购含税单价
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-36"
                            >
                                销售对照
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-28"
                            >
                                含税小计
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {visible.map(({ line, comparison, negativeMargin }) => {
                            const deliveryHint = formatLineDeliveryHint(
                                deliveryItemById.get(line.id),
                                deliverySummary,
                            )
                            const lineSegment = toAutomationIdSegment(line.id)
                            return (
                                <PurchaseComparisonRow
                                    key={line.id}
                                    rowId={`${prefix}-row-${lineSegment}`}
                                    highlighted={highlightedLineId === line.id}
                                    line={line}
                                    comparison={comparison}
                                    negativeMargin={negativeMargin}
                                    source={
                                        sources.length > 1
                                            ? sources.find(
                                                  (source) =>
                                                      source.revision_id ===
                                                      line.source?.revision_id,
                                              )
                                            : undefined
                                    }
                                    deliveryHint={deliveryHint}
                                    prefix={`${prefix}-line-${lineSegment}`}
                                    onPreview={onPreview}
                                    onCompareSource={onCompareSource}
                                    onFocusDelivery={() =>
                                        onFocusDelivery(line.id)
                                    }
                                />
                            )
                        })}
                        {!visible.length ? (
                            <TableRow>
                                <TableCell
                                    colSpan={6}
                                    className="py-10 text-center text-xs text-muted-foreground"
                                >
                                    {riskOnly && !search
                                        ? "当前没有风险项，仍请核对完整采购明细。"
                                        : "没有匹配的商品，请调整搜索条件或清除筛选。"}
                                </TableCell>
                            </TableRow>
                        ) : null}
                    </TableBody>
                    {lines.length > 0 ? (
                        <TableFooter>
                            <TableRow className="hover:bg-transparent">
                                <TableCell
                                    colSpan={2}
                                    className="py-2.5 align-middle text-xs font-medium text-muted-foreground"
                                >
                                    合计 · 共{" "}
                                    <span className="num text-foreground">
                                        {lines.length}
                                    </span>{" "}
                                    项
                                    {filtered.length !== lines.length ? (
                                        <span className="num ml-1.5 font-normal">
                                            （当前显示 {filtered.length}{" "}
                                            项，合计按整单）
                                        </span>
                                    ) : null}
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="py-2.5 align-middle text-xs font-medium text-foreground"
                                >
                                    {metrics.totalQuantity ? (
                                        <QuantityValue
                                            value={metrics.totalQuantity.value}
                                            unit={metrics.totalQuantity.unit}
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="py-2.5 align-middle text-xs"
                                >
                                    {metrics.netSupplyVarianceSign === "over" &&
                                    metrics.absNetSupplyVariance ? (
                                        <span className="font-medium text-warning-soft-foreground">
                                            净超预估 +
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    metrics.absNetSupplyVariance
                                                }
                                                inheritColor
                                            />
                                        </span>
                                    ) : metrics.netSupplyVarianceSign ===
                                          "saved" &&
                                      metrics.absNetSupplyVariance ? (
                                        <span className="font-medium text-success-soft-foreground">
                                            低于预估 -
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    metrics.absNetSupplyVariance
                                                }
                                                inheritColor
                                            />
                                        </span>
                                    ) : null}
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="py-2.5 align-middle text-xs"
                                >
                                    {metrics.orderMargin ? (
                                        <>
                                            <div
                                                className={cn(
                                                    "num font-medium",
                                                    metrics.orderMargin.negative
                                                        ? "text-destructive-soft-foreground"
                                                        : "text-foreground/85",
                                                )}
                                            >
                                                {metrics.orderMargin.negative
                                                    ? `整单亏 ${metrics.orderMargin.absPercent}%`
                                                    : `整单毛利 ${metrics.orderMargin.percent}%`}
                                            </div>
                                            <div className="mt-0.5 text-tiny font-normal text-muted-foreground">
                                                {metrics.orderMargin.negative
                                                    ? "亏 "
                                                    : "毛利额 "}
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        metrics.orderMargin
                                                            .absAmount
                                                    }
                                                />
                                            </div>
                                        </>
                                    ) : (
                                        <span className="text-muted-foreground">
                                            —
                                        </span>
                                    )}
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="py-2.5 align-middle"
                                >
                                    {metrics.grossTotal ? (
                                        <div className="font-semibold text-foreground">
                                            <MoneyValue
                                                display="exact"
                                                value={metrics.grossTotal}
                                            />
                                        </div>
                                    ) : (
                                        "—"
                                    )}
                                    {metrics.netTotal ? (
                                        <div className="mt-0.5 text-xs font-normal text-muted-foreground">
                                            不含税{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={metrics.netTotal}
                                            />
                                        </div>
                                    ) : null}
                                </TableCell>
                            </TableRow>
                        </TableFooter>
                    ) : null}
                </Table>
            </div>

            <ApprovalLinePagination
                idPrefix={prefix}
                filteredCount={filtered.length}
                page={currentPage}
                pageSize={pageSize}
                onPageChange={setPage}
                onPageSizeChange={(next) => {
                    setPageSize(next)
                    setPage(1)
                }}
                ariaLabel="采购明细分页"
            />
        </div>
    )
}
