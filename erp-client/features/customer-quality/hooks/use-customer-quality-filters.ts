"use client"

import * as React from "react"

import type { BusinessTypeFilter, FundsReviewFilter } from "../types"
import { useCustomerQualitySearch } from "./use-customer-quality-search"
import type { CustomerQualityPatch } from "./use-customer-quality-navigation-state"

/** 可被单独移除的已生效条件。 */
export type CustomerQualityFilterKey =
    | "q"
    | "fundsReview"
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

function hasStructuredFilters(fundsReview: FundsReviewFilter, businessType?: BusinessTypeFilter) {
    return fundsReview === "reviewed_only" || businessType != null
}

/**
 * 客户经营质量明细筛选：三层状态（docs/ui-filter-design.md §5）。
 *
 * - Applied 由 URL 派生（唯一事实源），query / 计数 / 摘要 / 空态只读它；
 * - Draft 为本地受控 state，变化不触发请求；
 * - panelOpen 等 UI 态为本地 state。
 * 提交（Enter / 尾部箭头 / 「应用全部筛选」）共用 applyFilters 一次性写 URL 并收起面板。
 */
export function useCustomerQualityFilters({
    qParam,
    fundsReview,
    businessType,
    customerId,
    customerName,
    patchUrl,
}: {
    qParam: string
    fundsReview: FundsReviewFilter
    businessType?: BusinessTypeFilter
    customerId?: string
    customerName?: string
    patchUrl: CustomerQualityPatch
}) {
    const { searchDraft, setSearchDraft, searchInputRef } =
        useCustomerQualitySearch({ qParam })

    const [fundsReviewDraft, setFundsReviewDraft] =
        React.useState<FundsReviewFilter>(fundsReview)
    const [businessTypeDraft, setBusinessTypeDraft] = React.useState<
        BusinessTypeFilter | "all"
    >(businessType ?? "all")
    // 有结构化条件的初始深链展开面板；URL 回填不重置展开态
    const [panelOpen, setPanelOpen] = React.useState(() =>
        hasStructuredFilters(fundsReview, businessType),
    )

    /** 唯一提交路径：收起态 Enter / 尾部箭头与展开态「应用全部筛选」共用。 */
    const applyFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || null,
            fundsReview:
                fundsReviewDraft === "all" ? null : "reviewed_only",
            businessType:
                businessTypeDraft === "all" ? null : businessTypeDraft,
        })
        setPanelOpen(false)
    }, [businessTypeDraft, fundsReviewDraft, patchUrl, searchDraft])

    /** 移除单个已生效条件（含来源锁定 customerId 与图表筛选）。 */
    const removeFilter = React.useCallback(
        (key: CustomerQualityFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "fundsReview") setFundsReviewDraft("all")
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

    /** 仅清除「更多筛选」；保留关键词，保持面板展开，立即刷新结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setFundsReviewDraft("all")
        setBusinessTypeDraft("all")
        patchUrl({ fundsReview: null, businessType: null })
    }, [patchUrl])

    /** 清除全部筛选：同时重置 Draft、面板、URL 筛选参数与分页；保留排序/期间/导航上下文。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setFundsReviewDraft("all")
        setBusinessTypeDraft("all")
        setPanelOpen(false)
        patchUrl({
            q: null,
            fundsReview: null,
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
        setFundsReviewDraft(fundsReview)
        setBusinessTypeDraft(businessType ?? "all")
    }, [businessType, fundsReview])

    const appliedChips = React.useMemo<readonly CustomerQualityAppliedChip[]>(
        () => {
            const chips: CustomerQualityAppliedChip[] = []
            if (qParam.trim()) {
                chips.push({ key: "q", label: `搜索：${qParam.trim()}` })
            }
            if (fundsReview === "reviewed_only") {
                chips.push({
                    key: "fundsReview",
                    label: "票款口径：仅已复核卡券票款",
                })
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
        },
        [businessType, customerId, customerName, fundsReview, qParam],
    )

    return {
        searchDraft,
        setSearchDraft,
        searchInputRef,
        fundsReviewDraft,
        setFundsReviewDraft,
        businessTypeDraft,
        setBusinessTypeDraft,
        panelOpen,
        setPanelOpen,
        hasStructuredFilters: hasStructuredFilters(fundsReview, businessType),
        applyFilters,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
        appliedChips,
    }
}
