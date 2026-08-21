"use client"

import * as React from "react"

import { sortOptions } from "@/features/inventory/lib/presentation"
import type { InventoryView } from "@/features/inventory/types"
import type { LedgerPatchUrl } from "./use-inventory-ledger-url-state"

export interface LedgerFilterActionsInput {
    patchUrl: LedgerPatchUrl
    resetPagination: () => void
    sortValue: string
}

/**
 * 视图与排序等非筛选工具栏动作：切换视图、调整排序直接写 URL（replace）并回第 1 页。
 * 筛选条件（q / 仓库 / 状态 / 流水类型 / 日期）一律走 useLedgerFilters 的统一提交，
 * 不在本 hook 内逐字段写 URL。
 */
export function useLedgerFilterActions({
    patchUrl,
    resetPagination,
    sortValue,
}: LedgerFilterActionsInput) {
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

    const handleSortChange = React.useCallback(
        (nextSort: string) => {
            patchUrl({ sort: nextSort || null }, { replace: true })
            resetPagination()
        },
        [patchUrl, resetPagination],
    )

    return {
        handleViewChange,
        handleSortChange,
    }
}
