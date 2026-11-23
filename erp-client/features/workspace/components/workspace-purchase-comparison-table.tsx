"use client"

import { useMemo, useState } from "react"
import { ChevronLeftIcon, ChevronRightIcon, SearchIcon } from "lucide-react"
import { detailWorkspaceStyles } from "@/components/business"
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
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import {
    purchaseComparison,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"
import { PurchaseComparisonRow } from "./workspace-purchase-comparison-row"

function hasNegativeMargin(line: ApprovalPurchaseLine): boolean {
    const cost = line.unit_cost_gross
    const salesPrice = line.source?.unit_price_gross
    if (cost == null || cost === "" || salesPrice == null || salesPrice === "")
        return false
    const unitMismatch = Boolean(
        line.unit?.trim() &&
        line.source?.unit?.trim() &&
        line.unit.trim() !== line.source.unit.trim(),
    )
    if (unitMismatch) return false
    try {
        return compareDecimal(cost, salesPrice, 6) > 0
    } catch {
        return false
    }
}

/** 搜索、提示计数和分页均基于本次审批返回的全部冻结明细。 */
export function WorkspacePurchaseComparisonTable({
    lines,
    sources,
    prefix,
    onPreview,
}: {
    lines: ApprovalPurchaseLine[]
    sources: PurchaseSource[]
    prefix: string
    onPreview: (revisionId: string) => void
}) {
    const [search, setSearch] = useState("")
    const [attentionOnly, setAttentionOnly] = useState(false)
    const [page, setPage] = useState(1)
    const [pageSize, setPageSize] = useState(20)
    const [expandedIds, setExpandedIds] = useState<ReadonlySet<string>>(
        () => new Set(),
    )
    const toggleExpanded = (id: string) => {
        setExpandedIds((current) => {
            const next = new Set(current)
            if (next.has(id)) next.delete(id)
            else next.add(id)
            return next
        })
    }
    const rows = useMemo(
        () =>
            lines.map((line) => ({
                line,
                comparison: purchaseComparison(line),
                negativeMargin: hasNegativeMargin(line),
            })),
        [lines],
    )
    const attentionCount = rows.filter(
        ({ comparison, negativeMargin }) =>
            comparison.warnings.length > 0 || negativeMargin,
    ).length
    const needle = search.trim().toLocaleLowerCase()
    const filtered = rows.filter(
        ({ line, comparison, negativeMargin }) =>
            (!attentionOnly ||
                comparison.warnings.length > 0 ||
                negativeMargin) &&
            (!needle ||
                [
                    line.title,
                    line.specification,
                    line.source?.title,
                    line.source?.specification,
                ]
                    .filter(Boolean)
                    .join(" ")
                    .toLocaleLowerCase()
                    .includes(needle)),
    )
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
    const showPagination = filtered.length > 20 || pageSize !== 20

    return (
        <TableToolbarScope>
            <div className="space-y-3">
                <div
                    className="flex flex-wrap items-center justify-between gap-3"
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
                            data-active={!attentionOnly ? "true" : undefined}
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
                                    attentionCount > 0 ? "warning" : "neutral"
                                }
                                className="rounded-full px-1.5 py-0 text-tiny tabular-nums"
                            >
                                {attentionCount}
                            </Badge>
                        </button>
                    </div>
                    <div className="relative w-full sm:w-64">
                        <SearchIcon
                            className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
                            aria-hidden="true"
                        />
                        <Input
                            id={`${prefix}-search`}
                            value={search}
                            onChange={(event) => {
                                setSearch(event.target.value)
                                setPage(1)
                            }}
                            placeholder="搜索商品名称 / 规格"
                            aria-label="搜索商品名称或规格"
                            className="h-control pl-9"
                        />
                    </div>
                </div>
                <TableToolbar
                    actions={
                        search || attentionOnly ? (
                            <Button
                                id={`${prefix}-clear-filters`}
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
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
                        共 {lines.length} 项
                        {search || attentionOnly
                            ? ` · 筛选结果 ${filtered.length} 项`
                            : ""}
                        {showPagination
                            ? ` · 当前显示 ${visible.length ? (currentPage - 1) * pageSize + 1 : 0}–${Math.min(currentPage * pageSize, filtered.length)} 项`
                            : ""}
                        {attentionCount > 0
                            ? " · 点击异常行可展开明细对比"
                            : ""}
                    </span>
                </TableToolbar>
                <div
                    id={`${prefix}-table-scroll`}
                    data-slot="detail-inline-table"
                    role="region"
                    aria-label="采购与销售明细对照"
                    className={detailWorkspaceStyles.inlineTableWrapper}
                >
                    <Table className="w-full [&_:is(th,td):first-child]:ps-0">
                        <TableCaption className="sr-only">
                            本次提交的采购与来源销售逐行对照；筛选及分页不改变整单审批范围。
                        </TableCaption>
                        <TableHeader>
                            <TableRow>
                                <TableHead scope="col" className="w-9">
                                    <span className="sr-only">行号</span>
                                </TableHead>
                                <TableHead scope="col">商品 / 规格</TableHead>
                                <TableHead scope="col" data-align="end">
                                    数量（销 / 采）
                                </TableHead>
                                <TableHead scope="col">售价 / 供给价</TableHead>
                                <TableHead scope="col" data-align="end">
                                    本次采购单价
                                </TableHead>
                                <TableHead scope="col">
                                    交期（销 / 采）
                                </TableHead>
                                <TableHead scope="col">核对提示</TableHead>
                                <TableHead scope="col" className="w-9">
                                    <span className="sr-only">补充信息</span>
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {visible.map(
                                ({ line, comparison, negativeMargin }) => (
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
                                        prefix={`${prefix}-line-${toAutomationIdSegment(line.id)}`}
                                        expanded={expandedIds.has(line.id)}
                                        onToggle={() => toggleExpanded(line.id)}
                                        onPreview={onPreview}
                                    />
                                ),
                            )}
                            {!visible.length && (
                                <TableRow>
                                    <TableCell
                                        colSpan={8}
                                        className="py-12 text-center text-muted-foreground"
                                    >
                                        {attentionOnly && !search
                                            ? "当前没有异常项，仍请核对完整采购明细。"
                                            : "没有匹配的商品，请调整搜索条件或清除筛选。"}
                                    </TableCell>
                                </TableRow>
                            )}
                        </TableBody>
                    </Table>
                </div>
                <p className="text-xs text-muted-foreground">
                    供给价为销售开单时填写的预计采购含税单价；销售数量为来源行全部需求，本次采购仅覆盖其中的采购数量。部分数量采购不直接视为异常。销售日期为客户承诺交期，采购日期为预计交付日。
                </p>
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
