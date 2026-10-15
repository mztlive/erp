"use client"

import * as React from "react"
import { PURCHASE_TYPE_LABEL, type PurchaseType } from "../types"

import { usePurchaseOrdersListUrl } from "@/features/purchase-orders/hooks/use-purchase-orders-list-url"

/** 可被单独移除的已生效筛选条件。 */
export type PurchaseOrderFilterKey = "q" | "ownerUserIds" | "purchaseType"

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

    const hasStructuredFilters =
        statusFilter !== "all" ||
        Boolean(url.ownerUserIds) ||
        Boolean(url.purchaseType)
    const hasActiveFilters =
        Boolean(url.purchaseType) ||
        Boolean(url.q?.trim()) ||
        Boolean(url.ownerUserIds) ||
        statusFilter !== "all" ||
        effectiveMetric !== "all"

    const [typeDraft, setTypeDraft] = React.useState<PurchaseType | "all">(
        url.purchaseType ?? "all",
    )
    React.useEffect(
        () => setTypeDraft(url.purchaseType ?? "all"),
        [url.purchaseType],
    )

    const [ownerDraft, setOwnerDraft] = React.useState(url.ownerUserIds ?? "")
    React.useEffect(
        () => setOwnerDraft(url.ownerUserIds ?? ""),
        [url.ownerUserIds],
    )

    const [searchDraft, setSearchDraft] = React.useState(search)

    // URL 回填草稿：正在输入搜索框时不覆盖尚未提交的关键词
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(search)
        }
    }, [search, searchInputRef])

    const applyFilters = React.useCallback(() => {
        pushUrl({
            q: searchDraft.trim() || undefined,
            purchaseType: typeDraft === "all" ? undefined : typeDraft,
            ownerUserIds: ownerDraft || undefined,
            page: 1,
        })
    }, [pushUrl, searchDraft, ownerDraft, typeDraft])

    /** 移除单个已生效条件；chip 关闭按钮只移除该条件。 */
    const removeFilter = React.useCallback(
        (key: PurchaseOrderFilterKey) => {
            if (key === "purchaseType") {
                setTypeDraft("all")
                pushUrl({ purchaseType: undefined, page: 1 })
                return
            }
            if (key === "ownerUserIds") {
                setOwnerDraft("")
                pushUrl({ ownerUserIds: undefined, page: 1 })
                return
            }
            if (key === "q") setSearchDraft("")
            pushUrl({ q: undefined, page: 1 })
        },
        [pushUrl],
    )

    /** 无更多面板，重置为空操作。 */
    const resetMoreFilters = React.useCallback(() => {}, [])

    const hasPendingChanges =
        typeDraft !== (url.purchaseType ?? "all") ||
        searchDraft.trim() !== search.trim() ||
        ownerDraft !== (url.ownerUserIds ?? "")

    /** 清除全部：Draft 与 URL 筛选参数一并重置；排序与导航上下文保留。 */
    const clearAllFilters = React.useCallback(() => {
        setTypeDraft("all")
        setSearchDraft("")
        setOwnerDraft("")
        pushUrl({
            purchaseType: undefined,
            q: undefined,
            ownerUserIds: undefined,
            status: "all",
            metric: "all",
            page: 1,
        })
    }, [pushUrl])

    const appliedChips = React.useMemo<
        readonly PurchaseOrderAppliedChip[]
    >(() => {
        const chips: PurchaseOrderAppliedChip[] = []
        if (url.purchaseType)
            chips.push({
                key: "purchaseType",
                label: `采购类型：${PURCHASE_TYPE_LABEL[url.purchaseType]}`,
            })
        if (url.ownerUserIds)
            chips.push({
                key: "ownerUserIds",
                label: `采购负责人：已选 ${url.ownerUserIds.split(",").length} 人`,
            })
        const q = url.q?.trim()
        if (q) chips.push({ key: "q", label: `搜索：${q}` })
        return chips
    }, [url.q, url.ownerUserIds, url.purchaseType])

    return {
        typeDraft,
        setTypeDraft,
        ownerDraft,
        setOwnerDraft,
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
