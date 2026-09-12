"use client"

import * as React from "react"
import { ChevronRightIcon } from "lucide-react"
import {
    getCoreRowModel,
    useReactTable,
    type PaginationState,
} from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DataTablePagination,
} from "@/components/business"
import { listWorkspaceEmptyStateClassName } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import type { MasterDataListItem } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

/** 简单字典使用整行入口，分页与筛选结果保持一致。 */
export function DictionaryCompactList({
    id,
    rows,
    pagination,
    onPaginationChange,
    loading,
    listLoadFailed,
    error,
    onRetry,
    hasActiveFilters,
    onClearFilters,
    emptyTitle,
    emptyDescription,
    emptyAction,
    codeLabel,
    selectedId,
    onPreview,
}: {
    id: string
    rows: readonly MasterDataListItem[]
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    loading: boolean
    listLoadFailed: boolean
    error: unknown
    onRetry: () => void
    hasActiveFilters: boolean
    onClearFilters: () => void
    emptyTitle: string
    emptyDescription: string
    emptyAction?: React.ReactNode
    codeLabel: string
    selectedId: string | null
    onPreview: (row: MasterDataListItem) => void
}) {
    const pageIndex = Math.min(
        pagination.pageIndex,
        Math.max(0, Math.ceil(rows.length / pagination.pageSize) - 1),
    )
    React.useEffect(() => {
        if (!loading && !listLoadFailed && pageIndex !== pagination.pageIndex) {
            onPaginationChange({ ...pagination, pageIndex })
        }
    }, [loading, listLoadFailed, pageIndex, pagination, onPaginationChange])
    const pageRows = React.useMemo(
        () =>
            rows.slice(
                pageIndex * pagination.pageSize,
                (pageIndex + 1) * pagination.pageSize,
            ),
        [rows, pageIndex, pagination.pageSize],
    )
    const table = useReactTable({
        data: pageRows,
        columns: [],
        rowCount: rows.length,
        state: { pagination: { ...pagination, pageIndex } },
        onPaginationChange: (next) =>
            onPaginationChange(
                typeof next === "function"
                    ? next({ ...pagination, pageIndex })
                    : next,
            ),
        getCoreRowModel: getCoreRowModel(),
        manualPagination: true,
    })

    if (listLoadFailed)
        return (
            <BusinessFailureState
                idPrefix={id}
                error={error}
                onRetry={onRetry}
            />
        )
    if (!loading && rows.length === 0)
        return (
            <BusinessEmptyState
                kind={hasActiveFilters ? "filter" : "no-data"}
                className={listWorkspaceEmptyStateClassName}
                title={hasActiveFilters ? "当前筛选无结果" : emptyTitle}
                description={
                    hasActiveFilters
                        ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                        : emptyDescription
                }
                action={
                    hasActiveFilters ? (
                        <Button
                            id={`${id}-clear-filters`}
                            variant="secondary"
                            size="sm"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : (
                        emptyAction
                    )
                }
            />
        )

    return (
        <div id={id} aria-busy={loading}>
            {loading ? (
                <p role="status" className="py-2 text-sm text-muted-foreground">
                    正在更新列表…
                </p>
            ) : null}
            <ul className="divide-y divide-border">
                {pageRows.map((row) => (
                    <li key={row.stableId}>
                        <button
                            id={`${id}-row-${toAutomationIdSegment(row.stableId)}`}
                            data-row-id={row.stableId}
                            type="button"
                            aria-label={`查看${row.name}，${codeLabel}：${row.dictionaryCode ?? row.stableNo}，${row.lifecycleStatusLabel}`}
                            aria-haspopup="dialog"
                            aria-expanded={selectedId === row.stableId}
                            onClick={() => onPreview(row)}
                            className={cn(
                                "flex w-full items-center gap-3 rounded-md px-3 py-3.5 text-left transition-colors hover:bg-muted/50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring sm:gap-6",
                                selectedId === row.stableId && "bg-muted/60",
                            )}
                        >
                            <span className="min-w-0 flex-1 space-y-1">
                                <span className="block break-words text-sm font-medium text-foreground">
                                    {row.name}
                                </span>
                                <span className="block break-all text-xs text-muted-foreground">
                                    {codeLabel}：
                                    <span className="num">
                                        {row.dictionaryCode ?? row.stableNo}
                                    </span>
                                </span>
                            </span>
                            <span className="shrink-0">
                                <BusinessStatusBadge
                                    label={row.lifecycleStatusLabel}
                                    tone={row.lifecycleTone}
                                />
                            </span>
                            <ChevronRightIcon
                                className="size-4 shrink-0 text-muted-foreground"
                                aria-hidden
                            />
                        </button>
                    </li>
                ))}
            </ul>
            {rows.length > pagination.pageSize || pagination.pageIndex > 0 ? (
                <DataTablePagination
                    table={table}
                    idPrefix={id}
                    pageSizeOptions={[20, 50, 100]}
                />
            ) : null}
        </div>
    )
}
