"use client"

import * as React from "react"
import type {
    ColumnDef,
    ColumnPinningState,
    PaginationState,
} from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ProductPublicationRow } from "@/features/product-publications/types"

export function PublicationListTable({
    isPending,
    isError,
    error,
    onRetry,
    items,
    emptyReason,
    creationBlockerMessage,
    onClearFilters,
    columns,
    columnPinning,
    pagination,
    onPaginationChange,
    rowCount,
    isFetching,
    onRowPreview,
    onRowOpen,
}: {
    isPending: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    items: ProductPublicationRow[]
    emptyReason?: "NO_DATA" | "FILTER_NO_RESULT" | "NO_DATA_SCOPE"
    creationBlockerMessage?: string
    onClearFilters: () => void
    columns: ColumnDef<ProductPublicationRow, unknown>[]
    columnPinning: ColumnPinningState
    pagination: PaginationState
    onPaginationChange: (pagination: PaginationState) => void
    rowCount: number
    isFetching: boolean
    onRowPreview: (row: ProductPublicationRow) => void
    onRowOpen: (row: ProductPublicationRow) => void
}) {
    if (isPending) {
        return (
            <div
                className="h-64 animate-pulse rounded-lg bg-muted"
                aria-busy
            />
        )
    }
    if (isError) {
        return (
            <BusinessFailureState
                title="加载失败"
                error={error}
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                action={
                    <Button
                        type="button"
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        onClick={onRetry}
                    >
                        重试
                    </Button>
                }
            />
        )
    }
    if (items.length === 0) {
        const isFilterNoResult = emptyReason === "FILTER_NO_RESULT"
        return (
            <BusinessEmptyState
                kind={isFilterNoResult ? "filter" : "no-data"}
                title={isFilterNoResult ? "无符合条件的发布" : "尚无商品发布"}
                description={
                    isFilterNoResult
                        ? "可清除筛选或调整条件后重试；已失效发布请在「发布状态」选择「已失效」查看。"
                        : creationBlockerMessage
                }
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                action={
                    isFilterNoResult ? (
                        <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            className="rounded-lg shadow-none"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : undefined
                }
            />
        )
    }
    return (
        <DataTable
            data={items}
            columns={columns}
            getRowId={(row) => row.publicationId}
            layout="flush"
            enableColumnPinning
            columnPinning={columnPinning}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            rowCount={rowCount}
            manualPagination
            loading={isFetching}
            onRowPreview={onRowPreview}
            onRowOpen={onRowOpen}
            showPagination
            pageSizeOptions={[10, 20, 50]}
        />
    )
}
