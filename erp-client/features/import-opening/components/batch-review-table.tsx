import { Fragment, useRef } from "react"
import { SearchIcon, XIcon } from "lucide-react"

import { TableToolbar } from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

import { CONFIRMATION_SCOPE_LABEL } from "@/features/import-opening/types"
import type { ReviewRow } from "./batch-review-data"

export const reviewPageSize = 20

const resultMeta: Record<
    ReviewRow["result"],
    { label: string; tone: StatusTone }
> = {
    valid: { label: "通过", tone: "success" },
    warning: { label: "警示", tone: "warning" },
    invalid: { label: "无效", tone: "destructive" },
}

export function batchReviewRowId(prefix: string, rowId: string) {
    return `${prefix}-row-${toAutomationIdSegment(rowId)}`
}

export function BatchReviewTable({
    idPrefix,
    total,
    anomalyCount,
    rows,
    page,
    filteredCount,
    onlyAnomalies,
    search,
    highlightedRowId,
    onFilter,
    onSearch,
    onPage,
}: {
    idPrefix: string
    total: number
    anomalyCount: number
    rows: readonly ReviewRow[]
    page: number
    filteredCount: number
    onlyAnomalies: boolean
    search: string
    highlightedRowId?: string
    onFilter: (onlyAnomalies: boolean) => void
    onSearch: (search: string) => void
    onPage: (page: number) => void
}) {
    const searchInputRef = useRef<HTMLInputElement>(null)
    const filtered = onlyAnomalies || Boolean(search.trim())
    const pageCount = Math.max(1, Math.ceil(filteredCount / reviewPageSize))
    return (
        <section
            className="min-w-0 space-y-2.5"
            aria-labelledby={`${idPrefix}-rows-title`}
        >
            <h2
                id={`${idPrefix}-rows-title`}
                className="text-section-title font-semibold"
            >
                批次核对明细
            </h2>
            {total > 8 || anomalyCount > 0 || filtered ? (
                <TableToolbar className="[&>[data-slot=table-toolbar-summary]]:w-full">
                    <div className="flex w-full min-w-0 flex-wrap items-center gap-1.5">
                        <div
                            className="flex shrink-0 items-center gap-0.5"
                            role="group"
                            aria-label="校验状态"
                        >
                            <Button
                                id={`${idPrefix}-filter-all`}
                                type="button"
                                size="xs"
                                variant={onlyAnomalies ? "ghost" : "secondary"}
                                aria-pressed={!onlyAnomalies}
                                onClick={() => onFilter(false)}
                            >
                                全部 <span className="num">{total}</span>
                            </Button>
                            <Button
                                id={`${idPrefix}-filter-anomalies`}
                                type="button"
                                size="xs"
                                variant={onlyAnomalies ? "secondary" : "ghost"}
                                aria-pressed={onlyAnomalies}
                                onClick={() => onFilter(true)}
                            >
                                仅看异常{" "}
                                <span className="num">{anomalyCount}</span>
                            </Button>
                        </div>
                        <div className="relative flex min-w-0 flex-1 max-w-64 items-center">
                            <SearchIcon
                                className="pointer-events-none absolute left-2.5 size-3.5 text-muted-foreground"
                                aria-hidden="true"
                            />
                            <Input
                                ref={searchInputRef}
                                id={`${idPrefix}-search`}
                                size="sm"
                                type="text"
                                aria-label="搜索导入行"
                                placeholder="搜索来源行、数据类型或问题"
                                className="w-full min-w-0 pr-7 pl-8"
                                value={search}
                                onChange={(event) =>
                                    onSearch(event.target.value)
                                }
                            />
                            {search ? (
                                <Button
                                    id={`${idPrefix}-search-clear`}
                                    type="button"
                                    size="icon-xs"
                                    variant="ghost"
                                    aria-label="清除搜索"
                                    className="absolute right-0.5"
                                    onClick={() => {
                                        onSearch("")
                                        searchInputRef.current?.focus()
                                    }}
                                >
                                    <XIcon aria-hidden="true" />
                                </Button>
                            ) : null}
                        </div>
                        {filtered ? (
                            <Button
                                id={`${idPrefix}-clear-filters`}
                                type="button"
                                size="xs"
                                variant="ghost"
                                onClick={() => {
                                    onFilter(false)
                                    onSearch("")
                                }}
                            >
                                清除筛选
                            </Button>
                        ) : null}
                    </div>
                </TableToolbar>
            ) : null}

            <Table
                aria-labelledby={`${idPrefix}-rows-title`}
                className="table-fixed"
            >
                <TableHeader className="bg-surface-sunken [&_tr]:border-0">
                    <TableRow className="border-0">
                        <TableHead
                            scope="col"
                            className="w-[28%] rounded-l-lg bg-surface-sunken whitespace-normal"
                        >
                            来源行
                        </TableHead>
                        <TableHead
                            scope="col"
                            className="w-[28%] bg-surface-sunken whitespace-normal"
                        >
                            数据类型
                        </TableHead>
                        <TableHead
                            scope="col"
                            className="w-[24%] bg-surface-sunken whitespace-normal"
                        >
                            责任范围
                        </TableHead>
                        <TableHead
                            scope="col"
                            className="w-[20%] rounded-r-lg bg-surface-sunken whitespace-normal"
                        >
                            校验
                        </TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {rows.map((row) => {
                        const anomaly = row.result !== "valid"
                        const highlight = highlightedRowId === row.id
                        return (
                            <Fragment key={row.id}>
                                <TableRow
                                    id={batchReviewRowId(idPrefix, row.id)}
                                    tabIndex={highlight ? -1 : undefined}
                                    className={cn(
                                        "scroll-mt-3 [&>td]:px-2.5 [&>td]:wrap-anywhere [&>td]:whitespace-normal",
                                        anomaly && "border-b-0",
                                        highlight &&
                                            "bg-info-soft/50 ring-1 ring-inset ring-info-border hover:bg-info-soft/50 focus:outline-none motion-safe:animate-pulse",
                                    )}
                                >
                                    <TableCell className="num">
                                        {row.sourceKey}
                                    </TableCell>
                                    <TableCell>{row.objectLabel}</TableCell>
                                    <TableCell>
                                        {row.scope
                                            ? CONFIRMATION_SCOPE_LABEL[
                                                  row.scope
                                              ]
                                            : "—"}
                                    </TableCell>
                                    <TableCell>
                                        <StatusBadge
                                            appearance="text"
                                            {...resultMeta[row.result]}
                                        />
                                    </TableCell>
                                </TableRow>
                                {anomaly ? (
                                    <TableRow>
                                        <TableCell
                                            colSpan={4}
                                            className="px-0 pt-0 pb-1.5 whitespace-normal"
                                        >
                                            <p
                                                className={cn(
                                                    "border-l-2 px-2.5 py-1 text-xs wrap-anywhere",
                                                    row.result === "invalid"
                                                        ? "border-l-destructive bg-destructive-soft text-destructive-soft-foreground"
                                                        : "border-l-warning bg-warning-soft text-warning-soft-foreground",
                                                )}
                                            >
                                                {row.problems.join(" · ")}
                                            </p>
                                        </TableCell>
                                    </TableRow>
                                ) : null}
                            </Fragment>
                        )
                    })}
                    {rows.length === 0 ? (
                        <TableRow>
                            <TableCell
                                colSpan={4}
                                className="text-center text-xs text-muted-foreground"
                            >
                                {filtered
                                    ? "当前筛选下没有匹配行"
                                    : "批次暂无导入行"}
                            </TableCell>
                        </TableRow>
                    ) : null}
                </TableBody>
                <TableFooter className="border-0 bg-surface-sunken [&_tr]:border-b-0">
                    <TableRow className="hover:bg-transparent">
                        <TableCell
                            colSpan={4}
                            className="num rounded-lg text-xs whitespace-normal"
                        >
                            整批合计 {total} 项
                            {filtered
                                ? `（当前显示 ${rows.length} 项${filteredCount > rows.length ? `，筛选命中 ${filteredCount} 项` : ""}，合计按整批）`
                                : null}
                        </TableCell>
                    </TableRow>
                </TableFooter>
            </Table>

            {filteredCount > reviewPageSize ? (
                <nav
                    className="flex flex-wrap items-center justify-end gap-2 text-xs text-muted-foreground"
                    aria-label="导入行分页"
                >
                    <Button
                        id={`${idPrefix}-previous`}
                        type="button"
                        size="xs"
                        variant="outline"
                        disabled={page <= 1}
                        onClick={() => onPage(page - 1)}
                    >
                        上一页
                    </Button>
                    <span className="num">
                        第 {page} / {pageCount} 页
                        {filtered ? ` · 匹配 ${filteredCount} 项` : null}
                    </span>
                    <Button
                        id={`${idPrefix}-next`}
                        type="button"
                        size="xs"
                        variant="outline"
                        disabled={page >= pageCount}
                        onClick={() => onPage(page + 1)}
                    >
                        下一页
                    </Button>
                </nav>
            ) : null}
        </section>
    )
}
