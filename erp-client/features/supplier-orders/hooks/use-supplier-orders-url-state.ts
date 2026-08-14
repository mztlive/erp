"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import {
    buildSupplierOrdersSearchParams,
    parseSupplierOrdersSearchParams,
    type SupplierOrdersUrlPatch,
    type SupplierOrdersUrlState,
    type SupplierOrdersUrlUpdater,
} from "@/features/supplier-orders/lib/url-state"

/**
 * 列表页 URL 状态管理：
 * - 解析 searchParams → url（含 returnTo 返回上下文）；
 * - 统一 updateUrl（筛选/分页 replace、预览 push）；
 * - 派生 hasActiveFilters / clearFilters；
 * - W25 钻取：supplierOrderId / from=W25 时跳转对象中心。
 */
export function useSupplierOrdersUrlState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const url = React.useMemo(
        () => parseSupplierOrdersSearchParams(searchParams),
        [searchParams],
    )
    const returnTo = searchParams.get("returnTo") ?? undefined

    // W25 钻取：supplierOrderId / from=W25 时进入对象中心
    React.useEffect(() => {
        const soId =
            searchParams.get("supplierOrderId") ?? searchParams.get("preview")
        const from = searchParams.get("from")
        if (
            soId &&
            (from === "W25" ||
                from === "mall-order" ||
                searchParams.get("openCenter") === "1")
        ) {
            const mall =
                searchParams.get("mallOrderId") ?? searchParams.get("sourceId")
            const qs = new URLSearchParams()
            if (from) qs.set("from", from === "W25" ? "mall-order" : from)
            if (mall) qs.set("sourceId", mall)
            const s = qs.toString()
            router.replace(`/supplier-api/orders/${soId}${s ? `?${s}` : ""}`)
        }
    }, [router, searchParams])

    // P2：筛选/分页/搜索变更统一 replace，不膨胀历史。
    // 原实现整体用 push，意图是「后退可逐步撤销筛选」，与规范冲突已改为 replace；
    // 仅预览打开/关闭这类详情导航保留 push（openPreview/closePreview 传 "push"）。
    const updateUrl = React.useCallback<SupplierOrdersUrlUpdater>(
        (patch: SupplierOrdersUrlPatch, navigate = "replace") => {
            const next = { ...url, ...patch }
            let qs = buildSupplierOrdersSearchParams(next)
            // URL state codec 不声明 returnTo，筛选/分页变化时手动保留返回上下文
            if (returnTo) {
                qs += `${qs ? "&" : "?"}returnTo=${encodeURIComponent(returnTo)}`
            }
            if (navigate === "push") {
                router.push(`${pathname}${qs}`, { scroll: false })
            } else {
                router.replace(`${pathname}${qs}`, { scroll: false })
            }
        },
        [pathname, router, url, returnTo],
    )

    // P4：清除=清全部筛选参数并回第 1 页；保留 view（视图类）与排序。
    // 工具栏常驻清除与空态 BusinessEmptyState 共用同一函数（D21）。
    const hasActiveFilters = Boolean(
        url.q ||
        url.supplierId ||
        url.fulfillmentStatuses?.length ||
        url.cancelStatuses?.length ||
        url.refundStatuses?.length ||
        url.aftersalePending ||
        url.paidFrom ||
        url.paidTo,
    )
    const clearFilters = React.useCallback(() => {
        updateUrl({
            fulfillmentStatuses: undefined,
            cancelStatuses: undefined,
            refundStatuses: undefined,
            aftersalePending: false,
            q: undefined,
            supplierId: undefined,
            paidFrom: undefined,
            paidTo: undefined,
            page: 1,
        })
    }, [updateUrl])

    return {
        url,
        returnTo,
        updateUrl,
        hasActiveFilters,
        clearFilters,
    }
}

export type { SupplierOrdersUrlState }
