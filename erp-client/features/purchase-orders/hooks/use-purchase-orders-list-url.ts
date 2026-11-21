"use client"

import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"

import type { PurchaseOrderListQuery } from "@/features/purchase-orders/api/purchase-orders"
import {
    buildPurchaseOrdersSearchParams,
    parsePurchaseOrdersSearchParams,
    type PurchaseOrdersUrlState,
} from "@/features/purchase-orders/lib/url-state"

/**
 * 采购单列表 URL 状态：解析查询参数、产出查询输入与筛选派生值，
 * 所有写操作都经 router.replace 反映到地址栏。
 */
export function usePurchaseOrdersListUrl() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const profile = useAccountProfileQuery()
    const canFilterOwner =
        !profile.isError &&
        hasPermission(profile.data?.permissions, "procurement_person:list")
    const hasOwnerFilter = searchParams.has("ownerUserIds")
    const hasUnauthorizedFilter = !canFilterOwner && hasOwnerFilter
    const waitingForOwnerPermission = hasOwnerFilter && profile.isPending
    const url = React.useMemo(() => {
        const values = parsePurchaseOrdersSearchParams(searchParams)
        return hasUnauthorizedFilter
            ? { ...values, ownerUserIds: undefined, page: 1 }
            : values
    }, [hasUnauthorizedFilter, searchParams])

    const pushUrl = React.useCallback(
        (patch: Partial<PurchaseOrdersUrlState>) => {
            const next = { ...url, ...patch }
            if (!canFilterOwner) next.ownerUserIds = undefined
            router.replace(
                `${pathname}${buildPurchaseOrdersSearchParams(next)}`,
                { scroll: false },
            )
        },
        [canFilterOwner, pathname, router, url],
    )
    React.useEffect(() => {
        if (profile.isSuccess && hasUnauthorizedFilter) pushUrl({ page: 1 })
    }, [profile.isSuccess, hasUnauthorizedFilter, pushUrl])

    // 跨单据跳转的返回目标：保留销售单关系筛选；建单参数带回去会再次进入建单页。
    const listReturnHref = React.useMemo(() => {
        const sp = new URLSearchParams(searchParams.toString())
        sp.delete("basisId")
        sp.delete("action")
        sp.delete("mode")
        if (hasUnauthorizedFilter) {
            sp.delete("ownerUserIds")
            sp.delete("page")
        }
        const qs = sp.toString()
        return qs ? `${pathname}?${qs}` : pathname
    }, [hasUnauthorizedFilter, pathname, searchParams])

    const [sortBy, sortDir] = React.useMemo(() => {
        const [id, dir] = (url.sort ?? "").split(":")
        if (!id || (dir !== "asc" && dir !== "desc")) {
            return [undefined, undefined] as const
        }
        return [id, dir] as const
    }, [url.sort])

    const listQueryInput = React.useMemo<PurchaseOrderListQuery>(
        () => ({
            q: url.q,
            purchaseType: url.purchaseType,
            ownerUserIds: url.ownerUserIds,
            salesOrderId: url.salesOrderId,
            status: url.status,
            metric: url.metric,
            page: url.page,
            pageSize: url.pageSize,
            sortBy,
            sortDir,
        }),
        [sortBy, sortDir, url],
    )

    return {
        canFilterOwner,
        waitingForOwnerPermission,
        url,
        pushUrl,
        listReturnHref,
        sortBy,
        sortDir,
        listQueryInput,
        search: url.q ?? "",
        statusFilter: url.status,
        salesOrderFromUrl: url.salesOrderId ?? null,
        workItemFromUrl: url.workItemId ?? null,
    }
}
