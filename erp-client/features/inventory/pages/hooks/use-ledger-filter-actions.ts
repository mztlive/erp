"use client"

import * as React from "react"

import { sortOptions } from "@/features/inventory/lib/presentation"
import type { InventoryView } from "@/features/inventory/types"
import type { LedgerPatchUrl } from "./use-inventory-ledger-url-state"

export interface LedgerFilterActionsInput {
    patchUrl: LedgerPatchUrl
    resetPagination: () => void
    setSearchInput: React.Dispatch<React.SetStateAction<string>>
    sortValue: string
}

export function useLedgerFilterActions({
    patchUrl,
    resetPagination,
    setSearchInput,
    sortValue,
}: LedgerFilterActionsInput) {
    const handleApplyFilterPatch = React.useCallback(
        (patch: Record<string, string | null | undefined>) => {
            patchUrl(patch, { replace: true })
            resetPagination()
        },
        [patchUrl, resetPagination],
    )

    const handleViewChange = React.useCallback(
        (nextView: InventoryView) => {
            // 排序参数跨视图残留会让下拉显示占位而旧排序仍生效：不属于目标视图则一并清掉。
            const validSorts = sortOptions(nextView).map((o) => o.value)
            const patch: Record<string, string | null | undefined> = {
                view: nextView,
            }
            if (sortValue && !validSorts.includes(sortValue)) {
                patch.sort = null
            }
            patchUrl(patch, { replace: true })
            resetPagination()
        },
        [patchUrl, resetPagination, sortValue],
    )

    const handleClearAllFilters = React.useCallback(() => {
        setSearchInput("")
        // P4：清全部筛选参数；保留视图、排序与预览（balanceId 导航上下文）
        patchUrl(
            {
                q: null,
                warehouseId: null,
                availability: "all",
                skuId: null,
                salesOrderLineId: null,
                adjustmentId: null,
                movementType: null,
                occurredFrom: null,
                occurredTo: null,
            },
            { replace: true },
        )
        resetPagination()
    }, [patchUrl, resetPagination, setSearchInput])

    const handleClearFiltersEmptyState = React.useCallback(() => {
        setSearchInput("")
        // P4：清全部筛选参数；保留当前视图（不强制回 balance）
        patchUrl(
            {
                q: null,
                warehouseId: null,
                availability: "all",
                skuId: null,
                salesOrderLineId: null,
                adjustmentId: null,
            },
            { replace: true },
        )
    }, [patchUrl, setSearchInput])

    return {
        handleApplyFilterPatch,
        handleViewChange,
        handleClearAllFilters,
        handleClearFiltersEmptyState,
    }
}
