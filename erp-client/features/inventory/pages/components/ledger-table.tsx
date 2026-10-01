"use client"

import type {
    ColumnDef,
    ColumnPinningState,
    PaginationState,
} from "@tanstack/react-table"

import { DataTable } from "@/components/business"

interface LedgerDataTableProps<TData> {
    id?: string
    data: TData[]
    columns: ColumnDef<TData, unknown>[]
    getRowId: (row: TData) => string
    rowCount: number
    loading: boolean
    pagination: PaginationState
    onPaginationChange: (pagination: PaginationState) => void
    defaultColumnPinning: ColumnPinningState
    onRowPreview?: (row: TData) => void
    onRowOpen?: (row: TData) => void
}

export function LedgerDataTable<TData>({
    id,
    data,
    columns,
    getRowId,
    rowCount,
    loading,
    pagination,
    onPaginationChange,
    defaultColumnPinning,
    onRowPreview,
    onRowOpen,
}: LedgerDataTableProps<TData>) {
    return (
        <DataTable
            id={id}
            data={data}
            loading={loading}
            showRefreshingBanner={loading}
            columns={columns}
            getRowId={getRowId}
            rowCount={rowCount}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            layout="flush"
            defaultColumnPinning={defaultColumnPinning}
            onRowPreview={onRowPreview}
            onRowOpen={onRowOpen}
        />
    )
}
