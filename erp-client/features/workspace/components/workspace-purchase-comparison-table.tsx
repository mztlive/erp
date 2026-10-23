"use client"

import { useMemo, useRef, useState } from "react"
import { ChevronLeftIcon, ChevronRightIcon, SearchIcon } from "lucide-react"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
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
import { cn } from "@/lib/utils"
import {
    purchaseComparison,
    purchaseSelectedClass,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"
import { PurchaseComparisonRow } from "./workspace-purchase-comparison-row"

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
    const [expanded, setExpanded] = useState<string | null>(null)
    const scrollRef = useRef<HTMLDivElement>(null)
    const changePage = (value: number) => {
        setPage(value)
        if (scrollRef.current) scrollRef.current.scrollTop = 0
    }
    const rows = useMemo(
        () =>
            lines.map((line) => ({
                line,
                comparison: purchaseComparison(line),
            })),
        [lines],
    )
    const attentionCount = rows.filter(
        ({ comparison }) => comparison.warnings.length > 0,
    ).length
    const needle = search.trim().toLocaleLowerCase()
    const filtered = rows.filter(
        ({ line, comparison }) =>
            (!attentionOnly || comparison.warnings.length > 0) &&
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
        changePage(1)
    }
    return (
        <TableToolbarScope>
            <div className="space-y-2">
                <div
                    className="flex flex-wrap items-center justify-between gap-3"
                    aria-label="商品对照筛选"
                >
                    <div
                        className="inline-flex rounded-lg border border-border/70 bg-muted/30 p-0.5"
                        role="group"
                        aria-label="核对范围"
                    >
                        <Button
                            id={`${prefix}-filter-all`}
                            variant="ghost"
                            size="sm"
                            aria-pressed={!attentionOnly}
                            className={cn(
                                !attentionOnly && purchaseSelectedClass,
                            )}
                            onClick={() => {
                                setAttentionOnly(false)
                                changePage(1)
                            }}
                        >
                            全部 {lines.length}
                        </Button>
                        <Button
                            id={`${prefix}-filter-attention`}
                            variant="ghost"
                            size="sm"
                            aria-pressed={attentionOnly}
                            className={cn(
                                attentionOnly && purchaseSelectedClass,
                            )}
                            onClick={() => {
                                setAttentionOnly(true)
                                changePage(1)
                            }}
                        >
                            需关注{" "}
                            <span className="ml-1 rounded-full bg-warning-border/60 px-1.5 text-xs text-warning-soft-foreground">
                                {attentionCount}
                            </span>
                        </Button>
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
                                changePage(1)
                            }}
                            placeholder="搜索商品名称 / 规格"
                            aria-label="搜索商品名称或规格"
                            className="h-9 pl-9"
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
                            : ""}{" "}
                        · 当前显示{" "}
                        {visible.length ? (currentPage - 1) * pageSize + 1 : 0}–
                        {Math.min(currentPage * pageSize, filtered.length)} 项
                    </span>
                </TableToolbar>
                <div
                    id={`${prefix}-table-scroll`}
                    ref={scrollRef}
                    role="region"
                    aria-label="采购与销售明细，可滚动查看"
                    className="max-h-[36rem] overflow-auto rounded-lg border border-grid outline-none focus-visible:ring-2 focus-visible:ring-info [&>[data-slot=table-container]]:overflow-visible"
                >
                    <Table className="min-w-[1120px] text-sm">
                        <TableCaption className="sr-only">
                            本次提交的采购与来源销售逐行对照；筛选及分页不改变整单审批范围。
                        </TableCaption>
                        <TableHeader className="sticky top-0 z-10">
                            <TableRow>
                                <TableHead scope="col" className="w-10">
                                    <span className="sr-only">行号</span>
                                </TableHead>
                                <TableHead scope="col" className="min-w-44">
                                    商品 / 规格
                                </TableHead>
                                <TableHead scope="col">
                                    数量（销售 / 采购）
                                </TableHead>
                                <TableHead scope="col">销售含税单价</TableHead>
                                <TableHead scope="col">采购供给价</TableHead>
                                <TableHead scope="col">本次采购单价</TableHead>
                                <TableHead scope="col">
                                    交期（销售 / 采购）
                                </TableHead>
                                <TableHead scope="col" className="min-w-36">
                                    核对提示
                                </TableHead>
                                <TableHead scope="col" className="w-12">
                                    <span className="sr-only">展开对照</span>
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {visible.map(({ line, comparison }) => (
                                <PurchaseComparisonRow
                                    key={line.id}
                                    line={line}
                                    comparison={comparison}
                                    source={sources.find(
                                        (source) =>
                                            source.revision_id ===
                                            line.source?.revision_id,
                                    )}
                                    prefix={`${prefix}-line-${toAutomationIdSegment(line.id)}`}
                                    expanded={expanded === line.id}
                                    onToggle={() =>
                                        setExpanded(
                                            expanded === line.id
                                                ? null
                                                : line.id,
                                        )
                                    }
                                    onPreview={onPreview}
                                />
                            ))}
                            {!visible.length && (
                                <TableRow>
                                    <TableCell
                                        colSpan={9}
                                        className="py-12 text-center text-muted-foreground"
                                    >
                                        {attentionOnly && !search
                                            ? "当前没有需关注的商品，仍请核对完整采购明细。"
                                            : "没有匹配的商品，请调整搜索条件或清除筛选。"}
                                    </TableCell>
                                </TableRow>
                            )}
                        </TableBody>
                    </Table>
                </div>
                <p className="text-xs leading-5 text-muted-foreground">
                    供给价为销售开单时填写的预计采购含税单价；销售数量为来源行全部需求，本次采购仅覆盖其中的采购数量。部分数量采购不直接视为异常。销售日期为客户承诺交期，采购日期为预计交付日。
                </p>
                <div className="flex flex-wrap items-center justify-between gap-3 pt-1">
                    <div className="flex items-center gap-2 text-xs text-muted-foreground">
                        <label htmlFor={`${prefix}-page-size`}>每页</label>
                        <NativeSelect
                            id={`${prefix}-page-size`}
                            size="sm"
                            value={pageSize}
                            onChange={(event) => {
                                setPageSize(
                                    event.target.value === "50" ? 50 : 20,
                                )
                                changePage(1)
                            }}
                        >
                            <NativeSelectOption
                                id={`${prefix}-page-size-20`}
                                value="20"
                            >
                                20 项
                            </NativeSelectOption>
                            <NativeSelectOption
                                id={`${prefix}-page-size-50`}
                                value="50"
                            >
                                50 项
                            </NativeSelectOption>
                        </NativeSelect>
                        <span>
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
                            onClick={() => changePage(currentPage - 1)}
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
                                    value === currentPage ? "page" : undefined
                                }
                                className={cn(
                                    value === currentPage &&
                                        "border-info-border bg-info-soft text-info-soft-foreground",
                                )}
                                onClick={() => changePage(value)}
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
                            onClick={() => changePage(currentPage + 1)}
                        >
                            <ChevronRightIcon aria-hidden="true" />
                        </Button>
                    </nav>
                </div>
            </div>
        </TableToolbarScope>
    )
}
