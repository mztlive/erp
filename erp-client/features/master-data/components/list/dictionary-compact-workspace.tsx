"use client"

import type * as React from "react"

import { ListWorkSurface } from "@/components/business/list-workspace"
import { DictionaryCompactList } from "@/features/master-data/components/list/dictionary-compact-list"
import { DictionaryListToolbar } from "@/features/master-data/components/list/dictionary-list-toolbar"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import type { useLifecycleListFilters } from "@/features/master-data/hooks/use-lifecycle-list-filters"
import type { MasterDataListItem } from "@/features/master-data/types"

type DictionaryFilters = ReturnType<typeof useLifecycleListFilters>

/**
 * 品牌、计量单位、卡券类目共用的紧凑列表工作区。
 * 文案、稳定 id 和是否包一层全宽容器仍由各页面传入。
 */
export function DictionaryCompactWorkspace({
    listId,
    ariaLabel,
    fullWidth = false,
    metricsAriaLabel,
    allLabel,
    hint,
    countLabel,
    searchPlaceholder,
    showRevisionFilter = true,
    codeLabel,
    emptyTitle,
    emptyDescription,
    emptyAction,
    renderCard,
    searchInputRef,
    filters,
    syncedMetrics,
    rows,
    previewId,
    fetching,
    failed,
    hasData,
    error,
    onRetry,
    onPreview,
}: {
    listId: string
    ariaLabel: string
    fullWidth?: boolean
    metricsAriaLabel: string
    allLabel: string
    hint: string
    countLabel: string
    searchPlaceholder: string
    showRevisionFilter?: boolean
    codeLabel: string
    emptyTitle: string
    emptyDescription?: string
    emptyAction?: React.ReactNode
    renderCard?: (row: MasterDataListItem) => React.ReactNode
    searchInputRef: React.RefObject<HTMLInputElement | null>
    filters: DictionaryFilters
    syncedMetrics: readonly {
        key: string
        label: string
        value: number
        detail?: string
    }[]
    rows: readonly MasterDataListItem[]
    previewId: string | null
    fetching: boolean
    failed: boolean
    hasData: boolean
    error: unknown
    onRetry: () => void
    onPreview: (row: MasterDataListItem) => void
}) {
    const hasActiveFilters =
        filters.q.trim() !== "" ||
        filters.lifecycleStatus !== "all" ||
        filters.revisionTiming !== "all"
    const surface = (
        <ListWorkSurface
            ariaLabel={ariaLabel}
            views={
                <LifecycleMetricStrip
                    idPrefix={`${listId}-metrics`}
                    metrics={syncedMetrics}
                    metricKey={filters.metricKey}
                    ariaLabel={metricsAriaLabel}
                    allLabel={allLabel}
                    hint={hint}
                    onChangeLifecycle={filters.changeLifecycle}
                />
            }
            toolbar={
                <DictionaryListToolbar
                    idPrefix={`${listId}-toolbar`}
                    searchInputRef={searchInputRef}
                    filters={filters}
                    showRevisionFilter={showRevisionFilter}
                    searchPlaceholder={searchPlaceholder}
                    countLabel={countLabel}
                    resultCount={hasData ? rows.length : undefined}
                    loading={fetching}
                    failed={failed}
                />
            }
            table={
                <DictionaryCompactList
                    id={listId}
                    rows={rows}
                    codeLabel={codeLabel}
                    renderCard={renderCard}
                    selectedId={previewId}
                    pagination={filters.pagination}
                    onPaginationChange={filters.changePagination}
                    loading={fetching}
                    listLoadFailed={failed || !hasData}
                    error={error}
                    onRetry={onRetry}
                    hasActiveFilters={hasActiveFilters}
                    onClearFilters={filters.clearAllFilters}
                    emptyTitle={emptyTitle}
                    emptyDescription={emptyDescription}
                    emptyAction={emptyAction}
                    onPreview={onPreview}
                />
            }
        />
    )
    return fullWidth ? <div className="w-full">{surface}</div> : surface
}
