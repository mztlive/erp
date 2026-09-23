"use client"

import * as React from "react"

import { FixedOptionRadioFilter } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import type { BusinessTypeFilter } from "../types"
import type {
    CustomerQualityAppliedChip,
    CustomerQualityFilterKey,
} from "../hooks/use-customer-quality-filters"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

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
    appliedChips: readonly CustomerQualityAppliedChip[]
    onRemoveFilter: (key: CustomerQualityFilterKey) => void
    onApplyFilters: () => void
    onClearAllFilters: () => void
    businessTypeDraft: BusinessTypeFilter | "all"
    setBusinessTypeDraft: SetState<BusinessTypeFilter | "all">
    hasPendingChanges: boolean
    resultCount?: number
    loading?: boolean
    failed?: boolean
}

/**
 * 客户经营质量明细筛选：业务性质单独成行。没有低频条件，不提供更多面板。
 * 期间条不属于本表单。
 */
export function CustomerQualityFilterCard({
    searchDraft,
    onSearchDraftChange,
    searchInputRef,
    appliedChips,
    onRemoveFilter,
    onApplyFilters,
    onClearAllFilters,
    businessTypeDraft,
    setBusinessTypeDraft,
    hasPendingChanges,
    resultCount,
    loading,
    failed,
}: CustomerQualityFilterCardProps) {
    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
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
            clearButtonId="customers-quality-clear-all"
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
