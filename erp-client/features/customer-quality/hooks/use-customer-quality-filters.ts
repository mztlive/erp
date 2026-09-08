"use client"

import * as React from "react"

import type { BusinessTypeFilter } from "../types"
import { useCustomerQualitySearch } from "./use-customer-quality-search"
import type { CustomerQualityPatch } from "./use-customer-quality-navigation-state"

/** 可被单独移除的已生效条件。 */
export type CustomerQualityFilterKey =
    | "q"
    | "businessType"
    | "customerId"
    | "chart"

export type CustomerQualityAppliedChip = Readonly<{
    key: CustomerQualityFilterKey
    label: string
}>

const BUSINESS_TYPE_LABELS: Record<BusinessTypeFilter, string> = {
    VOUCHER: "卡券",
    GOODS_SERVICE: "非卡券",
}

function hasStructuredFilters(businessType?: BusinessTypeFilter) {
    return businessType != null
}

/**
 * 客户经营质量明细筛选：三层状态（docs/ui-filter-design.md §5）。
 *
 * - Applied 由 URL 派生（唯一事实源），query / 计数 / 摘要 / 空态只读它；
 * - Draft 为本地受控 state，变化不触发请求；
 * 提交（Enter / 「查询」）共用 applyFilters 一次性写 URL。
 */
export function useCustomerQualityFilters({
    qParam,
    businessType,
    customerId,
    customerName,
    patchUrl,
}: {
    qParam: string
    businessType?: BusinessTypeFilter
    customerId?: string
    customerName?: string
    patchUrl: CustomerQualityPatch
}) {
    const { searchDraft, setSearchDraft, searchInputRef } =
        useCustomerQualitySearch({ qParam })

    const [businessTypeDraft, setBusinessTypeDraft] = React.useState<
        BusinessTypeFilter | "all"
    >(businessType ?? "all")

    /** 唯一提交路径：Enter 与「查询」共用。 */
    const applyFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || null,
            businessType:
                businessTypeDraft === "all" ? null : businessTypeDraft,
        })
    }, [businessTypeDraft, patchUrl, searchDraft])

    /** 移除单个已生效条件（含来源锁定 customerId 与图表筛选）。 */
    const removeFilter = React.useCallback(
        (key: CustomerQualityFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "businessType") setBusinessTypeDraft("all")
            if (key === "chart") {
                patchUrl({
                    chartDimension: null,
                    chartCode: null,
                    scaleTag: null,
                    profitTag: null,
                    riskTag: null,
                })
                return
            }
            patchUrl({ [key]: null })
        },
        [patchUrl, setSearchDraft],
    )

    /** 清除全部筛选：同时重置 Draft、面板、URL 筛选参数与分页；保留排序/期间/导航上下文。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setBusinessTypeDraft("all")
        patchUrl({
            q: null,
            businessType: null,
            customerId: null,
            scaleTag: null,
            profitTag: null,
            riskTag: null,
            chartDimension: null,
            chartCode: null,
            focusMetric: null,
        })
    }, [patchUrl, setSearchDraft])

    // URL → Draft 回填（面板展开态不回填重置）
    React.useEffect(() => {
        setBusinessTypeDraft(businessType ?? "all")
    }, [businessType])

    const appliedChips = React.useMemo<
        readonly CustomerQualityAppliedChip[]
    >(() => {
        const chips: CustomerQualityAppliedChip[] = []
        if (qParam.trim()) {
            chips.push({ key: "q", label: `搜索：${qParam.trim()}` })
        }
        if (businessType) {
            chips.push({
                key: "businessType",
                label: `业务性质：${BUSINESS_TYPE_LABELS[businessType]}`,
            })
        }
        if (customerId) {
            chips.push({
                key: "customerId",
                label: `客户：${customerName ?? "已定位客户"}`,
            })
        }
        return chips
    }, [businessType, customerId, customerName, qParam])

    const hasPendingChanges =
        searchDraft.trim() !== qParam.trim() ||
        businessTypeDraft !== (businessType ?? "all")

    return {
        searchDraft,
        setSearchDraft,
        searchInputRef,
        businessTypeDraft,
        setBusinessTypeDraft,
        hasStructuredFilters: hasStructuredFilters(businessType),
        hasPendingChanges,
        applyFilters,
        removeFilter,
        clearAllFilters,
        appliedChips,
    }
}
