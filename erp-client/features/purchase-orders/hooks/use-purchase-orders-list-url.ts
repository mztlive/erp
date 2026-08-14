"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

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

    const url = React.useMemo(
        () => parsePurchaseOrdersSearchParams(searchParams),
        [searchParams],
    )

    const pushUrl = React.useCallback(
        (patch: Partial<PurchaseOrdersUrlState>) => {
            const next = { ...url, ...patch }
            router.replace(
                `${pathname}${buildPurchaseOrdersSearchParams(next)}`,
                { scroll: false },
            )
        },
        [pathname, router, url],
    )

    // 跨单据跳转的返回目标：当前列表（保留筛选）。basisId 只服务于建单 Dialog，
    // 带回去会在返回时误弹建单框，故剔除。
    const listReturnHref = React.useMemo(() => {
        const sp = new URLSearchParams(searchParams.toString())
        sp.delete("basisId")
        const qs = sp.toString()
        return qs ? `${pathname}?${qs}` : pathname
    }, [pathname, searchParams])

    const [sortBy, sortDir] = React.useMemo(() => {
        const [id, dir] = (url.sort ?? "").split(":")
        if (!id || (dir !== "asc" && dir !== "desc")) {
            return [undefined, undefined] as const
        }
        return [id, dir] as const
    }, [url.sort])

    // 「可建单依据」是动作卡不是筛选卡：URL 带该值时按全部列表处理。
    // metric=pending_create 只由建单入口携带，指标条上无对应高亮控件（其它分支的
    // metricKey 均为有控件的高亮枚举值，按原值消费即可，无需额外分支处理）。
    const effectiveMetric = url.metric === "pending_create" ? "all" : url.metric

    const listQueryInput = React.useMemo<PurchaseOrderListQuery>(
        () => ({
            q: url.q,
            status: url.status,
            metric: effectiveMetric,
            page: url.page,
            pageSize: url.pageSize,
            sortBy,
            sortDir,
        }),
        [effectiveMetric, sortBy, sortDir, url],
    )

    return {
        router,
        url,
        pushUrl,
        listReturnHref,
        sortBy,
        sortDir,
        effectiveMetric,
        listQueryInput,
        search: url.q ?? "",
        statusFilter: url.status,
        metricKey: url.metric,
        basisFromUrl: url.basisId ?? null,
    }
}
