"use client"

import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"

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
    canFilterPeople: boolean
    canFilterWarehouse: boolean
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
    operatorUserIds: string | undefined
    applicantUserIds: string | undefined
    handlerUserIds: string | undefined
    hasActiveFilters: boolean
    patchUrl: LedgerPatchUrl
}

export function useInventoryLedgerUrlState(): InventoryLedgerUrlState {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const profile = useAccountProfileQuery()
    const canFilterPeople = hasPermission(
        profile.data?.permissions,
        "business_person:list",
    )
    const canFilterWarehouse = hasPermission(
        profile.data?.permissions,
        "warehouse:list",
    )
    const unauthorizedKeys = React.useMemo(
        () =>
            [
                ...(!canFilterWarehouse ? ["warehouseId"] : []),
                ...(!canFilterPeople
                    ? ["operatorUserIds", "applicantUserIds", "handlerUserIds"]
                    : []),
            ].filter((key) => searchParams.has(key)),
        [canFilterPeople, canFilterWarehouse, searchParams],
    )

    const view = parseView(searchParams.get("view"))
    const qParam = searchParams.get("q") ?? ""
    const warehouseId = canFilterWarehouse
        ? (searchParams.get("warehouseId") ?? undefined)
        : undefined
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
    const cursorParam = unauthorizedKeys.length
        ? undefined
        : (searchParams.get("cursor") ?? undefined)
    const cursorOffset = decodeInventoryCursor(cursorParam, view)
    const operatorUserIds = canFilterPeople
        ? (searchParams.get("operatorUserIds") ?? undefined)
        : undefined
    const applicantUserIds = canFilterPeople
        ? (searchParams.get("applicantUserIds") ?? undefined)
        : undefined
    const handlerUserIds = canFilterPeople
        ? (searchParams.get("handlerUserIds") ?? undefined)
        : undefined

    const patchUrl = React.useCallback<LedgerPatchUrl>(
        (patch, options) =>
            patchSearchParams(
                { router, pathname, searchParams, view, clearCursor: true },
                {
                    ...patch,
                    ...(!canFilterWarehouse ? { warehouseId: null } : {}),
                    ...(!canFilterPeople
                        ? {
                              operatorUserIds: null,
                              applicantUserIds: null,
                              handlerUserIds: null,
                          }
                        : {}),
                    ...(unauthorizedKeys.length ? { cursor: null } : {}),
                },
                options,
            ),
        [
            canFilterPeople,
            canFilterWarehouse,
            router,
            pathname,
            searchParams,
            unauthorizedKeys,
            view,
        ],
    )

    React.useEffect(() => {
        if (profile.data && unauthorizedKeys.length)
            patchUrl({}, { replace: true, scroll: false })
    }, [profile.data, unauthorizedKeys, patchUrl])

    const hasOccurredFromParam = Boolean(occurredFrom)
    const hasOccurredToParam = Boolean(occurredTo)
    // 视图无关的通用筛选 + 仅在对应视图生效的结构化条件（availability 仅余额视图、
    // 流水类型与发生日期仅流水视图；其它视图下这些参数不被查询消费，不算已生效筛选）。
    const hasActiveFilters = Boolean(
        qParam.trim() ||
        warehouseId ||
        skuId ||
        (view === "reservation" && salesOrderLineId) ||
        (view === "adjustment" && adjustmentIdParam) ||
        (view === "balance" && availability !== "all") ||
        (view === "movement" && movementType.length > 0) ||
        (view === "movement" && (hasOccurredFromParam || hasOccurredToParam)) ||
        (view === "movement" && operatorUserIds) ||
        (view === "adjustment" &&
            (operatorUserIds || applicantUserIds || handlerUserIds)),
    )

    return {
        canFilterPeople,
        canFilterWarehouse,
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
        operatorUserIds,
        applicantUserIds,
        handlerUserIds,
        hasActiveFilters,
        patchUrl,
    }
}
