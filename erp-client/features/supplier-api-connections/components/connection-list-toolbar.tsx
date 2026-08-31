"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    FilterChip,
    FixedOptionRadioFilter,
    ListToolbar,
    MultiOptionCombobox,
    OptionCombobox,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import type {
    ConnectionAppliedChip,
    ConnectionFilterKey,
    ConnectionStatusFilter,
} from "@/features/supplier-api-connections/hooks/use-connection-list-filters"
import {
    CAPABILITY_LABEL,
    CATALOG_LABEL,
    ENVIRONMENT_LABEL,
    HEALTH_LABEL,
    STATUS_LABEL,
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

const STATUS_FILTER_OPTIONS: ReadonlyArray<{
    value: ConnectionStatusFilter
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "ENABLED", label: STATUS_LABEL.ENABLED },
    { value: "DISABLED", label: STATUS_LABEL.DISABLED },
    { value: "FAULTED", label: STATUS_LABEL.FAULTED },
    { value: "PENDING_CONFIG", label: STATUS_LABEL.PENDING_CONFIG },
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

/** 连接列表筛选区：单一 form，收起态靠搜索框尾部提交箭头与 Enter，展开态只保留面板底部主提交（docs/ui-filter-design.md §3.5）。 */
export type ConnectionListToolbarProps = {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    onSearchDraftChange: SetState<string>
    environment: ConnectionEnvironment | "ALL"
    onEnvironmentChange: (value: ConnectionEnvironment | "ALL") => void
    filterPanelOpen: boolean
    onFilterPanelOpenChange: SetState<boolean>
    hasStructuredFilters: boolean
    appliedChips: readonly ConnectionAppliedChip[]
    removeFilter: (key: ConnectionFilterKey) => void
    onApplyFilters: () => void
    onClearFilters: () => void
    onResetMoreFilters: () => void
    statusDraft: ConnectionStatusFilter
    onStatusDraftChange: SetState<ConnectionStatusFilter>
    healthDraft: readonly string[]
    onHealthDraftChange: SetState<string[]>
    capabilityDraft: string
    onCapabilityDraftChange: SetState<string>
    catalogFreshnessDraft: readonly string[]
    onCatalogFreshnessDraftChange: SetState<string[]>
    supplierIdDraft: string | null
    onSupplierIdDraftChange: SetState<string | null>
}

export function ConnectionListToolbar({
    searchInputRef,
    searchDraft,
    onSearchDraftChange,
    environment,
    onEnvironmentChange,
    filterPanelOpen,
    onFilterPanelOpenChange,
    hasStructuredFilters,
    appliedChips,
    removeFilter,
    onApplyFilters,
    onClearFilters,
    onResetMoreFilters,
    statusDraft,
    onStatusDraftChange,
    healthDraft,
    onHealthDraftChange,
    capabilityDraft,
    onCapabilityDraftChange,
    catalogFreshnessDraft,
    onCatalogFreshnessDraftChange,
    supplierIdDraft,
    onSupplierIdDraftChange,
}: ConnectionListToolbarProps) {
    const panelId = React.useId()
    const hasChips = appliedChips.length > 0

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                onApplyFilters()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            id="supplier-api-connections-toolbar-search"
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(event) =>
                                onSearchDraftChange(event.target.value)
                            }
                            placeholder="连接代码、供应商名称"
                            aria-label="搜索连接"
                        />
                    </InputGroup>
                }
                filters={
                    <>
                        <div
                            role="group"
                            aria-label="环境快捷筛选"
                            className="flex h-control max-w-full items-stretch overflow-x-auto rounded-lg border bg-muted/40 p-0.5 [&_[data-slot=button]]:h-full [&_[data-slot=button]]:min-h-0"
                        >
                            {ENVIRONMENT_FILTER_OPTIONS.map((option) => {
                                const active = environment === option.value
                                return (
                                    <Button
                                        key={option.value}
                                        id={`supplier-api-connections-toolbar-environment-${toAutomationIdSegment(option.value)}`}
                                        type="button"
                                        variant={active ? "secondary" : "ghost"}
                                        className={
                                            active
                                                ? "bg-card shadow-xs"
                                                : "shadow-none"
                                        }
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
                        <Button
                            id="supplier-api-connections-toolbar-more-filters"
                            type="button"
                            variant="outline"
                            aria-expanded={filterPanelOpen}
                            aria-controls={panelId}
                            onClick={() =>
                                onFilterPanelOpenChange(!filterPanelOpen)
                            }
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            更多筛选
                            {hasStructuredFilters ? (
                                <Badge variant="info">已启用</Badge>
                            ) : null}
                            <ChevronDownIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                                className={
                                    filterPanelOpen
                                        ? "rotate-180 transition-transform"
                                        : "transition-transform"
                                }
                            />
                        </Button>
                    </>
                }
                secondary={
                    hasChips || filterPanelOpen ? (
                        <div className="w-full space-y-3">
                            {hasChips ? (
                                <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                                    <span className="text-xs text-muted-foreground">
                                        已筛选
                                    </span>
                                    {appliedChips.map((chip) => (
                                        <FilterChip
                                            key={chip.key}
                                            id={`supplier-api-connections-toolbar-filter-chip-${toAutomationIdSegment(chip.key)}`}
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                removeFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        id="supplier-api-connections-toolbar-clear-all"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={onClearFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {filterPanelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="连接列表更多筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        label="状态"
                                        value={statusDraft}
                                        onValueChange={onStatusDraftChange}
                                        options={STATUS_FILTER_OPTIONS}
                                    />
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                供应商
                                            </span>
                                            <SupplierSearchCombobox
                                                id="supplier-api-connections-toolbar-supplier"
                                                value={
                                                    supplierIdDraft ?? undefined
                                                }
                                                onValueChange={(value) =>
                                                    onSupplierIdDraftChange(
                                                        value ?? null,
                                                    )
                                                }
                                                purpose="filter"
                                                placeholder="全部供应商"
                                                className="w-full"
                                                aria-label="供应商"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                能力
                                            </span>
                                            <OptionCombobox
                                                id="supplier-api-connections-toolbar-capability"
                                                className="w-full"
                                                value={
                                                    capabilityDraft || undefined
                                                }
                                                onValueChange={(value) =>
                                                    onCapabilityDraftChange(
                                                        value ?? "",
                                                    )
                                                }
                                                options={
                                                    CAPABILITY_FILTER_OPTIONS
                                                }
                                                placeholder="全部能力"
                                                searchPlaceholder="搜索能力名称"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                健康结果
                                            </span>
                                            <MultiOptionCombobox
                                                id="supplier-api-connections-toolbar-health"
                                                className="w-full"
                                                value={healthDraft}
                                                onValueChange={
                                                    onHealthDraftChange
                                                }
                                                options={HEALTH_FILTER_OPTIONS}
                                                placeholder="全部健康结果"
                                                aria-label="健康结果"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                目录更新时间
                                            </span>
                                            <MultiOptionCombobox
                                                id="supplier-api-connections-toolbar-catalog"
                                                className="w-full"
                                                value={catalogFreshnessDraft}
                                                onValueChange={
                                                    onCatalogFreshnessDraftChange
                                                }
                                                options={
                                                    CATALOG_FRESHNESS_FILTER_OPTIONS
                                                }
                                                placeholder="全部目录状态"
                                                aria-label="目录更新时间"
                                            />
                                        </div>
                                    </div>
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                id="supplier-api-connections-toolbar-reset-more"
                                                type="button"
                                                variant="ghost"
                                                onClick={onResetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button
                                                id="supplier-api-connections-toolbar-apply"
                                                type="submit"
                                            >
                                                <SearchIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                应用全部筛选
                                            </Button>
                                        </div>
                                    </div>
                                </div>
                            ) : null}
                        </div>
                    ) : undefined
                }
            />
        </form>
    )
}
