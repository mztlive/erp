"use client"

import * as React from "react"

import { useListSelection } from "@/hooks/use-list-selection"
import type { MasterDataListItem } from "@/features/master-data/types"

const getRowId = (row: MasterDataListItem) => row.stableId

/** 表格对当前页、卡片对已加载商品执行全选。 */
export function useSellableGallerySelection(
    rows: readonly MasterDataListItem[],
    resetKey: string,
) {
    const selection = useListSelection({ pageRows: rows, getRowId, resetKey })
    const { clear, onRowSelectionChange, remove, rowSelection } = selection
    React.useEffect(() => {
        clear()
    }, [clear])
    const toggle = React.useCallback(
        (id: string, checked: boolean) => {
            if (checked) onRowSelectionChange({ ...rowSelection, [id]: true })
            else remove(id)
        },
        [onRowSelectionChange, remove, rowSelection],
    )

    return { ...selection, toggle }
}
