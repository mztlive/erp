"use client"

import * as React from "react"

import { usePurchaseOrdersListUrl } from "@/features/purchase-orders/hooks/use-purchase-orders-list-url"
import {
    PO_METRIC_LABEL,
    PO_STATUS_FILTER_LABEL,
} from "@/features/purchase-orders/types"
import type { PurchaseOrderStatusFilter } from "@/features/purchase-orders/types"

/** 可被单独移除的已生效筛选条件。 */
export type PurchaseOrderFilterKey = "q" | "status" | "metric"

/** 已生效条件的 chip 展示；所有被查询消费的参数都必须显性可见。 */
export type PurchaseOrderAppliedChip = Readonly<{
    key: PurchaseOrderFilterKey
    label: string
}>

/**
 * 采购单列表筛选状态模型（docs/ui-filter-design.md §5）：
 * Applied 在 URL（唯一事实源，查询 / 导出 / 计数只读它），
 * Draft 本地受控不触发请求。
 * 收起态 Enter 与主行「查询」共用 applyFilters 一条提交路径。
 */
export function usePurchaseOrdersListFilters(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const {
        url,
        pushUrl,
        search,
        statusFilter,
        effectiveMetric,
        listReturnHref,
        sortBy,
        sortDir,
        listQueryInput,
        metricKey,
        basisFromUrl,
        salesOrderFromUrl,
        workItemFromUrl,
        createFromSales,
    } = usePurchaseOrdersListUrl()

    const hasStructuredFilters = statusFilter !== "all"
    const hasActiveFilters =
        Boolean(url.q?.trim()) ||
        statusFilter !== "all" ||
        effectiveMetric !== "all"

    const [searchDraft, setSearchDraft] = React.useState(search)
    const [statusDraft, setStatusDraft] =
        React.useState<PurchaseOrderStatusFilter>(statusFilter)

    // URL 回填草稿：正在输入搜索框时不覆盖尚未提交的关键词
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(search)
        }
    }, [search, searchInputRef])

    React.useEffect(() => {
        setStatusDraft(statusFilter)
    }, [statusFilter])

    const applyFilters = React.useCallback(() => {
        pushUrl({
            q: searchDraft.trim() || undefined,
            status: statusDraft,
            // 指标与主状态是同一筛选维度：应用状态时清除指标粗筛，
            // 避免服务端按 metric 静默覆盖用户刚应用的状态
            metric: "all",
            page: 1,
        })
    }, [pushUrl, searchDraft, statusDraft])

    /** 移除单个已生效条件；chip 关闭按钮只移除该条件。 */
    const removeFilter = React.useCallback(
        (key: PurchaseOrderFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "status") setStatusDraft("all")
            pushUrl(
                key === "q"
                    ? { q: undefined, page: 1 }
                    : key === "status"
                      ? { status: "all", page: 1 }
                      : { metric: "all", page: 1 },
            )
        },
        [pushUrl],
    )

    /** 无更多面板，重置为空操作。 */
    const resetMoreFilters = React.useCallback(() => {}, [])

    const hasPendingChanges =
        searchDraft.trim() !== search.trim() || statusDraft !== statusFilter

    /** 清除全部：Draft 与 URL 筛选参数一并重置；排序与导航上下文保留。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setStatusDraft("all")
        pushUrl({ q: undefined, status: "all", metric: "all", page: 1 })
    }, [pushUrl])

    const appliedChips = React.useMemo<
        readonly PurchaseOrderAppliedChip[]
    >(() => {
        const chips: PurchaseOrderAppliedChip[] = []
        const q = url.q?.trim()
        if (q) chips.push({ key: "q", label: `搜索：${q}` })
        if (statusFilter !== "all") {
            chips.push({
                key: "status",
                label: `状态：${PO_STATUS_FILTER_LABEL[statusFilter]}`,
            })
        }
        if (effectiveMetric !== "all") {
            chips.push({
                key: "metric",
                label: `指标：${PO_METRIC_LABEL[url.metric]}`,
            })
        }
        return chips
    }, [effectiveMetric, statusFilter, url.metric, url.q])

    return {
        // URL 派生值：查询 / 导出 / 摘要只读 Applied
        url,
        pushUrl,
        search,
        statusFilter,
        metricKey,
        effectiveMetric,
        listReturnHref,
        sortBy,
        sortDir,
        listQueryInput,
        basisFromUrl,
        salesOrderFromUrl,
        workItemFromUrl,
        createFromSales,
        // 草稿与 UI 态
        searchDraft,
        setSearchDraft,
        statusDraft,
        setStatusDraft,
        hasActiveFilters,
        hasStructuredFilters,
        hasPendingChanges,
        appliedChips,
        // 动作
        removeFilter,
        applyFilters,
        resetMoreFilters,
        clearAllFilters,
    }
}
