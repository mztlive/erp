"use client"

import * as React from "react"

import { FixedOptionRadioFilter } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { LIFECYCLE_RADIO_FILTER_OPTIONS } from "@/features/master-data/lib/list-filters"
import type {
    CategoryTreeAppliedChip,
    CategoryTreeFilterKey,
} from "@/features/master-data/hooks/use-master-data-category-tree"

/** 分类树筛选条：搜索、启停常用筛选与已生效 chip。 */
export function CategoryTreeToolbar({
    idPrefix,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    applyTreeFilters,
    lifecycleStatus,
    onLifecycleStatusChange,
    appliedChips,
    removeFilter,
    clearFilters,
    hasPendingChanges,
    resultCount,
    loading,
    failed,
}: {
    idPrefix?: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: (value: string) => void
    applyTreeFilters: () => void
    lifecycleStatus: "enabled" | "disabled" | "all"
    onLifecycleStatusChange: (value: "enabled" | "disabled" | "all") => void
    appliedChips: readonly CategoryTreeAppliedChip[]
    removeFilter: (key: CategoryTreeFilterKey) => void
    clearFilters: () => void
    hasPendingChanges: boolean
    resultCount?: number
    loading: boolean
    failed: boolean
}) {
    const prefix = idPrefix ?? "master-data-category-tree-toolbar"

    return (
        <div className="border-b border-grid px-3 py-2.5">
            <ListWorkspaceFilterBar
                idPrefix={prefix}
                formAriaLabel="分类树查询"
                onSubmit={applyTreeFilters}
                search={
                    <ListSearchField
                        id={`${prefix}-search`}
                        searchInputRef={searchInputRef}
                        value={searchDraft}
                        onChange={setSearchDraft}
                        placeholder={masterDataCopy.categoryTreeSearch}
                        aria-label={masterDataCopy.categoryTreeSearch}
                    />
                }
                clearButtonId={`${prefix}-clear-all`}
                commonFilters={
                    <FixedOptionRadioFilter
                        id={`${prefix}-lifecycle`}
                        label="启停"
                        variant="quiet"
                        value={lifecycleStatus}
                        onValueChange={onLifecycleStatusChange}
                        options={LIFECYCLE_RADIO_FILTER_OPTIONS}
                        aria-label="生命周期筛选"
                    />
                }
                resultStatus={listWorkspaceFilterStatusText({
                    loading,
                    failed,
                    resultCount,
                    noun: "项",
                    loadingLabel: "正在加载分类…",
                })}
                chips={appliedChips}
                onClearChip={(key) =>
                    removeFilter(key as CategoryTreeFilterKey)
                }
                onClearAll={clearFilters}
                hasPendingChanges={hasPendingChanges}
                pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
                idleHint="导出与当前查询结果一致"
            />
        </div>
    )
}
