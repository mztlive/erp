"use client"

import * as React from "react"
import { flexRender, type ColumnDef } from "@tanstack/react-table"

import {
    blockerColumn,
    disableOnlyActionsColumn,
    effectivePeriodColumn,
    lifecycleColumn,
    nameColumn,
    revisionNoColumn,
    revisionTimingColumn,
    stableNoColumn,
    updateOnlyActionsColumn,
    warehouseActionsColumn,
} from "@/features/master-data/components/list/list-column-primitives"
import { VoucherCategoryStatusButton } from "@/features/master-data/components/list/voucher-category-status-dialog"
import type { MasterDataListItem } from "@/features/master-data/types"

export function useBrandListColumns({
    lastFocusedRowId,
    rows,
    onDisableTarget,
}: {
    lastFocusedRowId: React.MutableRefObject<string | null>
    rows: readonly MasterDataListItem[]
    onDisableTarget: (item: MasterDataListItem) => void
}) {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            stableNoColumn(),
            nameColumn(),
            revisionNoColumn(),
            lifecycleColumn(),
            revisionTimingColumn(),
            ...blockerColumn(rows),
            disableOnlyActionsColumn({ lastFocusedRowId, onDisableTarget }),
        ],
        [lastFocusedRowId, onDisableTarget, rows],
    )
}

export function useUnitOfMeasureListColumns({
    lastFocusedRowId,
    rows,
    onDisableTarget,
}: {
    lastFocusedRowId: React.MutableRefObject<string | null>
    rows: readonly MasterDataListItem[]
    onDisableTarget: (item: MasterDataListItem) => void
}) {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            stableNoColumn(),
            nameColumn(),
            revisionNoColumn(),
            lifecycleColumn(),
            revisionTimingColumn(),
            ...blockerColumn(rows),
            disableOnlyActionsColumn({ lastFocusedRowId, onDisableTarget }),
        ],
        [lastFocusedRowId, onDisableTarget, rows],
    )
}

export function useVoucherCategoryListColumns({
    lastFocusedRowId,
    rows,
    onReviseTarget,
    onStatusTarget,
}: {
    lastFocusedRowId: React.MutableRefObject<string | null>
    rows: readonly MasterDataListItem[]
    onReviseTarget: (item: MasterDataListItem) => void
    onStatusTarget: (item: MasterDataListItem) => void
}) {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(() => {
        const actions = updateOnlyActionsColumn({
            lastFocusedRowId,
            onReviseTarget,
        })
        return [
            stableNoColumn(),
            nameColumn(),
            revisionNoColumn(),
            lifecycleColumn(),
            revisionTimingColumn(),
            effectivePeriodColumn(),
            ...blockerColumn(rows),
            {
                ...actions,
                cell: (context) => (
                    <div className="flex items-center gap-1">
                        {flexRender(actions.cell, context)}
                        <VoucherCategoryStatusButton
                            row={context.row.original}
                            surface="table"
                            onClick={() => {
                                lastFocusedRowId.current =
                                    context.row.original.stableId
                                onStatusTarget(context.row.original)
                            }}
                        />
                    </div>
                ),
            },
        ]
    }, [lastFocusedRowId, onReviseTarget, onStatusTarget, rows])
}

export function useWarehouseListColumns({
    lastFocusedRowId,
    rows,
    onPreview,
    onReviseTarget,
    canMaintainHandlers,
}: {
    lastFocusedRowId: React.MutableRefObject<string | null>
    rows: readonly MasterDataListItem[]
    onPreview: (stableId: string) => void
    onReviseTarget: (item: MasterDataListItem) => void
    canMaintainHandlers: boolean
}) {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            stableNoColumn(),
            nameColumn(),
            revisionNoColumn(),
            lifecycleColumn(),
            revisionTimingColumn(),
            effectivePeriodColumn(),
            ...blockerColumn(rows),
            warehouseActionsColumn({
                lastFocusedRowId,
                onPreview,
                onReviseTarget,
                canMaintainHandlers,
            }),
        ],
        [
            canMaintainHandlers,
            lastFocusedRowId,
            onPreview,
            onReviseTarget,
            rows,
        ],
    )
}
