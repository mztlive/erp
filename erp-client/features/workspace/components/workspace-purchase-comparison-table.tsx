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
import { countLineDeliveryPoints } from "@/features/approval-workflow/purchase-fulfillment"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    APPROVAL_TABLE_DEFAULT_PAGE_SIZE,
    ApprovalLineFilterBar,
    ApprovalLinePagination,
} from "./workspace-approval-table-controls"
import { PurchaseProfitValue } from "./workspace-purchase-amount-details"
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
    uniqueDeliveryPointCount = 0,
    prefix,
    riskOnly: controlledRiskOnly,
    onRiskOnlyChange,
    highlightedLineId,
    tableHandleRef,
    onPreview,
    onCompareSource,
}: {
    lines: ApprovalPurchaseLine[]
    evaluatedRows?: readonly EvaluatedPurchaseRow[]
    metrics?: PurchaseApprovalMetrics
    sources: PurchaseSource[]
    uniqueDeliveryPointCount?: number
    prefix: string
    riskOnly?: boolean
    onRiskOnlyChange?: (next: boolean) => void
    highlightedLineId?: string | null
    tableHandleRef?: RefObject<PurchaseComparisonTableHandle | null>
    onPreview: (revisionId: string) => void
    onCompareSource?: (target: PurchaseSourceComparisonTarget) => void
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
                return [
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
                data-inset-cells
                role="region"
                aria-label="采购与销售明细对照"
                className={detailWorkspaceStyles.inlineTableWrapper}
            >
                <Table data-density="comfortable" aria-label="采购明细">
                    <TableCaption className="sr-only">
                        金额含税。采购毛利按供给金额减采购成本计算，公司对外毛利按本次采购对应的销售单成交金额减采购成本计算。
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
                                className="w-20"
                            >
                                采购数量
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-28"
                            >
                                实际采购成本
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-24"
                            >
                                采购供给单价
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-28"
                            >
                                采购毛利
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-28"
                            >
                                销售单成交单价
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-32"
                            >
                                公司对外毛利
                            </TableHead>
                            <TableHead scope="col" className="w-24">
                                <span className="sr-only">查看价格明细</span>
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {visible.map(({ line, comparison, externalLoss }) => {
                            const lineDeliveryPointCount =
                                uniqueDeliveryPointCount > 1
                                    ? countLineDeliveryPoints(
                                          line.delivery_allocations,
                                      )
                                    : 0
                            const lineSegment = toAutomationIdSegment(line.id)
                            return (
                                <PurchaseComparisonRow
                                    key={line.id}
                                    rowId={`${prefix}-row-${lineSegment}`}
                                    highlighted={highlightedLineId === line.id}
                                    line={line}
                                    comparison={comparison}
                                    externalLoss={externalLoss}
                                    source={
                                        sources.length > 1
                                            ? sources.find(
                                                  (source) =>
                                                      source.revision_id ===
                                                      line.source?.revision_id,
                                              )
                                            : undefined
                                    }
                                    deliveryPointCount={
                                        lineDeliveryPointCount > 0
                                            ? lineDeliveryPointCount
                                            : undefined
                                    }
                                    prefix={`${prefix}-line-${lineSegment}`}
                                    onPreview={onPreview}
                                    onCompareSource={onCompareSource}
                                />
                            )
                        })}
                        {!visible.length ? (
                            <TableRow>
                                <TableCell
                                    colSpan={9}
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
                                    className="py-2.5 align-top text-xs font-medium text-muted-foreground"
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
                                    className="py-2.5 align-top text-xs font-medium text-foreground"
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
                                    className="py-2.5 align-top font-semibold"
                                >
                                    <MoneyValue
                                        display="exact"
                                        value={metrics.grossTotal}
                                    />
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="py-2.5 align-top text-muted-foreground"
                                >
                                    —
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="py-2.5 align-top"
                                >
                                    <PurchaseProfitValue
                                        amount={
                                            metrics.purchaseMargin?.margin ??
                                            null
                                        }
                                    />
                                    {metrics.purchaseMargin &&
                                    (metrics.purchaseMargin.lineCount <
                                        metrics.goodsLineCount ||
                                        metrics.feeLineCount > 0) ? (
                                        <div className="num mt-0.5 text-xs font-normal text-muted-foreground">
                                            {metrics.purchaseMargin.lineCount <
                                            metrics.goodsLineCount
                                                ? `已核算 ${metrics.purchaseMargin.lineCount}/${metrics.goodsLineCount} 行`
                                                : "商品合计"}
                                            {metrics.feeLineCount > 0
                                                ? " · 费用单列"
                                                : null}
                                        </div>
                                    ) : null}
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="py-2.5 align-top text-muted-foreground"
                                >
                                    —
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="py-2.5 align-top"
                                >
                                    <PurchaseProfitValue
                                        amount={metrics.externalGrossMargin}
                                    />
                                    {metrics.externalMargin ? (
                                        <div className="num mt-0.5 text-xs font-normal text-muted-foreground">
                                            {metrics.externalMargin.percent}%
                                        </div>
                                    ) : null}
                                    {metrics.partialSalesCoverage ? (
                                        <div className="num text-xs font-normal text-muted-foreground">
                                            已核算 {metrics.salesCoveredCount}/
                                            {metrics.goodsLineCount} 行
                                        </div>
                                    ) : null}
                                </TableCell>
                                <TableCell />
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
