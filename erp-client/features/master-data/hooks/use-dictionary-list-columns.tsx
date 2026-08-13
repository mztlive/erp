"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    blockerColumn,
    disableOnlyActionsColumn,
    effectivePeriodColumn,
    fullActionsColumn,
    lifecycleColumn,
    nameColumn,
    revisionNoColumn,
    revisionTimingColumn,
    stableNoColumn,
    updateOnlyActionsColumn,
} from "@/features/master-data/lib/list-column-primitives"
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
}: {
    lastFocusedRowId: React.MutableRefObject<string | null>
    rows: readonly MasterDataListItem[]
    onReviseTarget: (item: MasterDataListItem) => void
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
            updateOnlyActionsColumn({ lastFocusedRowId, onReviseTarget }),
        ],
        [lastFocusedRowId, onReviseTarget, rows],
    )
}

export function useWarehouseListColumns({
    lastFocusedRowId,
    rows,
    onPreview,
    onReviseTarget,
    onDisableTarget,
}: {
    lastFocusedRowId: React.MutableRefObject<string | null>
    rows: readonly MasterDataListItem[]
    onPreview: (stableId: string) => void
    onReviseTarget: (item: MasterDataListItem) => void
    onDisableTarget: (item: MasterDataListItem) => void
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
            fullActionsColumn({
                lastFocusedRowId,
                onPreview,
                onReviseTarget,
                onDisableTarget,
            }),
        ],
        [lastFocusedRowId, onDisableTarget, onPreview, onReviseTarget, rows],
    )
}
