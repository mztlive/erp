"use client"

import * as React from "react"
import { useRouter } from "@/lib/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    usePurchaseOrderExportDataQuery,
    usePurchaseOrdersQuery,
} from "@/features/purchase-orders/hooks/queries"
import { usePurchaseOrdersListFilters } from "@/features/purchase-orders/hooks/use-purchase-orders-list-filters"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { downloadTransfer } from "@/lib/data-transfer"
import { getErrorMessage } from "@/lib/api/errors"

export type PurchaseOrdersActionResult = {
    status: "succeeded" | "failed" | "unknown"
    title: string
    description: string
    reference?: string
}

/**
 * 采购单列表页控制器：URL 状态、查询接线、筛选状态模型（Applied/Draft/UI）、
 * 搜索快捷键与导出/进入建单页的状态逻辑。
 */
export function usePurchaseOrdersListController() {
    const router = useRouter()
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const filters = usePurchaseOrdersListFilters(searchInputRef)
    const {
        url,
        pushUrl,
        listReturnHref,
        sortBy,
        sortDir,
        listQueryInput,
        statusFilter,
        salesOrderFromUrl,
        workItemFromUrl,
    } = filters

    const listQuery = usePurchaseOrdersQuery(listQueryInput, {
        enabled: !filters.waitingForOwnerPermission,
    })
    const exportQuery = usePurchaseOrderExportDataQuery(listQueryInput)

    const pageRows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data],
    )
    const total = listQuery.data?.total ?? 0

    const [actionResult, setActionResult] =
        React.useState<PurchaseOrdersActionResult | null>(null)

    const pagination = React.useMemo<PaginationState>(
        () => ({
            pageIndex: Math.max(0, url.page - 1),
            pageSize: url.pageSize,
        }),
        [url.page, url.pageSize],
    )

    const sorting = React.useMemo<SortingState>(
        () =>
            (sortBy === "document" || sortBy === "purchase_no") && sortDir
                ? [{ id: "document", desc: sortDir === "desc" }]
                : [],
        [sortBy, sortDir],
    )

    React.useEffect(() => {
        const data = listQuery.data
        if (!data || data.page === url.page) return
        pushUrl({ page: data.page })
    }, [listQuery.data, pushUrl, url.page])

    useSearchHotkey(searchInputRef)

    const openDetail = React.useCallback(
        (purchaseOrderId: string) => {
            router.push(`/procurement/orders/${purchaseOrderId}`)
        },
        [router],
    )

    const exportCsv = React.useCallback(async () => {
        try {
            const result = await exportQuery.mutateAsync()
            downloadTransfer(result)
            setActionResult({
                status: "succeeded",
                title: "导出已生成",
                description: "已下载当前筛选结果，可在后台任务页再次下载。",
                reference: result.jobId,
            })
        } catch (error) {
            setActionResult({
                status: "failed",
                title: "导出失败",
                description: getErrorMessage(error, "请查看原任务后重试"),
            })
        }
    }, [exportQuery])

    const openCreatePage = React.useCallback(() => {
        const params = new URLSearchParams()
        params.set("mode", "create")
        if (salesOrderFromUrl) params.set("salesOrderId", salesOrderFromUrl)
        if (workItemFromUrl) params.set("workItemId", workItemFromUrl)
        router.push(`/procurement/orders?${params.toString()}`)
    }, [router, salesOrderFromUrl, workItemFromUrl])

    return {
        searchInputRef,
        filters,
        // 查询与派生数据
        listQuery,
        exportQuery,
        pageRows,
        total,
        pagination,
        sorting,
        // URL 状态
        pushUrl,
        listReturnHref,
        statusFilter,
        salesOrderFromUrl,
        // 交互状态
        actionResult,
        setActionResult,
        // 动作
        exportCsv,
        openCreatePage,
        openDetail,
    }
}
