"use client"

import * as React from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { MasterDataListItem } from "@/features/master-data/types"

/** 字典/仓库列表共用表格：列固定、错误态、筛选空态与新建空态。 */
export function DictionaryListTable({
    rows,
    pageRows,
    columns,
    pagination,
    onPaginationChange,
    listLoadFailed,
    error,
    onRetry,
    hasActiveFilters,
    onClearFilters,
    emptyTitle,
    emptyDescription,
    emptyAction,
    onRowPreview,
    onRowOpen,
}: {
    rows: readonly MasterDataListItem[]
    pageRows: MasterDataListItem[]
    columns: ColumnDef<MasterDataListItem>[]
    pagination: PaginationState
    onPaginationChange: (next: PaginationState) => void
    listLoadFailed: boolean
    error: unknown
    onRetry: () => void
    hasActiveFilters: boolean
    onClearFilters: () => void
    emptyTitle: string
    emptyDescription: string
    emptyAction?: React.ReactNode
    onRowPreview: (row: MasterDataListItem) => void
    onRowOpen: (row: MasterDataListItem) => void
}) {
    return (
        <DataTable
            data={pageRows}
            columns={columns}
            getRowId={(row) => row.stableId}
            rowCount={rows.length}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            layout="flush"
            density="compact"
            defaultColumnPinning={{
                left: ["stableNo"],
                right: ["actions"],
            }}
            errorState={
                listLoadFailed ? (
                    <BusinessFailureState error={error} onRetry={onRetry} />
                ) : undefined
            }
            emptyState={
                !listLoadFailed && rows.length === 0 ? (
                    <BusinessEmptyState
                        kind={hasActiveFilters ? "filter" : "no-data"}
                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                        title={hasActiveFilters ? "当前筛选无结果" : emptyTitle}
                        description={
                            hasActiveFilters
                                ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                : emptyDescription
                        }
                        action={
                            hasActiveFilters ? (
                                <Button
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    className="rounded-lg shadow-none"
                                    onClick={onClearFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : (
                                emptyAction
                            )
                        }
                    />
                ) : undefined
            }
            onRowPreview={onRowPreview}
            onRowOpen={onRowOpen}
        />
    )
}
