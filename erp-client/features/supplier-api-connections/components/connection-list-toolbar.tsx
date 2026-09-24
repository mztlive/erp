"use client"

import * as React from "react"

import { MultiOptionCombobox, OptionCombobox } from "@/components/business"
import { SelectorQueryFeedback } from "@/components/business/selector-query-feedback"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"
import { useSupplierSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import type {
    ConnectionAppliedChip,
    ConnectionFilterKey,
} from "@/features/supplier-api-connections/hooks/use-connection-list-filters"
import {
    CAPABILITY_LABEL,
    CATALOG_LABEL,
    ENVIRONMENT_LABEL,
    HEALTH_LABEL,
    type CapabilityCode,
    type CatalogFreshnessState,
    type ConnectionEnvironment,
    type HealthResult,
} from "@/features/supplier-api-connections/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const ENVIRONMENT_FILTER_OPTIONS: ReadonlyArray<{
    value: ConnectionEnvironment | "ALL"
    label: string
}> = [
    { value: "ALL", label: "全部" },
    { value: "PRODUCTION", label: ENVIRONMENT_LABEL.PRODUCTION },
    { value: "STAGING", label: ENVIRONMENT_LABEL.STAGING },
    { value: "DEVELOPMENT", label: ENVIRONMENT_LABEL.DEVELOPMENT },
]

const CAPABILITY_FILTER_OPTIONS: ReadonlyArray<{
    value: CapabilityCode
    label: string
}> = (Object.keys(CAPABILITY_LABEL) as CapabilityCode[]).map((code) => ({
    value: code,
    label: CAPABILITY_LABEL[code],
}))

const HEALTH_FILTER_OPTIONS: ReadonlyArray<{
    value: HealthResult
    label: string
}> = (Object.keys(HEALTH_LABEL) as HealthResult[]).map((value) => ({
    value,
    label: HEALTH_LABEL[value],
}))

const CATALOG_FRESHNESS_FILTER_OPTIONS: ReadonlyArray<{
    value: CatalogFreshnessState
    label: string
}> = (Object.keys(CATALOG_LABEL) as CatalogFreshnessState[]).map((value) => ({
    value,
    label: CATALOG_LABEL[value],
}))

const MORE_CHIP_KEYS: readonly ConnectionFilterKey[] = [
    "health",
    "capability",
    "catalogFreshness",
]

function ResidentSupplierFilter({
    id,
    value,
    onValueChange,
}: {
    id: string
    value: string | null
    onValueChange: (value: string | null) => void
}) {
    const search = useSearchInput()
    const query = useSupplierSelectorQuery(
        { query: search.input, purpose: "filter" },
        value ?? undefined,
    )
    const { rows, loading, emptyLabel } = useRemoteSearchCombobox({
        selectedId: value ?? undefined,
        list: query.list,
        selected: query.selected,
        idOf: (item) => item.supplierId,
        fallbackError: "供应商加载失败，请重试",
    })
    const directoryFailed = query.list.isError || query.selected.isError
    return (
        <div className="min-w-0">
            <OptionCombobox
                id={id}
                className="w-56 max-w-full min-w-0"
                filterLabel="供应商"
                aria-label="供应商"
                placeholder="全部"
                searchPlaceholder="搜索供应商名称或编码"
                filterMode="remote"
                onSearchChange={search.onSearchChange}
                loading={loading}
                emptyLabel={emptyLabel}
                value={value}
                onValueChange={onValueChange}
                options={rows.map((item) => ({
                    value: item.supplierId,
                    label: item.supplierName,
                    keywords: item.supplierCode,
                }))}
            />
            <SelectorQueryFeedback
                id={id}
                failed={directoryFailed}
                error={query.list.error ?? query.selected.error}
                noScope={
                    !query.list.isFetching &&
                    !directoryFailed &&
                    query.list.emptyReason === "no_scope"
                }
                onRetry={() => {
                    void query.list.refetch()
                    if (value) void query.selected.refetch()
                }}
            />
        </div>
    )
}

export type ConnectionListToolbarProps = {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    onSearchDraftChange: SetState<string>
    environment: ConnectionEnvironment | "ALL"
    onEnvironmentChange: (value: ConnectionEnvironment | "ALL") => void
    filterPanelOpen: boolean
    onFilterPanelOpenChange: SetState<boolean>
    appliedChips: readonly ConnectionAppliedChip[]
    removeFilter: (key: ConnectionFilterKey) => void
    onApplyFilters: () => void
    onClearFilters: () => void
    onResetMoreFilters: () => void
    onCancelMoreFilters: () => void
    healthDraft: readonly string[]
    onHealthDraftChange: SetState<string[]>
    capabilityDraft: string
    onCapabilityDraftChange: SetState<string>
    catalogFreshnessDraft: readonly string[]
    onCatalogFreshnessDraftChange: SetState<string[]>
    supplierIdDraft: string | null
    onSupplierIdDraftChange: SetState<string | null>
    hasPendingChanges?: boolean
    resultCount?: number
    loading?: boolean
    failed?: boolean
}

export function ConnectionListToolbar({
    searchInputRef,
    searchDraft,
    onSearchDraftChange,
    environment,
    onEnvironmentChange,
    filterPanelOpen,
    onFilterPanelOpenChange,
    appliedChips,
    removeFilter,
    onApplyFilters,
    onClearFilters,
    onResetMoreFilters,
    onCancelMoreFilters,
    healthDraft,
    onHealthDraftChange,
    capabilityDraft,
    onCapabilityDraftChange,
    catalogFreshnessDraft,
    onCatalogFreshnessDraftChange,
    supplierIdDraft,
    onSupplierIdDraftChange,
    hasPendingChanges = false,
    resultCount,
    loading,
    failed,
}: ConnectionListToolbarProps) {
    const moreCount = appliedChips.filter(({ key }) =>
        MORE_CHIP_KEYS.includes(key),
    ).length

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            moreSize="compact"
            className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
            idPrefix="supplier-api-connections-toolbar-filter"
            formAriaLabel="API 供应商连接查询"
            onSubmit={onApplyFilters}
            search={
                <ListSearchField
                    id="supplier-api-connections-toolbar-search"
                    searchInputRef={searchInputRef}
                    value={searchDraft}
                    onChange={onSearchDraftChange}
                    placeholder="连接代码、供应商名称"
                    aria-label="搜索连接"
                />
            }
            queryButtonId="supplier-api-connections-toolbar-apply"
            moreCount={moreCount}
            moreOpen={filterPanelOpen}
            onToggleMore={() =>
                filterPanelOpen
                    ? onCancelMoreFilters()
                    : onFilterPanelOpenChange(true)
            }
            moreButtonId="supplier-api-connections-toolbar-more-filters"
            morePanelId="supplier-api-connections-toolbar-more-panel"
            morePanelAriaLabel="连接列表更多筛选条件"
            onResetMore={onResetMoreFilters}
            resetMoreButtonId="supplier-api-connections-toolbar-reset-more"
            primaryFilters={
                <ResidentSupplierFilter
                    id="supplier-api-connections-toolbar-supplier"
                    value={supplierIdDraft}
                    onValueChange={onSupplierIdDraftChange}
                />
            }
            commonFilters={
                <div
                    role="group"
                    aria-label="环境快捷筛选"
                    className="flex min-w-0 flex-wrap items-center gap-1"
                >
                    {ENVIRONMENT_FILTER_OPTIONS.map((option) => {
                        const active = environment === option.value
                        return (
                            <Button
                                key={option.value}
                                id={`supplier-api-connections-toolbar-environment-${toAutomationIdSegment(option.value)}`}
                                type="button"
                                variant={active ? "secondary" : "ghost"}
                                size="sm"
                                aria-pressed={active}
                                onClick={() =>
                                    onEnvironmentChange(option.value)
                                }
                            >
                                {option.label}
                            </Button>
                        )
                    })}
                </div>
            }
            morePanel={
                <div className="grid min-w-0 gap-3">
                    <ListWorkspaceFilterField
                        htmlFor="supplier-api-connections-toolbar-capability"
                        label="能力"
                    >
                        <OptionCombobox
                            id="supplier-api-connections-toolbar-capability"
                            className="w-full min-w-0"
                            value={capabilityDraft || undefined}
                            onValueChange={(value) =>
                                onCapabilityDraftChange(value ?? "")
                            }
                            options={CAPABILITY_FILTER_OPTIONS}
                            placeholder="全部能力"
                            searchPlaceholder="搜索能力名称"
                            aria-label="能力"
                        />
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField
                        htmlFor="supplier-api-connections-toolbar-health"
                        label="健康结果"
                    >
                        <MultiOptionCombobox
                            id="supplier-api-connections-toolbar-health"
                            className="w-full min-w-0"
                            value={healthDraft}
                            onValueChange={onHealthDraftChange}
                            options={HEALTH_FILTER_OPTIONS}
                            placeholder="全部健康结果"
                            aria-label="健康结果"
                        />
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField
                        htmlFor="supplier-api-connections-toolbar-catalog"
                        label="目录更新时间"
                    >
                        <MultiOptionCombobox
                            id="supplier-api-connections-toolbar-catalog"
                            className="w-full min-w-0"
                            value={catalogFreshnessDraft}
                            onValueChange={onCatalogFreshnessDraftChange}
                            options={CATALOG_FRESHNESS_FILTER_OPTIONS}
                            placeholder="全部目录状态"
                            aria-label="目录更新时间"
                        />
                    </ListWorkspaceFilterField>
                </div>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "个连接",
                loadingLabel: "正在加载连接…",
            })}
            chips={appliedChips}
            onClearChip={(key) => removeFilter(key as ConnectionFilterKey)}
            onClearAll={onClearFilters}
            clearButtonId="supplier-api-connections-toolbar-clear-all"
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询"
        />
    )
}
