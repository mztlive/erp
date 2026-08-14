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
    options?: { replace?: boolean },
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

    const hasOccurredFromParam = searchParams.has("occurredFrom")
    const hasOccurredToParam = searchParams.has("occurredTo")
    const hasActiveFilters = Boolean(
        qParam ||
            warehouseId ||
            (availability !== "all" && view === "balance") ||
            skuId ||
            salesOrderLineId ||
            adjustmentIdParam ||
            movementType.length > 0 ||
            hasOccurredFromParam ||
            hasOccurredToParam,
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
