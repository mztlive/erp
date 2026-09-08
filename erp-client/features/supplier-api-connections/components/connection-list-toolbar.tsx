"use client"

import * as React from "react"

import { MultiOptionCombobox, OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
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
    "supplierId",
]

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
            extraPrimary={
                <div
                    role="group"
                    aria-label="环境快捷筛选"
                    className="flex flex-wrap items-center gap-1"
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
            moreCount={moreCount}
            moreOpen={filterPanelOpen}
            onToggleMore={() => onFilterPanelOpenChange(!filterPanelOpen)}
            moreButtonId="supplier-api-connections-toolbar-more-filters"
            morePanelId="supplier-api-connections-toolbar-more-panel"
            morePanelAriaLabel="连接列表更多筛选条件"
            onResetMore={onResetMoreFilters}
            resetMoreButtonId="supplier-api-connections-toolbar-reset-more"
            morePanel={
                <div className="grid min-w-0 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                    <ListWorkspaceFilterField
                        htmlFor="supplier-api-connections-toolbar-supplier"
                        label="供应商"
                    >
                        <SupplierSearchCombobox
                            id="supplier-api-connections-toolbar-supplier"
                            value={supplierIdDraft ?? undefined}
                            onValueChange={(value) =>
                                onSupplierIdDraftChange(value ?? null)
                            }
                            purpose="filter"
                            placeholder="全部供应商"
                            className="w-full"
                            aria-label="供应商"
                        />
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField
                        htmlFor="supplier-api-connections-toolbar-capability"
                        label="能力"
                    >
                        <OptionCombobox
                            id="supplier-api-connections-toolbar-capability"
                            className="w-full"
                            value={capabilityDraft || undefined}
                            onValueChange={(value) =>
                                onCapabilityDraftChange(value ?? "")
                            }
                            options={CAPABILITY_FILTER_OPTIONS}
                            placeholder="全部能力"
                            searchPlaceholder="搜索能力名称"
                        />
                    </ListWorkspaceFilterField>
                    <ListWorkspaceFilterField
                        htmlFor="supplier-api-connections-toolbar-health"
                        label="健康结果"
                    >
                        <MultiOptionCombobox
                            id="supplier-api-connections-toolbar-health"
                            className="w-full"
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
                            className="w-full"
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
