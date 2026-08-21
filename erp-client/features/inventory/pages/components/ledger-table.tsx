"use client"

import Link from "next/link"
import type {
    ColumnDef,
    ColumnPinningState,
    PaginationState,
} from "@tanstack/react-table"

import { BusinessEmptyState, DataTable } from "@/components/business"
import { Button } from "@/components/ui/button"

type LedgerTableEmptyReason =
    | "NO_DATA"
    | "FILTER_NO_RESULT"
    | "NO_DATA_SCOPE"
    | "PERMISSION_REVOKED"

interface LedgerTableEmptyProps {
    emptyReason: LedgerTableEmptyReason | undefined
    filterSummary: string
    onClearFilters: () => void
}

export function LedgerTableEmpty({
    emptyReason,
    filterSummary,
    onClearFilters,
}: LedgerTableEmptyProps) {
    if (emptyReason === "FILTER_NO_RESULT") {
        return (
            <BusinessEmptyState
                kind="filter"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                title="当前筛选无结果"
                description={`没有符合「${filterSummary}」的记录。可清除筛选或切换视图。`}
                action={
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        className="rounded-lg shadow-none"
                        onClick={onClearFilters}
                    >
                        清除筛选
                    </Button>
                }
            />
        )
    }
    return (
        <BusinessEmptyState
            kind="no-data"
            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
            title="当前仓库尚无 ERP 自有库存记录"
            description="期初库存需在「导入与期初」完成导入后才会形成流水；商城旧库存不会自动显示在此。"
            action={
                <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    className="rounded-lg shadow-none"
                    render={<Link href="/governance/imports" />}
                >
                    前往导入与期初
                </Button>
            }
        />
    )
}

interface LedgerDataTableProps<TData> {
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
