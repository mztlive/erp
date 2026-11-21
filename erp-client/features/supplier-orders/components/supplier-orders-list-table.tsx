"use client"

import type {
    ColumnDef,
    PaginationState,
    SortingState,
} from "@tanstack/react-table"

import { ListDataTable } from "@/components/business"
import type { SupplierOrderListRow } from "@/features/supplier-orders/types"

const DEFAULT_COLUMN_ORDER = [
    "identity",
    "tracks",
    "followUp",
    "handler",
    "external",
    "updated",
    "itemCount",
    "actions",
]

export type SupplierOrdersListTableProps = {
    rows: SupplierOrderListRow[]
    columns: ColumnDef<SupplierOrderListRow>[]
    total: number
    loading: boolean
    isError: boolean
    error: Error | null
    onRetry: () => void
    hasActiveFilters: boolean
    noScope?: boolean
    onClearFilters: () => void
    sorting: SortingState
    onSortingChange: (next: SortingState) => void
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    highlightedRowId?: string
    onRowPreview?: (orderId: string) => void
}

export function SupplierOrdersListTable({
    rows,
    columns,
    total,
    loading,
    isError,
    error,
    onRetry,
    hasActiveFilters,
    noScope = false,
    onClearFilters,
    sorting,
    onSortingChange,
    pagination,
    onPaginationChange,
    highlightedRowId,
    onRowPreview,
}: SupplierOrdersListTableProps) {
    return (
        <ListDataTable
            id="supplier-orders-list-table"
            entityName="供应商订单"
            caption="供应商订单列表"
            data={rows}
            columns={columns}
            getRowId={(row) => row.orderId}
            rowLabel={(row) => `供应商订单 ${row.orderNo}`}
            rowCount={total}
            loading={loading}
            isError={isError}
            error={error}
            onRetry={onRetry}
            retryButtonId="supplier-orders-list-table-error-retry"
            noScope={noScope}
            hasActiveFilters={hasActiveFilters}
            onClearFilters={onClearFilters}
            clearFiltersButtonId="supplier-orders-list-table-empty-clear"
            emptyCopy={{
                emptyTitle: "当前视图没有供应商订单",
                emptyDescription: "可切换视图或调整查询条件。",
            }}
            sorting={sorting}
            onSortingChange={onSortingChange}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            defaultColumnOrder={DEFAULT_COLUMN_ORDER}
            defaultColumnPinning={{
                left: ["identity"],
                right: ["actions"],
            }}
            scrollable
            highlightedRowId={highlightedRowId}
            onRowPreview={
                onRowPreview ? (row) => onRowPreview(row.orderId) : undefined
            }
            onRowOpen={
                onRowPreview ? (row) => onRowPreview(row.orderId) : undefined
            }
        />
    )
}
