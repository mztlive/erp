"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    OptionCombobox,
} from "@/components/business"
import { ListWorkSurface } from "@/components/business/list-workspace"
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

    const isInitialEmpty =
        !isError && data?.emptyReason === "NO_DATA" && !hasActiveFilters

    return (
        <ListWorkSurface
            toolbarClassName="pt-3 pb-2"
            ariaLabel="库存台账列表"
            views={
                <LedgerViewTabs
                    view={view}
                    total={data?.total ?? 0}
                    onViewChange={onViewChange}
                />
            }
            toolbar={
                <LedgerToolbar
                    view={view}
                    hasActiveFilters={hasActiveFilters}
                    appliedChips={appliedChips}
                    searchInputRef={searchInputRef}
                    resultCount={data?.total}
                    loading={loading}
                    failed={isError}
                    {...filters}
                    actions={
                        isInitialEmpty ? undefined : (
                            <div className="flex items-center gap-2">
                                <label
                                    htmlFor="inventory-ledger-sort"
                                    className="shrink-0 text-sm text-muted-foreground"
                                >
                                    排序
                                </label>
                                <OptionCombobox
                                    id="inventory-ledger-sort"
                                    className="w-40"
                                    value={sortValue}
                                    onValueChange={(value) =>
                                        onSortChange(
                                            value ?? defaultSortValue(view),
                                        )
                                    }
                                    options={sortOptions(view)}
                                    allowClear={false}
                                    aria-label="排序方式"
                                    placeholder="排序"
                                />
                            </div>
                        )
                    }
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
                className="flex-none justify-start rounded-none border-0 bg-transparent px-4 py-12 shadow-none ring-0 md:py-16"
                title="没有符合条件的库存"
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
            className="flex-none justify-start rounded-none border-0 bg-transparent px-4 py-12 shadow-none ring-0 md:py-16"
            title="尚未建立库存台账"
            description={
                <>
                    完成期初库存导入后，即可查看库存余额和流水。
                    <span className="mt-2 block text-xs">
                        商城旧库存不会自动显示在此。
                    </span>
                </>
            }
            action={
                <Button
                    id="inventory-ledger-empty-go-imports"
                    type="button"
                    variant="secondary"
                    size="sm"
                    className="rounded-lg shadow-none"
                    render={<Link href="/governance/imports" />}
                >
                    导入期初库存
                </Button>
            }
        />
    )
}
