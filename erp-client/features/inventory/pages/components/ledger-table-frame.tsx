"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    OptionCombobox,
} from "@/components/business"
import {
    ListWorkSurface,
    listWorkspaceEmptyStateClassName,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import {
    defaultSortValue,
    sortOptions,
} from "@/features/inventory/lib/presentation"
import type { LedgerAppliedChip } from "@/features/inventory/pages/hooks/use-ledger-filters"
import type { useLedgerFilters } from "@/features/inventory/pages/hooks/use-ledger-filters"
import type {
    InventoryListView,
    InventoryView,
    StockAdjustmentRow,
    StockBalanceRow,
    StockMovementRow,
    StockReservationRow,
} from "@/features/inventory/types"
import { LedgerDataTable } from "./ledger-table"
import { LedgerToolbar } from "./ledger-toolbar"
import { LedgerViewTabs } from "./ledger-view-tabs"

interface LedgerTableFrameProps {
    view: InventoryView
    data: InventoryListView | undefined
    loading: boolean
    isError: boolean
    error: Error | null
    onRetry: () => void
    pagination: PaginationState
    onPaginationChange: (pagination: PaginationState) => void
    balanceColumns: ColumnDef<StockBalanceRow, unknown>[]
    movementColumns: ColumnDef<StockMovementRow, unknown>[]
    reservationColumns: ColumnDef<StockReservationRow, unknown>[]
    adjustmentColumns: ColumnDef<StockAdjustmentRow, unknown>[]
    onOpenDetail: (balanceId: string) => void
    onOpenAdjustment: (adjustmentId: string) => void
    onViewChange: (nextView: InventoryView) => void
    sortValue: string
    onSortChange: (value: string) => void
    hasActiveFilters: boolean
    appliedChips: readonly LedgerAppliedChip[]
    searchInputRef: React.RefObject<HTMLInputElement | null>
    filters: ReturnType<typeof useLedgerFilters>
}

export function LedgerTableFrame({
    view,
    data,
    loading,
    isError,
    error,
    onRetry,
    pagination,
    onPaginationChange,
    balanceColumns,
    movementColumns,
    reservationColumns,
    adjustmentColumns,
    onOpenDetail,
    onOpenAdjustment,
    onViewChange,
    sortValue,
    onSortChange,
    hasActiveFilters,
    appliedChips,
    searchInputRef,
    filters,
}: LedgerTableFrameProps) {
    const pageRows = (() => {
        if (view === "balance") {
            return data?.balances ?? []
        }
        if (view === "movement") {
            return data?.movements ?? []
        }
        if (view === "reservation") {
            return data?.reservations ?? []
        }
        return data?.adjustments ?? []
    })()

    return (
        <ListWorkSurface
            ariaLabel="库存台账列表"
            views={
                <LedgerViewTabs
                    view={view}
                    total={data?.total ?? 0}
                    onViewChange={onViewChange}
                />
            }
            toolbar={
                <div className="flex min-w-0 items-start gap-3">
                    <div className="min-w-0 flex-1">
                        <LedgerToolbar
                            view={view}
                            hasActiveFilters={hasActiveFilters}
                            appliedChips={appliedChips}
                            searchInputRef={searchInputRef}
                            resultCount={data?.total}
                            loading={loading}
                            failed={isError}
                            {...filters}
                        />
                    </div>
                    <OptionCombobox
                        id="inventory-ledger-sort"
                        className="w-40 shrink-0"
                        value={sortValue}
                        onValueChange={(value) =>
                            onSortChange(value ?? defaultSortValue(view))
                        }
                        options={sortOptions(view)}
                        allowClear={false}
                        aria-label="排序方式"
                        placeholder="排序"
                    />
                </div>
            }
            table={
                isError ? (
                    <BusinessFailureState
                        title="库存台账加载失败"
                        error={error}
                        onRetry={onRetry}
                    />
                ) : (data?.total ?? 0) === 0 ? (
                    <LedgerTableEmptyContent
                        emptyReason={data?.emptyReason}
                        filterSummary={data?.filterSummary ?? ""}
                        onClearFilters={filters.clearAllFilters}
                    />
                ) : view === "balance" ? (
                    <LedgerDataTable
                        id="inventory-ledger-balance-table"
                        data={pageRows as StockBalanceRow[]}
                        loading={loading}
                        columns={balanceColumns}
                        getRowId={(row) => row.balanceId}
                        rowCount={data?.total ?? 0}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        defaultColumnPinning={{
                            left: ["identity"],
                            right: ["actions"],
                        }}
                        onRowPreview={(row) => onOpenDetail(row.balanceId)}
                        onRowOpen={(row) => onOpenDetail(row.balanceId)}
                    />
                ) : view === "movement" ? (
                    <LedgerDataTable
                        id="inventory-ledger-movement-table"
                        data={pageRows as StockMovementRow[]}
                        loading={loading}
                        columns={movementColumns}
                        getRowId={(row) => row.movementId}
                        rowCount={data?.total ?? 0}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        defaultColumnPinning={{ left: ["identity"] }}
                    />
                ) : view === "reservation" ? (
                    <LedgerDataTable
                        id="inventory-ledger-reservation-table"
                        data={pageRows as StockReservationRow[]}
                        loading={loading}
                        columns={reservationColumns}
                        getRowId={(row) => row.reservationId}
                        rowCount={data?.total ?? 0}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        defaultColumnPinning={{
                            left: ["identity"],
                            right: ["actions"],
                        }}
                    />
                ) : (
                    <LedgerDataTable
                        id="inventory-ledger-adjustment-table"
                        data={pageRows as StockAdjustmentRow[]}
                        loading={loading}
                        columns={adjustmentColumns}
                        getRowId={(row) => row.adjustmentId}
                        rowCount={data?.total ?? 0}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        defaultColumnPinning={{ left: ["doc"] }}
                        onRowPreview={(row) =>
                            onOpenAdjustment(row.adjustmentId)
                        }
                        onRowOpen={(row) => onOpenAdjustment(row.adjustmentId)}
                    />
                )
            }
        />
    )
}

function LedgerTableEmptyContent({
    emptyReason,
    filterSummary,
    onClearFilters,
}: {
    emptyReason: InventoryListView["emptyReason"]
    filterSummary: string
    onClearFilters: () => void
}) {
    if (emptyReason === "FILTER_NO_RESULT") {
        return (
            <BusinessEmptyState
                kind="filter"
                className={listWorkspaceEmptyStateClassName}
                title="当前筛选无结果"
                description={`没有符合「${filterSummary}」的记录。可清除筛选或切换视图。`}
                action={
                    <Button
                        id="inventory-ledger-empty-clear-filters"
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
            className={listWorkspaceEmptyStateClassName}
            title="当前仓库尚无 ERP 自有库存记录"
            description="期初库存需在「导入与期初」完成导入后才会形成流水；商城旧库存不会自动显示在此。"
            action={
                <Button
                    id="inventory-ledger-empty-go-imports"
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
