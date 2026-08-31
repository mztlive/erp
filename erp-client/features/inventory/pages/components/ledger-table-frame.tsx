"use client"

import * as React from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
    BusinessFailureState,
    BusinessTableFrame,
    OptionCombobox,
} from "@/components/business"
import {
    defaultSortValue,
    sortOptions,
} from "@/features/inventory/lib/presentation"
import type { LedgerAppliedChip } from "@/features/inventory/pages/hooks/use-ledger-filters"
import type { useLedgerFilters } from "@/features/inventory/pages/hooks/use-ledger-filters"
import { VIEW_LABEL } from "@/features/inventory/types"
import type {
    InventoryListView,
    InventoryView,
    StockAdjustmentRow,
    StockBalanceRow,
    StockMovementRow,
    StockReservationRow,
} from "@/features/inventory/types"
import { LedgerDataTable, LedgerTableEmpty } from "./ledger-table"
import { LedgerToolbar } from "./ledger-toolbar"

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
        <BusinessTableFrame
            showHeader
            title={
                <span className="inline-flex items-baseline gap-2">
                    {VIEW_LABEL[view]}
                    <span
                        aria-live="polite"
                        className="font-normal text-muted-foreground"
                    >
                        {(data?.total ?? 0).toLocaleString("zh-CN")} 条
                    </span>
                </span>
            }
            description={
                <span aria-live="polite">
                    {data?.filterSummary ?? ""}
                    {view === "balance" ? (
                        <span className="text-muted-foreground">
                            {" "}
                            · 数量均带基础单位；可用数量以系统数据为准
                        </span>
                    ) : null}
                </span>
            }
            headerActions={
                <div className="flex items-center gap-1.5 text-sm">
                    <span className="sr-only">排序</span>
                    <OptionCombobox
                        id="inventory-ledger-sort"
                        className="w-40"
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
            toolbar={
                <LedgerToolbar
                    view={view}
                    hasActiveFilters={hasActiveFilters}
                    appliedChips={appliedChips}
                    searchInputRef={searchInputRef}
                    {...filters}
                />
            }
            table={
                isError ? (
                    <BusinessFailureState
                        title="库存台账加载失败"
                        error={error}
                        onRetry={onRetry}
                    />
                ) : (data?.total ?? 0) === 0 ? (
                    <LedgerTableEmpty
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
