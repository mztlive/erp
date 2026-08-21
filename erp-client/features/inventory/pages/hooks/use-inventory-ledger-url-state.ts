"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { decodeInventoryCursor } from "@/features/inventory/lib/cursor"
import {
    defaultSortValue,
    parseAvailability,
    parseView,
} from "@/features/inventory/lib/presentation"
import type {
    InventoryAvailability,
    InventoryView,
} from "@/features/inventory/types"

export type LedgerPatchUrl = (
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean; scroll?: boolean },
) => void

export interface InventoryLedgerUrlState {
    view: InventoryView
    qParam: string
    warehouseId: string | undefined
    skuId: string | undefined
    salesOrderLineId: string | undefined
    availability: InventoryAvailability
    balanceIdParam: string | undefined
    adjustmentIdParam: string | undefined
    workItemIdParam: string | undefined
    movementType: string[]
    occurredFrom: string | undefined
    occurredTo: string | undefined
    sortValue: string
    pageSize: number
    cursorParam: string | undefined
    cursorOffset: number
    hasActiveFilters: boolean
    patchUrl: LedgerPatchUrl
}

export function useInventoryLedgerUrlState(): InventoryLedgerUrlState {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const view = parseView(searchParams.get("view"))
    const qParam = searchParams.get("q") ?? ""
    const warehouseId = searchParams.get("warehouseId") ?? undefined
    const skuId = searchParams.get("skuId") ?? undefined
    const salesOrderLineId = searchParams.get("salesOrderLineId") ?? undefined
    const availability = parseAvailability(searchParams.get("availability"))
    const balanceIdParam = searchParams.get("balanceId") ?? undefined
    const adjustmentIdParam = searchParams.get("adjustmentId") ?? undefined
    const workItemIdParam =
        searchParams.get("currentWorkItemId") ??
        searchParams.get("workItemId") ??
        undefined
    const movementTypeParam = searchParams.get("movementType") ?? ""
    const movementType = React.useMemo(
        () => movementTypeParam.split(",").filter(Boolean),
        [movementTypeParam],
    )
    const occurredFrom = searchParams.get("occurredFrom") ?? undefined
    const occurredTo = searchParams.get("occurredTo") ?? undefined
    const sortValue = searchParams.get("sort") ?? defaultSortValue(view)
    const pageSizeParam = Number(searchParams.get("pageSize") ?? "20")
    const pageSize =
        Number.isSafeInteger(pageSizeParam) && pageSizeParam > 0
            ? Math.min(pageSizeParam, 100)
            : 20
    const cursorParam = searchParams.get("cursor") ?? undefined
    const cursorOffset = decodeInventoryCursor(cursorParam, view)

    const patchUrl = React.useCallback<LedgerPatchUrl>(
        (patch, options) =>
            patchSearchParams(
                { router, pathname, searchParams, view, clearCursor: true },
                patch,
                options,
            ),
        [router, pathname, searchParams, view],
    )

    const hasOccurredFromParam = Boolean(occurredFrom)
    const hasOccurredToParam = Boolean(occurredTo)
    // 视图无关的通用筛选 + 仅在对应视图生效的结构化条件（availability 仅余额视图、
    // 流水类型与发生日期仅流水视图；其它视图下这些参数不被查询消费，不算已生效筛选）。
    const hasActiveFilters = Boolean(
        qParam.trim() ||
        warehouseId ||
        skuId ||
        salesOrderLineId ||
        adjustmentIdParam ||
        (view === "balance" && availability !== "all") ||
        (view === "movement" && movementType.length > 0) ||
        (view === "movement" && (hasOccurredFromParam || hasOccurredToParam)),
    )

    return {
        view,
        qParam,
        warehouseId,
        skuId,
        salesOrderLineId,
        availability,
        balanceIdParam,
        adjustmentIdParam,
        workItemIdParam,
        movementType,
        occurredFrom,
        occurredTo,
        sortValue,
        pageSize,
        cursorParam,
        cursorOffset,
        hasActiveFilters,
        patchUrl,
    }
}
