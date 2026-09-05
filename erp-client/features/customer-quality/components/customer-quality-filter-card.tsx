"use client"

import * as React from "react"

import { FixedOptionRadioFilter } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import type { BusinessTypeFilter, FundsReviewFilter } from "../types"
import type {
    CustomerQualityAppliedChip,
    CustomerQualityFilterKey,
} from "../hooks/use-customer-quality-filters"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const FUNDS_REVIEW_OPTIONS: ReadonlyArray<{
    value: FundsReviewFilter
    label: string
}> = [
    { value: "all", label: "全部授权记录" },
    { value: "reviewed_only", label: "仅已复核卡券票款" },
]

const BUSINESS_TYPE_OPTIONS: ReadonlyArray<{
    value: BusinessTypeFilter | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "VOUCHER", label: "卡券" },
    { value: "GOODS_SERVICE", label: "非卡券" },
]

export type CustomerQualityFilterCardProps = {
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    searchInputRef: React.RefObject<HTMLInputElement | null>
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    appliedChips: readonly CustomerQualityAppliedChip[]
    onRemoveFilter: (key: CustomerQualityFilterKey) => void
    onApplyFilters: () => void
    onClearAllFilters: () => void
    onResetMoreFilters: () => void
    fundsReviewDraft: FundsReviewFilter
    setFundsReviewDraft: SetState<FundsReviewFilter>
    businessTypeDraft: BusinessTypeFilter | "all"
    setBusinessTypeDraft: SetState<BusinessTypeFilter | "all">
    hasPendingChanges: boolean
    resultCount?: number
    loading?: boolean
    failed?: boolean
}

/**
 * 客户经营质量明细筛选工具栏：常用业务性质常驻，票款口径在更多面板。
 * 期间条不属于本表单。
 */
export function CustomerQualityFilterCard({
    searchDraft,
    onSearchDraftChange,
    searchInputRef,
    panelOpen,
    setPanelOpen,
    appliedChips,
    onRemoveFilter,
    onApplyFilters,
    onClearAllFilters,
    onResetMoreFilters,
    fundsReviewDraft,
    setFundsReviewDraft,
    businessTypeDraft,
    setBusinessTypeDraft,
    hasPendingChanges,
    resultCount,
    loading,
    failed,
}: CustomerQualityFilterCardProps) {
    const moreCount = appliedChips.filter(
        (chip) => chip.key === "fundsReview",
    ).length

    return (
        <ListWorkspaceFilterBar
            idPrefix="customers-quality"
            formAriaLabel="客户经营质量查询"
            onSubmit={onApplyFilters}
            search={
                <ListSearchField
                    id="customers-quality-search"
                    searchInputRef={searchInputRef}
                    value={searchDraft}
                    onChange={onSearchDraftChange}
                    placeholder="客户编号 / 名称"
                    aria-label="搜索客户"
                />
            }
            moreCount={moreCount}
            moreOpen={panelOpen}
            onToggleMore={() => setPanelOpen((open) => !open)}
            morePanelId="customers-quality-more-panel"
            morePanelAriaLabel="客户经营质量更多筛选条件"
            moreButtonId="customers-quality-more-filters-trigger"
            resetMoreButtonId="customers-quality-reset-more"
            clearButtonId="customers-quality-clear-all"
            onResetMore={onResetMoreFilters}
            commonFilters={
                <FixedOptionRadioFilter
                    id="customers-quality-business-type"
                    label="业务性质"
                    variant="quiet"
                    value={businessTypeDraft}
                    onValueChange={setBusinessTypeDraft}
                    options={BUSINESS_TYPE_OPTIONS}
                />
            }
            morePanel={
                <FixedOptionRadioFilter
                    id="customers-quality-funds-review"
                    label="票款口径"
                    value={fundsReviewDraft}
                    onValueChange={setFundsReviewDraft}
                    options={FUNDS_REVIEW_OPTIONS}
                />
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "户客户",
                loadingLabel: "正在加载客户…",
            })}
            chips={appliedChips}
            onClearChip={(key) =>
                onRemoveFilter(key as CustomerQualityFilterKey)
            }
            onClearAll={onClearAllFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
            idleHint="导出与当前查询结果一致"
        />
    )
}
