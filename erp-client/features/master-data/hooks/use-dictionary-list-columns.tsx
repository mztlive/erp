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
    canReadRevisions,
}: {
    lastFocusedRowId: React.MutableRefObject<string | null>
    rows: readonly MasterDataListItem[]
    onPreview: (stableId: string) => void
    onReviseTarget: (item: MasterDataListItem) => void
    canMaintainHandlers: boolean
    canReadRevisions: boolean
}) {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            stableNoColumn(),
            nameColumn(),
            ...(canReadRevisions ? [revisionNoColumn()] : []),
            lifecycleColumn(),
            ...(canReadRevisions
                ? [revisionTimingColumn(), effectivePeriodColumn()]
                : []),
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
            canReadRevisions,
            lastFocusedRowId,
            onPreview,
            onReviseTarget,
            rows,
        ],
    )
}
