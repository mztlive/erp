"use client"

import * as React from "react"

import { FixedOptionRadioFilter } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    LIFECYCLE_RADIO_FILTER_OPTIONS,
    REVISION_TIMING_RADIO_FILTER_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import type {
    DictionaryFilterKey,
    useLifecycleListFilters,
} from "@/features/master-data/hooks/use-lifecycle-list-filters"

export function DictionaryListToolbar({
    idPrefix,
    searchInputRef,
    filters: f,
    searchPlaceholder,
    countLabel,
    resultCount,
    loading,
    failed,
}: {
    idPrefix?: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    filters: ReturnType<typeof useLifecycleListFilters>
    searchPlaceholder: string
    countLabel: string
    resultCount?: number
    loading: boolean
    failed: boolean
}) {
    const prefix = idPrefix ?? "master-data-list-dictionary-list-toolbar"

    return (
        <ListWorkspaceFilterBar
            idPrefix={prefix}
            formAriaLabel={`${countLabel}查询`}
            onSubmit={f.applyListFilters}
            search={
                <ListSearchField
                    id={`${prefix}-search-input`}
                    searchInputRef={searchInputRef}
                    value={f.searchDraft}
                    onChange={f.setSearchDraft}
                    placeholder={searchPlaceholder}
                    aria-label="搜索基础资料"
                />
            }
            clearButtonId={`${prefix}-clear-filters`}
            commonFilters={
                <>
                    <FixedOptionRadioFilter
                        idPrefix={`${prefix}-lifecycle`}
                        label="启停"
                        variant="quiet"
                        value={f.lifecycleStatusDraft}
                        onValueChange={f.setLifecycleStatusDraft}
                        options={LIFECYCLE_RADIO_FILTER_OPTIONS}
                        aria-label={masterDataCopy.filterLifecycleAria}
                    />
                    <FixedOptionRadioFilter
                        idPrefix={`${prefix}-revision`}
                        label="版本"
                        variant="quiet"
                        value={f.revisionTimingDraft}
                        onValueChange={f.setRevisionTimingDraft}
                        options={REVISION_TIMING_RADIO_FILTER_OPTIONS}
                        aria-label={masterDataCopy.filterVersionAria}
                    />
                </>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "条",
                loadingLabel: `正在加载${countLabel}…`,
            })}
            chips={f.appliedChips}
            onClearChip={(key) => f.removeFilter(key as DictionaryFilterKey)}
            onClearAll={f.clearAllFilters}
            hasPendingChanges={f.hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
            idleHint="导出与当前查询结果一致"
        />
    )
}
