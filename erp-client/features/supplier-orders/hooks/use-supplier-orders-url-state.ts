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
 * - W25 钻取已随商城移除：supplierOrderId/preview + openCenter=1 时跳转对象中心。
 *
 * 筛选草稿、清除与提交逻辑见 use-supplier-orders-filters.ts。
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

    // 对象中心直达：supplierOrderId/preview + openCenter=1 时进入对象中心
    React.useEffect(() => {
        const soId =
            searchParams.get("supplierOrderId") ?? searchParams.get("preview")
        if (soId && searchParams.get("openCenter") === "1") {
            router.replace(`/supplier-api/orders/${soId}`)
        }
    }, [router, searchParams])

    // P2：筛选/分页/搜索变更统一 replace，不膨胀历史。
    // 原实现整体用 push，意图是「后退可逐步撤销筛选」，与规范冲突已改为 replace；
    // 仅预览打开/关闭这类详情导航保留 push（openPreview/closePreview 传 "push"）。
    const updateUrl = React.useCallback<SupplierOrdersUrlUpdater>(
        (patch: SupplierOrdersUrlPatch, navigate = "replace") => {
            const next = { ...url, ...patch }
            // 默认值不写 URL：aftersalePending=false 是默认值，直接省略
            if (!next.aftersalePending) next.aftersalePending = undefined
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

    return {
        url,
        returnTo,
        updateUrl,
    }
}

export type { SupplierOrdersUrlState }
