"use client"

import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"

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

    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions
    const canFilterSupplier = hasPermission(permissions, "supplier:list")
    const canFilterPeople = hasPermission(permissions, "business_person:list")
    const canFilterOrg = hasPermission(permissions, "org_unit:list")
    const sanitizeFilters = React.useCallback(
        (values: SupplierOrdersUrlState): SupplierOrdersUrlState => ({
            ...values,
            supplierId: canFilterSupplier ? values.supplierId : undefined,
            ownerUserIds: canFilterPeople ? values.ownerUserIds : undefined,
            handlerUserIds: canFilterPeople ? values.handlerUserIds : undefined,
            orgUnitIds: canFilterOrg ? values.orgUnitIds : undefined,
            includeDescendants:
                canFilterOrg && values.orgUnitIds
                    ? values.includeDescendants
                    : undefined,
        }),
        [canFilterSupplier, canFilterPeople, canFilterOrg],
    )
    const unauthorizedKeys = React.useMemo(
        () =>
            [
                ...(!canFilterSupplier ? ["supplierId"] : []),
                ...(!canFilterPeople ? ["ownerUserIds", "handlerUserIds"] : []),
                ...(!canFilterOrg ? ["orgUnitIds", "includeDescendants"] : []),
            ].filter((key) => searchParams.has(key)),
        [canFilterSupplier, canFilterPeople, canFilterOrg, searchParams],
    )
    const url = React.useMemo(() => {
        const next = sanitizeFilters(
            parseSupplierOrdersSearchParams(searchParams),
        )
        return unauthorizedKeys.length ? { ...next, page: 1 } : next
    }, [sanitizeFilters, searchParams, unauthorizedKeys])
    const returnTo = searchParams.get("returnTo") ?? undefined

    React.useEffect(() => {
        const unauthorized = profile.data ? unauthorizedKeys : []
        if (
            !unauthorized.length &&
            !searchParams.has("paidFrom") &&
            !searchParams.has("paidTo")
        )
            return
        const target =
            searchParams.get("supplierOrderId") ?? searchParams.get("preview")
        if (target && searchParams.get("openCenter") === "1") return
        const clean = new URLSearchParams(searchParams.toString())
        for (const key of [...unauthorized, "paidFrom", "paidTo"])
            clean.delete(key)
        if (unauthorized.length) clean.delete("page")
        const query = clean.toString()
        router.replace(`${pathname}${query ? `?${query}` : ""}`, {
            scroll: false,
        })
    }, [pathname, profile.data, router, searchParams, unauthorizedKeys])

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
            const next = sanitizeFilters({ ...url, ...patch })
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
        [pathname, router, url, returnTo, sanitizeFilters],
    )

    return {
        canFilterSupplier,
        canFilterPeople,
        canFilterOrg,
        url,
        returnTo,
        updateUrl,
    }
}

export type { SupplierOrdersUrlState }
