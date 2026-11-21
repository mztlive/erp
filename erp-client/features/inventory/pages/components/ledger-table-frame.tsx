"use client"

import type * as React from "react"
import Link from "next/link"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import { BusinessEmptyState, OptionCombobox } from "@/components/business"
import {
    ListWorkSurface,
    listWorkspaceEmptyStateClassName,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import {
    defaultSortValue,
    sortOptions,
} from "@/features/inventory/lib/presentation"
import type {
    LedgerAppliedChip,
    useLedgerFilters,
} from "@/features/inventory/pages/hooks/use-ledger-filters"
import {
    VIEW_LABEL,
    type InventoryListView,
    type InventoryView,
    type StockAdjustmentRow,
    type StockBalanceRow,
    type StockMovementRow,
    type StockReservationRow,
} from "@/features/inventory/types"
import { LedgerDataTable, type LedgerTableState } from "./ledger-table"
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
    onOpenDetail?: (balanceId: string) => void
    onOpenAdjustment?: (adjustmentId: string) => void
    previewBalanceId: string | null
    previewAdjustmentId: string | null
    canOpenImports: boolean
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
    previewBalanceId,
    previewAdjustmentId,
    canOpenImports,
    sortValue,
    onSortChange,
    hasActiveFilters,
    appliedChips,
    searchInputRef,
    filters,
}: LedgerTableFrameProps) {
    const denied = !isError && data?.emptyReason === "PERMISSION_REVOKED"
    const noScope = !isError && data?.emptyReason === "NO_DATA_SCOPE"
    const isInitialEmpty =
        !isError && data?.emptyReason === "NO_DATA" && !hasActiveFilters
    const unavailable = denied || noScope
    const state: LedgerTableState = {
        rowCount: data?.total ?? 0,
        loading,
        isError,
        error,
        onRetry,
        noScope,
        denied,
        hasActiveFilters,
        onClearFilters: filters.clearAllFilters,
        emptyState: isInitialEmpty ? (
            <BusinessEmptyState
                kind="no-data"
                className={listWorkspaceEmptyStateClassName}
                title="尚未建立库存台账"
                description="完成期初库存导入后，即可查看库存余额和流水。商城旧库存不会自动显示在此。"
                action={
                    canOpenImports ? (
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
                    ) : undefined
                }
            />
        ) : undefined,
    }
    const tableProps = {
        id: `inventory-ledger-${view}-table`,
        entityName: VIEW_LABEL[view],
        caption: `库存台账 · ${VIEW_LABEL[view]}`,
        pagination,
        onPaginationChange,
        state,
    }
    const previewBalance = onOpenDetail
        ? (row: StockBalanceRow) => onOpenDetail(row.balanceId)
        : undefined
    const previewAdjustment = onOpenAdjustment
        ? (row: StockAdjustmentRow) => onOpenAdjustment(row.adjustmentId)
        : undefined

    const table = (() => {
        switch (view) {
            case "balance":
                return (
                    <LedgerDataTable
                        {...tableProps}
                        key={view}
                        data={data?.balances}
                        columns={balanceColumns}
                        getRowId={(row) => row.balanceId}
                        rowLabel={(row) =>
                            `${row.skuName}（${row.skuCode}），${row.warehouseName}`
                        }
                        defaultColumnPinning={{
                            left: ["identity"],
                            right: ["actions"],
                        }}
                        defaultColumnVisibility={{ lastMovement: false }}
                        defaultColumnOrder={[
                            "identity",
                            "onHand",
                            "reserved",
                            "available",
                            "status",
                            "lastMovement",
                            "actions",
                        ]}
                        highlightedRowId={previewBalanceId ?? undefined}
                        onRowPreview={previewBalance}
                        onRowOpen={previewBalance}
                    />
                )
            case "movement":
                return (
                    <LedgerDataTable
                        {...tableProps}
                        key={view}
                        data={data?.movements}
                        columns={movementColumns}
                        getRowId={(row) => row.movementId}
                        defaultColumnPinning={{
                            left: ["identity"],
                            right: ["source"],
                        }}
                        defaultColumnOrder={[
                            "identity",
                            "type",
                            "qty",
                            "occurred",
                            "recorder",
                            "source",
                        ]}
                    />
                )
            case "reservation":
                return (
                    <LedgerDataTable
                        {...tableProps}
                        key={view}
                        data={data?.reservations}
                        columns={reservationColumns}
                        getRowId={(row) => row.reservationId}
                        defaultColumnPinning={{
                            left: ["identity"],
                            right: ["actions"],
                        }}
                        defaultColumnOrder={[
                            "identity",
                            "sales",
                            "qty",
                            "status",
                            "source",
                            "actions",
                        ]}
                    />
                )
            case "adjustment":
                return (
                    <LedgerDataTable
                        {...tableProps}
                        key={view}
                        data={data?.adjustments}
                        columns={adjustmentColumns}
                        getRowId={(row) => row.adjustmentId}
                        rowLabel={(row) =>
                            `调整单 ${row.adjustmentNo}，${row.warehouseName}`
                        }
                        defaultColumnPinning={{ left: ["doc"] }}
                        defaultColumnOrder={[
                            "doc",
                            "identity",
                            "status",
                            "people",
                            "time",
                        ]}
                        highlightedRowId={previewAdjustmentId ?? undefined}
                        onRowPreview={previewAdjustment}
                        onRowOpen={previewAdjustment}
                    />
                )
        }
    })()

    return (
        <ListWorkSurface
            ariaLabel="库存台账列表"
            tableClassName="max-sm:[&_:is([data-column-id=doc],[data-column-id=identity])[data-column-pinned=left]]:w-table-column-status max-sm:[&_:is([data-column-id=doc],[data-column-id=identity])[data-column-pinned=left]]:min-w-table-column-status-min"
            toolbar={
                unavailable ? undefined : (
                    <LedgerToolbar
                        key={view}
                        view={view}
                        hasActiveFilters={hasActiveFilters}
                        appliedChips={appliedChips}
                        searchInputRef={searchInputRef}
                        loading={loading}
                        failed={isError}
                        {...filters}
                    />
                )
            }
            tableActions={
                unavailable || isInitialEmpty ? undefined : (
                    <OptionCombobox
                        id="inventory-ledger-sort"
                        className="w-44 max-w-full min-w-0"
                        size="sm"
                        filterLabel="排序"
                        value={sortValue}
                        onValueChange={(value) =>
                            onSortChange(value ?? defaultSortValue(view))
                        }
                        options={sortOptions(view)}
                        allowClear={false}
                        aria-label="排序方式"
                        placeholder="排序"
                    />
                )
            }
            table={table}
        />
    )
}
