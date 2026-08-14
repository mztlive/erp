"use client"

import * as React from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import { BusinessTableFrame } from "@/components/business"
import { VIEW_LABEL } from "@/features/inventory/types"
import type {
    InventoryAvailability,
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
    data: InventoryListView
    loading: boolean
    pagination: PaginationState
    onPaginationChange: (pagination: PaginationState) => void
    balanceColumns: ColumnDef<StockBalanceRow, unknown>[]
    movementColumns: ColumnDef<StockMovementRow, unknown>[]
    reservationColumns: ColumnDef<StockReservationRow, unknown>[]
    adjustmentColumns: ColumnDef<StockAdjustmentRow, unknown>[]
    onOpenDetail: (balanceId: string) => void
    searchInput: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    onSearchChange: (value: string) => void
    warehouseId: string | undefined
    availability: InventoryAvailability
    movementType: string[]
    occurredFrom: string | undefined
    occurredTo: string | undefined
    sortValue: string
    hasActiveFilters: boolean
    skuId: string | undefined
    salesOrderLineId: string | undefined
    adjustmentIdParam: string | undefined
    chipSkuName: string | undefined
    chipSalesLineLabel: string | undefined
    chipAdjustmentNo: string | undefined
    onApplyPatch: (patch: Record<string, string | null | undefined>) => void
    onClearAll: () => void
    onClearFiltersEmpty: () => void
}

export function LedgerTableFrame({
    view,
    data,
    loading,
    pagination,
    onPaginationChange,
    balanceColumns,
    movementColumns,
    reservationColumns,
    adjustmentColumns,
    onOpenDetail,
    searchInput,
    searchInputRef,
    onSearchChange,
    warehouseId,
    availability,
    movementType,
    occurredFrom,
    occurredTo,
    sortValue,
    hasActiveFilters,
    skuId,
    salesOrderLineId,
    adjustmentIdParam,
    chipSkuName,
    chipSalesLineLabel,
    chipAdjustmentNo,
    onApplyPatch,
    onClearAll,
    onClearFiltersEmpty,
}: LedgerTableFrameProps) {
    const pageRows = (() => {
        if (view === "balance") {
            return data.balances
        }
        if (view === "movement") {
            return data.movements
        }
        if (view === "reservation") {
            return data.reservations
        }
        return data.adjustments
    })()

    return (
        <BusinessTableFrame
            title={VIEW_LABEL[view]}
            description={
                <span aria-live="polite">
                    {data.filterSummary}
                    {view === "balance" ? (
                        <span className="text-muted-foreground">
                            {" "}
                            · 数量均带基础单位；可用数量以系统数据为准
                        </span>
                    ) : null}
                </span>
            }
            toolbar={
                <LedgerToolbar
                    view={view}
                    searchInput={searchInput}
                    searchInputRef={searchInputRef}
                    onSearchChange={onSearchChange}
                    warehouseId={warehouseId}
                    availability={availability}
                    movementType={movementType}
                    occurredFrom={occurredFrom}
                    occurredTo={occurredTo}
                    sortValue={sortValue}
                    total={data.total}
                    hasActiveFilters={hasActiveFilters}
                    skuId={skuId}
                    salesOrderLineId={salesOrderLineId}
                    adjustmentIdParam={adjustmentIdParam}
                    chipSkuName={chipSkuName}
                    chipSalesLineLabel={chipSalesLineLabel}
                    chipAdjustmentNo={chipAdjustmentNo}
                    onApplyPatch={onApplyPatch}
                    onClearAll={onClearAll}
                />
            }
            table={
                data.total === 0 ? (
                    <LedgerTableEmpty
                        emptyReason={data.emptyReason}
                        filterSummary={data.filterSummary}
                        onClearFilters={onClearFiltersEmpty}
                    />
                ) : view === "balance" ? (
                    <LedgerDataTable
                        data={pageRows as StockBalanceRow[]}
                        loading={loading}
                        columns={balanceColumns}
                        getRowId={(row) => row.balanceId}
                        rowCount={data.total}
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
                        data={pageRows as StockMovementRow[]}
                        loading={loading}
                        columns={movementColumns}
                        getRowId={(row) => row.movementId}
                        rowCount={data.total}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        defaultColumnPinning={{ left: ["identity"] }}
                    />
                ) : view === "reservation" ? (
                    <LedgerDataTable
                        data={pageRows as StockReservationRow[]}
                        loading={loading}
                        columns={reservationColumns}
                        getRowId={(row) => row.reservationId}
                        rowCount={data.total}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        defaultColumnPinning={{
                            left: ["identity"],
                            right: ["actions"],
                        }}
                    />
                ) : (
                    <LedgerDataTable
                        data={pageRows as StockAdjustmentRow[]}
                        loading={loading}
                        columns={adjustmentColumns}
                        getRowId={(row) => row.adjustmentId}
                        rowCount={data.total}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        defaultColumnPinning={{ left: ["doc"] }}
                    />
                )
            }
        />
    )
}
