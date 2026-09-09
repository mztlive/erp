"use client"

import * as React from "react"

import { selectSellableRows } from "@/features/master-data/lib/sellable-excel-rows"
import type { MasterDataListItem } from "@/features/master-data/types"

export function useSellableGallerySelection(
    rows: readonly MasterDataListItem[],
) {
    const [selectedIds, setSelectedIds] = React.useState<ReadonlySet<string>>(
        () => new Set(),
    )
    const rowIdKey = rows.map((row) => row.stableId).join("\0")

    React.useEffect(() => {
        const allowed = new Set(rowIdKey.split("\0").filter(Boolean))
        setSelectedIds((current) => {
            const next = new Set([...current].filter((id) => allowed.has(id)))
            return next.size === current.size ? current : next
        })
    }, [rowIdKey])

    const selectedRows = React.useMemo(
        () => selectSellableRows(rows, selectedIds),
        [rows, selectedIds],
    )
    const selectedCount = selectedIds.size
    const allSelected = rows.length > 0 && selectedCount === rows.length
    const someSelected = selectedCount > 0 && !allSelected

    const toggle = React.useCallback((id: string, selected: boolean) => {
        setSelectedIds((current) => {
            const next = new Set(current)
            if (selected) next.add(id)
            else next.delete(id)
            return next
        })
    }, [])

    const selectAllResults = React.useCallback(() => {
        setSelectedIds(new Set(rows.map((row) => row.stableId)))
    }, [rows])

    const clear = React.useCallback(() => {
        setSelectedIds(new Set())
    }, [])

    return {
        selectedIds,
        selectedRows,
        selectedCount,
        allSelected,
        someSelected,
        toggle,
        selectAllResults,
        clear,
    }
}
