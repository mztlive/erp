"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    usePurchaseOrderExportDataQuery,
    usePurchaseOrdersQuery,
} from "@/features/purchase-orders/hooks/queries"
import { usePurchaseOrdersListFilters } from "@/features/purchase-orders/hooks/use-purchase-orders-list-filters"
import { usePurchaseOrdersListKeyboard } from "@/features/purchase-orders/hooks/use-purchase-orders-list-keyboard"
import { buildPurchaseOrdersCsv } from "@/features/purchase-orders/lib/purchase-orders-list-helpers"

export type PurchaseOrdersActionResult = {
    status: "succeeded" | "failed" | "unknown"
    title: string
    description: string
    reference?: string
}

/**
 * 采购单列表页控制器：URL 状态、查询接线、筛选状态模型（Applied/Draft/UI）、
 * 键盘导航与导出/进入建单页的状态逻辑。
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
        effectiveMetric,
        listQueryInput,
        search,
        statusFilter,
        metricKey,
        salesOrderFromUrl,
        workItemFromUrl,
    } = filters

    const listQuery = usePurchaseOrdersQuery(listQueryInput)
    const exportQuery = usePurchaseOrderExportDataQuery(listQueryInput)

    const pageRows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data],
    )
    const total = listQuery.data?.total ?? 0

    const [focusedIndex, setFocusedIndex] = React.useState(0)
    const [actionResult, setActionResult] =
        React.useState<PurchaseOrdersActionResult | null>(null)

    const rowRefs = React.useRef<Map<string, HTMLElement>>(new Map())

    const pagination = React.useMemo<PaginationState>(
        () => ({
            pageIndex: Math.max(0, url.page - 1),
            pageSize: url.pageSize,
        }),
        [url.page, url.pageSize],
    )

    const sorting = React.useMemo<SortingState>(
        () =>
            sortBy && sortDir ? [{ id: sortBy, desc: sortDir === "desc" }] : [],
        [sortBy, sortDir],
    )

    React.useEffect(() => {
        setFocusedIndex(0)
    }, [metricKey, pageRows.length, search, statusFilter])

    React.useEffect(() => {
        const data = listQuery.data
        if (!data || data.page === url.page) return
        pushUrl({ page: data.page })
    }, [listQuery.data, pushUrl, url.page])

    // 键盘导航：仅列表可见且建单弹层未打开时生效；焦点行滚动到可视区。
    React.useEffect(() => {
        const focusedRow = pageRows[focusedIndex]
        if (!focusedRow) return
        rowRefs.current.get(focusedRow.purchaseOrderId)?.scrollIntoView({
            block: "nearest",
        })
    }, [focusedIndex, pageRows])

    const openDetail = React.useCallback(
        (purchaseOrderId: string) => {
            router.push(`/procurement/orders/${purchaseOrderId}`)
        },
        [router],
    )

    usePurchaseOrdersListKeyboard({
        pageRows,
        focusedIndex,
        createOpen: false,
        onFocusIndex: setFocusedIndex,
        onOpenDetail: openDetail,
    })

    const exportCsv = React.useCallback(async () => {
        const result = await exportQuery.refetch()
        const rows = result.data ?? []
        if (rows.length === 0) return
        const csv = buildPurchaseOrdersCsv(rows)
        const objectUrl = URL.createObjectURL(
            new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" }),
        )
        const anchor = document.createElement("a")
        anchor.href = objectUrl
        anchor.download = "采购单列表.csv"
        anchor.click()
        URL.revokeObjectURL(objectUrl)
        setActionResult({
            status: "succeeded",
            title: "导出已生成",
            description: `已下载当前筛选 ${rows.length} 条。`,
            reference: `EXPORT-${rows.length}`,
        })
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
        filters: {
            searchDraft: filters.searchDraft,
            setSearchDraft: filters.setSearchDraft,
            statusDraft: filters.statusDraft,
            setStatusDraft: filters.setStatusDraft,
            panelOpen: filters.panelOpen,
            setPanelOpen: filters.setPanelOpen,
            hasActiveFilters: filters.hasActiveFilters,
            hasStructuredFilters: filters.hasStructuredFilters,
            appliedChips: filters.appliedChips,
            removeFilter: filters.removeFilter,
            applyFilters: filters.applyFilters,
            resetMoreFilters: filters.resetMoreFilters,
            clearAllFilters: filters.clearAllFilters,
        },
        // 查询与派生数据
        listQuery,
        exportQuery,
        pageRows,
        total,
        pagination,
        sorting,
        // URL 状态
        url,
        pushUrl,
        listReturnHref,
        search,
        statusFilter,
        metricKey,
        effectiveMetric,
        salesOrderFromUrl,
        // 交互状态
        focusedIndex,
        setFocusedIndex,
        actionResult,
        setActionResult,
        rowRefs,
        // 动作
        exportCsv,
        openCreatePage,
        openDetail,
    }
}
