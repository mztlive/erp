"use client"

import { type ColumnDef } from "@tanstack/react-table"
import * as React from "react"

import {
    blockerColumn,
    effectivePeriodColumn,
    lifecycleColumn,
    nameColumn,
    revisionNoColumn,
    revisionTimingColumn,
    stableNoColumn,
    warehouseActionsColumn,
} from "@/features/master-data/components/list/list-column-primitives"
import type { MasterDataListItem } from "@/features/master-data/types"

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
