"use client"

import type {
    ColumnDef,
    PaginationState,
    RowSelectionState,
} from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { ExecutionProjectionBulkBar } from "@/features/execution-projections/components/execution-projection-bulk-bar"
import { ExecutionProjectionFilterBar } from "@/features/execution-projections/components/execution-projection-filter-bar"
import type {
    ExecutionProjectionAppliedChip,
    ExecutionProjectionFilterState,
} from "@/features/execution-projections/hooks/use-execution-projection-filters"
import type { ExecutionProjectionRow } from "@/features/execution-projections/types"

type ReplaceParams = (patch: Record<string, string | null | undefined>) => void

export function ExecutionProjectionListPanel({
    rows,
    columns,
    total,
    rowSelection,
    onRowSelectionChange,
    pagination,
    onPaginationChange,
    listLoading,
    listLoadFailed,
    queryError,
    onRetry,
    hasActiveFilters,
    clearAllFilters,
    filterSummary,
    filters,
    appliedChips,
    hasChips,
    malls,
    replaceParams,
    selectedCount,
    bulkOverLimit,
    bulkPending,
    onClearSelection,
    onBulkQuery,
    onBulkRetry,
}: {
    rows: ExecutionProjectionRow[]
    columns: ColumnDef<ExecutionProjectionRow>[]
    total: number
    rowSelection: RowSelectionState
    onRowSelectionChange: (next: RowSelectionState) => void
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    listLoading: boolean
    listLoadFailed: boolean
    queryError: unknown
    onRetry: () => void
    hasActiveFilters: boolean
    clearAllFilters: () => void
    filterSummary: string | undefined
    filters: ExecutionProjectionFilterState
    appliedChips: readonly ExecutionProjectionAppliedChip[]
    hasChips: boolean
    malls: Array<{ id: string; name: string }>
    replaceParams: ReplaceParams
    selectedCount: number
    bulkOverLimit: boolean
    bulkPending: boolean
    onClearSelection: () => void
    onBulkQuery: () => void
    onBulkRetry: () => void
}) {
    return (
        <BusinessTableFrame
            showHeader
            title={
                <span className="inline-flex items-baseline gap-2">
                    执行信息
                    <span
                        aria-live="polite"
                        className="font-normal text-muted-foreground"
                    >
                        {total} 条
                    </span>
                </span>
            }
            description={
                filterSummary
                    ? `筛选：${filterSummary}`
                    : "销售单身份列与操作列固定；每页条数可在分页条切换。指标与列表数据均受权限范围控制。"
            }
            toolbar={
                <ExecutionProjectionFilterBar
                    filters={filters}
                    appliedChips={appliedChips}
                    removeFilter={filters.removeFilter}
                    hasChips={hasChips}
                    malls={malls}
                />
            }
            selectionBar={
                selectedCount > 0 ? (
                    <ExecutionProjectionBulkBar
                        selectedCount={selectedCount}
                        bulkOverLimit={bulkOverLimit}
                        bulkPending={bulkPending}
                        onClear={onClearSelection}
                        onBulkQuery={onBulkQuery}
                        onBulkRetry={onBulkRetry}
                    />
                ) : undefined
            }
            table={
                <DataTable
                    id="execution-projections-list"
                    columns={columns}
                    data={rows}
                    getRowId={(row) => row.projectionId}
                    rowCount={total}
                    enableRowSelection
                    rowSelection={rowSelection}
                    onRowSelectionChange={onRowSelectionChange}
                    onRowPreview={(row) =>
                        replaceParams({
                            projectionId: row.projectionId,
                            revision: null,
                        })
                    }
                    onRowOpen={(row) =>
                        replaceParams({
                            projectionId: row.projectionId,
                            revision: null,
                        })
                    }
                    pagination={pagination}
                    onPaginationChange={onPaginationChange}
                    manualPagination
                    layout="flush"
                    pageSizeOptions={[8, 20, 50]}
                    defaultColumnPinning={{
                        left: ["select", "salesOrder"],
                        right: ["actions"],
                    }}
                    loading={listLoading}
                    errorState={
                        listLoadFailed ? (
                            <BusinessFailureState
                                error={queryError}
                                action={
                                    <Button
                                        id="execution-projections-list-retry"
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={onRetry}
                                    >
                                        重试
                                    </Button>
                                }
                            />
                        ) : undefined
                    }
                    emptyState={
                        !listLoadFailed && rows.length === 0 ? (
                            hasActiveFilters ? (
                                <BusinessEmptyState
                                    kind="filter"
                                    title="没有匹配的执行信息"
                                    description={
                                        filterSummary
                                            ? `当前筛选：${filterSummary}`
                                            : "可清除筛选或返回销售单查看协同。"
                                    }
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    action={
                                        <Button
                                            id="execution-projections-list-clear-filters"
                                            type="button"
                                            size="sm"
                                            variant="secondary"
                                            className="rounded-lg shadow-none"
                                            onClick={clearAllFilters}
                                        >
                                            清除筛选
                                        </Button>
                                    }
                                />
                            ) : (
                                <BusinessEmptyState
                                    kind="no-data"
                                    title="当前范围没有执行信息"
                                    description="销售记录尚未形成发送记录；新数据到达后会自动显示。"
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                />
                            )
                        ) : undefined
                    }
                />
            }
        />
    )
}
