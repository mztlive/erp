"use client"

import type {
    ColumnDef,
    PaginationState,
    RowSelectionState,
} from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { ExecutionProjectionBulkBar } from "@/features/execution-projections/components/execution-projection-bulk-bar"
import { ExecutionProjectionFilterBar } from "@/features/execution-projections/components/execution-projection-filter-bar"
import type {
    ExecutionProjectionRow,
    LatencyBand,
    ProjectionSource,
    ReconciliationStatus,
} from "@/features/execution-projections/types"

type ReplaceParams = (patch: Record<string, string | null | undefined>) => void

export function ExecutionProjectionListPanel({
    rows,
    columns,
    total,
    rowSelection,
    onRowSelectionChange,
    pagination,
    onPaginationChange,
    hasActiveFilters,
    clearFilters,
    filterSummary,
    replaceParams,
    searchInputRef,
    searchDraft,
    onSearchDraftChange,
    mallId,
    deliveryStatus,
    latency,
    reconciliation,
    source,
    malls,
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
    hasActiveFilters: boolean
    clearFilters: () => void
    filterSummary: string | undefined
    replaceParams: ReplaceParams
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    mallId: string
    deliveryStatus: string
    latency: LatencyBand | "all"
    reconciliation: ReconciliationStatus | "all"
    source: ProjectionSource | "all"
    malls: Array<{ id: string; name: string }>
    selectedCount: number
    bulkOverLimit: boolean
    bulkPending: boolean
    onClearSelection: () => void
    onBulkQuery: () => void
    onBulkRetry: () => void
}) {
    return (
        /* D24：ListToolbar 移入 frame 的 toolbar 槽；批量选择条进 selectionBar 槽（表格正上方） */
        <BusinessTableFrame
            title="执行信息列表"
            description={
                <span aria-live="polite">
                    销售单身份列与操作列固定；每页条数可在分页条切换。指标与列表数据均受权限范围控制。
                    {filterSummary ? ` 筛选：${filterSummary}` : ""}
                </span>
            }
            toolbar={
                <ExecutionProjectionFilterBar
                    replaceParams={replaceParams}
                    searchInputRef={searchInputRef}
                    searchDraft={searchDraft}
                    onSearchDraftChange={onSearchDraftChange}
                    mallId={mallId}
                    deliveryStatus={deliveryStatus}
                    latency={latency}
                    reconciliation={reconciliation}
                    source={source}
                    malls={malls}
                    total={total}
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
                    emptyState={
                        rows.length === 0 ? (
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
                                            type="button"
                                            size="sm"
                                            variant="secondary"
                                            className="rounded-lg shadow-none"
                                            onClick={clearFilters}
                                        >
                                            清除筛选
                                        </Button>
                                    }
                                />
                            ) : (
                                // D24：无筛选时空态不引导「清除筛选」，避免误导
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
