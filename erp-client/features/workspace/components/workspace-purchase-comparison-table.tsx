"use client"

import { useMemo, useState } from "react"
import {
    ChevronLeftIcon,
    ChevronRightIcon,
    SearchIcon,
    XIcon,
} from "lucide-react"

import {
    MoneyValue,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
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
import { compactFixed, compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import {
    isPurchaseCostAbove,
    purchaseComparison,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"
import { PurchaseComparisonRow } from "./workspace-purchase-comparison-row"

/** 搜索、提示计数和分页均基于本次审批返回的全部冻结明细。 */
export function WorkspacePurchaseComparisonTable({
    lines,
    sources,
    deliverySummary,
    prefix,
    onPreview,
    onFocusDelivery,
}: {
    lines: ApprovalPurchaseLine[]
    sources: PurchaseSource[]
    deliverySummary: DeliveryAllocationSummary
    prefix: string
    onPreview: (revisionId: string) => void
    onFocusDelivery: (lineId: string) => void
}) {
    const [search, setSearch] = useState("")
    const [attentionOnly, setAttentionOnly] = useState(false)
    const [page, setPage] = useState(1)
    const [pageSize, setPageSize] = useState(20)

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
        () =>
            lines.map((line) => ({
                line,
                comparison: purchaseComparison(line),
                negativeMargin: isPurchaseCostAbove(
                    line,
                    line.source?.unit_price_gross,
                ),
            })),
        [lines],
    )
    const attentionCount = rows.filter(
        ({ comparison, negativeMargin }) =>
            comparison.warnings.length > 0 || negativeMargin,
    ).length
    const hasNegativeMargin = rows.some(({ negativeMargin }) => negativeMargin)

    const needle = search.trim().toLocaleLowerCase()
    const filtered = rows.filter(({ line, comparison, negativeMargin }) => {
        if (
            attentionOnly &&
            !(comparison.warnings.length > 0 || negativeMargin)
        )
            return false
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
    })

    const pageCount = Math.max(1, Math.ceil(filtered.length / pageSize))
    const currentPage = Math.min(page, pageCount)
    const visible = filtered.slice(
        (currentPage - 1) * pageSize,
        currentPage * pageSize,
    )
    const firstPage = Math.max(1, Math.min(currentPage - 2, pageCount - 4))
    const pageNumbers = Array.from(
        { length: Math.min(5, pageCount) },
        (_, index) => firstPage + index,
    )
    const clearFilters = () => {
        setSearch("")
        setAttentionOnly(false)
        setPage(1)
    }
    const showFilterBar =
        lines.length > 8 ||
        attentionCount > 0 ||
        Boolean(search) ||
        attentionOnly
    const showPagination = filtered.length > 20 || pageSize !== 20

    const footerTotals = useMemo(() => {
        const grossAmounts: string[] = []
        const netAmounts: string[] = []
        const overSupplyTotals: string[] = []
        const units = new Set<string>()
        const quantities: string[] = []
        let allHaveNet = true
        let quantityLineCount = 0

        for (const { line, comparison } of rows) {
            if (line.gross_amount != null) grossAmounts.push(line.gross_amount)
            if (line.net_amount != null) netAmounts.push(line.net_amount)
            else allHaveNet = false
            if (comparison.priceDifferenceTotal) {
                overSupplyTotals.push(comparison.priceDifferenceTotal)
            }
            if (line.quantity != null) {
                quantityLineCount += 1
                quantities.push(line.quantity)
                units.add(line.unitName ?? line.unit ?? "")
            }
        }

        const grossTotal = grossAmounts.length
            ? sumFixed(grossAmounts, { maxScale: 2, outputScale: 2 })
            : null
        const netTotal =
            allHaveNet && netAmounts.length
                ? sumFixed(netAmounts, { maxScale: 2, outputScale: 2 })
                : null
        const overSupplyTotal = overSupplyTotals.length
            ? sumFixed(overSupplyTotals, { maxScale: 2, outputScale: 2 })
            : null
        const totalQuantity =
            units.size === 1 &&
            quantityLineCount > 0 &&
            quantityLineCount === rows.length
                ? {
                      value: compactFixed(
                          sumFixed(quantities, { maxScale: 6, outputScale: 6 }),
                      ),
                      unit: [...units][0] || undefined,
                  }
                : null

        return {
            grossTotal,
            netTotal,
            overSupplyTotal:
                overSupplyTotal && compareDecimal(overSupplyTotal, "0", 2) > 0
                    ? overSupplyTotal
                    : null,
            totalQuantity,
        }
    }, [rows])

    return (
        <TableToolbarScope>
            <div className="space-y-3">
                {showFilterBar ? (
                    <div
                        className="flex flex-wrap items-center justify-between gap-2.5"
                        aria-label="商品对照筛选"
                    >
                        <div
                            role="group"
                            aria-label="异常筛选"
                            className={detailWorkspaceStyles.subTabs}
                        >
                            <button
                                id={`${prefix}-filter-all`}
                                type="button"
                                aria-pressed={!attentionOnly}
                                data-active={
                                    !attentionOnly ? "true" : undefined
                                }
                                className={detailWorkspaceStyles.subTab}
                                onClick={() => {
                                    setAttentionOnly(false)
                                    setPage(1)
                                }}
                            >
                                <span>全部</span>
                                <span className="num text-xs text-muted-foreground">
                                    {lines.length}
                                </span>
                            </button>
                            <button
                                id={`${prefix}-filter-attention`}
                                type="button"
                                aria-pressed={attentionOnly}
                                data-active={attentionOnly ? "true" : undefined}
                                className={detailWorkspaceStyles.subTab}
                                onClick={() => {
                                    setAttentionOnly(true)
                                    setPage(1)
                                }}
                            >
                                <span>仅看异常项</span>
                                <Badge
                                    variant={
                                        hasNegativeMargin
                                            ? "destructive"
                                            : attentionCount > 0
                                              ? "warning"
                                              : "neutral"
                                    }
                                    className="rounded-full px-1.5 py-0 text-tiny tabular-nums"
                                >
                                    {attentionCount}
                                </Badge>
                            </button>
                        </div>
                        <div className="relative w-full sm:w-60">
                            <SearchIcon
                                className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                aria-hidden="true"
                            />
                            <Input
                                id={`${prefix}-search`}
                                value={search}
                                onChange={(event) => {
                                    setSearch(event.target.value)
                                    setPage(1)
                                }}
                                placeholder="搜索商品、规格或收货人"
                                aria-label="搜索商品、规格或收货人"
                                className="h-control-sm pr-8 pl-8 text-xs"
                            />
                            {search ? (
                                <Button
                                    id={`${prefix}-search-clear`}
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label="清除搜索"
                                    className="absolute top-1/2 right-0.5 -translate-y-1/2"
                                    onClick={() => {
                                        setSearch("")
                                        setPage(1)
                                    }}
                                >
                                    <XIcon aria-hidden="true" />
                                </Button>
                            ) : null}
                        </div>
                    </div>
                ) : null}

                {search || attentionOnly || showPagination ? (
                    <TableToolbar
                        actions={
                            search || attentionOnly ? (
                                <Button
                                    id={`${prefix}-clear-filters`}
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.quietButton
                                    }
                                    onClick={clearFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : undefined
                        }
                    >
                        <span
                            className="text-xs text-muted-foreground"
                            role="status"
                            aria-live="polite"
                        >
                            {search || attentionOnly
                                ? `筛选结果 ${filtered.length} 行`
                                : `共 ${filtered.length} 行`}
                            {showPagination
                                ? ` · 当前显示 ${visible.length ? (currentPage - 1) * pageSize + 1 : 0}–${Math.min(currentPage * pageSize, filtered.length)} 行`
                                : ""}
                        </span>
                    </TableToolbar>
                ) : null}

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
                                <TableHead scope="col" className="min-w-44">
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
                            {visible.map(
                                ({ line, comparison, negativeMargin }) => {
                                    const deliveryHint = formatLineDeliveryHint(
                                        deliveryItemById.get(line.id),
                                        deliverySummary,
                                    )
                                    return (
                                        <PurchaseComparisonRow
                                            key={line.id}
                                            line={line}
                                            comparison={comparison}
                                            negativeMargin={negativeMargin}
                                            source={
                                                sources.length > 1
                                                    ? sources.find(
                                                          (source) =>
                                                              source.revision_id ===
                                                              line.source
                                                                  ?.revision_id,
                                                      )
                                                    : undefined
                                            }
                                            deliveryHint={deliveryHint}
                                            prefix={`${prefix}-line-${toAutomationIdSegment(line.id)}`}
                                            onPreview={onPreview}
                                            onFocusDelivery={() =>
                                                onFocusDelivery(line.id)
                                            }
                                        />
                                    )
                                },
                            )}
                            {!visible.length ? (
                                <TableRow>
                                    <TableCell
                                        colSpan={6}
                                        className="py-10 text-center text-xs text-muted-foreground"
                                    >
                                        {attentionOnly && !search
                                            ? "当前没有异常项，仍请核对完整采购明细。"
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
                                        {footerTotals.totalQuantity ? (
                                            <QuantityValue
                                                value={
                                                    footerTotals.totalQuantity
                                                        .value
                                                }
                                                unit={
                                                    footerTotals.totalQuantity
                                                        .unit
                                                }
                                            />
                                        ) : (
                                            "—"
                                        )}
                                    </TableCell>
                                    <TableCell className="py-2.5" />
                                    <TableCell
                                        data-align="end"
                                        className="py-2.5 align-middle text-xs"
                                    >
                                        {footerTotals.overSupplyTotal ? (
                                            <span className="font-medium text-warning-soft-foreground">
                                                超预估合计 +
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        footerTotals.overSupplyTotal
                                                    }
                                                    className="[&_[data-slot=money-value-number]]:text-inherit"
                                                />
                                            </span>
                                        ) : null}
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="py-2.5 align-middle"
                                    >
                                        {footerTotals.grossTotal ? (
                                            <div className="font-semibold text-foreground">
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        footerTotals.grossTotal
                                                    }
                                                />
                                            </div>
                                        ) : (
                                            "—"
                                        )}
                                        {footerTotals.netTotal ? (
                                            <div className="mt-0.5 text-xs font-normal text-muted-foreground">
                                                不含税{" "}
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        footerTotals.netTotal
                                                    }
                                                />
                                            </div>
                                        ) : null}
                                    </TableCell>
                                </TableRow>
                            </TableFooter>
                        ) : null}
                    </Table>
                </div>

                {showPagination ? (
                    <div className="flex flex-wrap items-center justify-between gap-3 pt-1">
                        <div className="flex items-center gap-2 text-xs text-muted-foreground">
                            <span id={`${prefix}-page-size-label`}>每页</span>
                            <Select
                                value={String(pageSize)}
                                onValueChange={(next) => {
                                    if (!next) return
                                    setPageSize(next === "50" ? 50 : 20)
                                    setPage(1)
                                }}
                            >
                                <SelectTrigger
                                    id={`${prefix}-page-size`}
                                    size="sm"
                                    aria-labelledby={`${prefix}-page-size-label`}
                                    className="w-24"
                                >
                                    <SelectValue>{pageSize} 项</SelectValue>
                                </SelectTrigger>
                                <SelectContent
                                    align="start"
                                    alignItemWithTrigger={false}
                                >
                                    <SelectItem
                                        id={`${prefix}-page-size-20`}
                                        value="20"
                                    >
                                        20 项
                                    </SelectItem>
                                    <SelectItem
                                        id={`${prefix}-page-size-50`}
                                        value="50"
                                    >
                                        50 项
                                    </SelectItem>
                                </SelectContent>
                            </Select>
                            <span className="num">
                                第 {currentPage} / {pageCount} 页
                            </span>
                        </div>
                        <nav
                            aria-label="采购明细分页"
                            className="flex items-center gap-1"
                        >
                            <Button
                                id={`${prefix}-page-previous`}
                                variant="outline"
                                size="icon-sm"
                                aria-label="上一页商品"
                                disabled={currentPage === 1}
                                onClick={() => setPage(currentPage - 1)}
                            >
                                <ChevronLeftIcon aria-hidden="true" />
                            </Button>
                            {pageNumbers.map((value) => (
                                <Button
                                    id={`${prefix}-page-${value}`}
                                    key={value}
                                    variant="ghost"
                                    size="icon-sm"
                                    aria-label={`第 ${value} 页商品`}
                                    aria-current={
                                        value === currentPage
                                            ? "page"
                                            : undefined
                                    }
                                    className={cn(
                                        "num",
                                        value === currentPage &&
                                            "border-info-border bg-info-soft text-info-soft-foreground",
                                    )}
                                    onClick={() => setPage(value)}
                                >
                                    {value}
                                </Button>
                            ))}
                            <Button
                                id={`${prefix}-page-next`}
                                variant="outline"
                                size="icon-sm"
                                aria-label="下一页商品"
                                disabled={currentPage === pageCount}
                                onClick={() => setPage(currentPage + 1)}
                            >
                                <ChevronRightIcon aria-hidden="true" />
                            </Button>
                        </nav>
                    </div>
                ) : null}
            </div>
        </TableToolbarScope>
    )
}
