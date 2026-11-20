"use client"

import { useListSelection } from "@/hooks/use-list-selection"
import type { MasterDataListItem } from "@/features/master-data/types"

const getProductRowId = (row: MasterDataListItem) => row.stableId

/** 商品列表跨页多选状态，复用通用 useListSelection。 */
export function useProductListSelection({
    pageRows,
    latestRows,
    resetKey,
}: {
    pageRows: readonly MasterDataListItem[]
    latestRows?: readonly MasterDataListItem[]
    resetKey: string
}) {
    return useListSelection({
        pageRows,
        latestRows,
        getRowId: getProductRowId,
        resetKey,
    })
}
