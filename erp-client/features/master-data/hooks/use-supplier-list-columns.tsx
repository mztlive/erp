"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    blockerColumn,
    fullActionsColumn,
    lifecycleColumn,
    nameColumn,
    revisionNoColumn,
    revisionTimingColumn,
} from "@/features/master-data/lib/list-column-primitives"
import type { MasterDataListItem } from "@/features/master-data/types"

export function useSupplierListColumns({
    lastFocusedRowId,
    rows,
    onOpen,
    onDisableTarget,
}: {
    lastFocusedRowId: React.MutableRefObject<string | null>
    rows: readonly MasterDataListItem[]
    onOpen: (item: MasterDataListItem) => void
    onDisableTarget: (item: MasterDataListItem) => void
}) {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            nameColumn(),
            revisionNoColumn(),
            lifecycleColumn(),
            revisionTimingColumn(),
            ...blockerColumn(rows),
            fullActionsColumn({
                lastFocusedRowId,
                onOpen,
                onDisableTarget,
            }),
        ],
        [lastFocusedRowId, onDisableTarget, onOpen, rows],
    )
}
