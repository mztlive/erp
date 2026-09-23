"use client"
import {
    ResponsibleUserFilter,
    type ResponsibleUserOption,
} from "@/features/entity-selectors/components/responsible-user-filter"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import type {
    CustomerAppliedChip,
    CustomerFilterKey,
} from "@/features/customers/hooks/use-customer-center-directory-state"
import type { DirectoryStatus } from "@/features/customers/lib/directory-url"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const STATUS_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "active", label: "启用" },
    { value: "disabled", label: "停用" },
] as const

const MORE_CHIP_KEYS: readonly CustomerFilterKey[] = ["orgUnitIds"]

/**
 * 客户中心目录工具条：负责销售和状态常驻，组织在更多筛选。
 */
export function CustomerCenterDirectoryToolbar({
    ownerDraft,
    setOwnerDraft,
    ownerOptions,
    orgDraft,
    setOrgDraft,
    descendantsDraft,
    setDescendantsDraft,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    statusDraft,
    setStatusDraft,
    panelOpen,
    setPanelOpen,
    appliedChips,
    removeFilter,
    applyFilters,
    resetMoreFilters,
    cancelMoreFilters,
    clearAllFilters,
    hasPendingChanges,
    resultCount,
    loading,
    failed,
}: {
    ownerDraft: string
    setOwnerDraft: (value: string) => void
    ownerOptions: readonly ResponsibleUserOption[]
    orgDraft: string
    setOrgDraft: (value: string) => void
    descendantsDraft: boolean
    setDescendantsDraft: (value: boolean) => void
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    statusDraft: DirectoryStatus
    setStatusDraft: SetState<DirectoryStatus>
    panelOpen: boolean
    setPanelOpen: (open: boolean) => void
    appliedChips: readonly CustomerAppliedChip[]
    removeFilter: (key: CustomerFilterKey) => void
    applyFilters: () => void
    resetMoreFilters: () => void
    cancelMoreFilters: () => void
    clearAllFilters: () => void
    hasPendingChanges: boolean
    resultCount?: number
    loading?: boolean
    failed?: boolean
}) {
    const moreCount = appliedChips.filter((chip) =>
        MORE_CHIP_KEYS.includes(chip.key),
    ).length

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            moreSize="compact"
            className="[&_[data-slot=list-toolbar-search]]:lg:w-80 [&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
            idPrefix="customers-directory"
            formAriaLabel="客户目录查询"
            onSubmit={applyFilters}
            queryButtonId="customers-directory-query"
            clearButtonId="customers-directory-clear-all"
            search={
                <ListSearchField
                    id="customers-directory-search"
                    searchInputRef={searchInputRef}
                    data-slot="customer-search"
                    value={searchDraft}
                    onChange={setSearchDraft}
                    placeholder="客户名称、编号或信用代码"
                    aria-label="搜索客户"
                />
            }
            moreCount={moreCount}
            moreOpen={panelOpen}
            onToggleMore={() =>
                panelOpen ? cancelMoreFilters() : setPanelOpen(true)
            }
            morePanelId="customers-directory-more-panel"
            morePanelAriaLabel="客户更多筛选条件"
            onResetMore={resetMoreFilters}
            primaryFilters={
                <>
                    <div className="w-56 min-w-0 max-w-full">
                        <ResponsibleUserFilter
                            id="customers-directory-owner"
                            label="负责销售"
                            hideLabel
                            value={ownerDraft}
                            onChange={setOwnerDraft}
                            options={ownerOptions}
                        />
                    </div>
                    <OptionCombobox
                        id="customers-directory-status"
                        className="w-44 min-w-0 max-w-full"
                        filterLabel="状态"
                        aria-label="状态"
                        allowClear={false}
                        value={statusDraft}
                        options={STATUS_OPTIONS}
                        onValueChange={(value) => {
                            setStatusDraft(
                                value === "disabled" || value === "all"
                                    ? value
                                    : "active",
                            )
                        }}
                        placeholder="启用"
                    />
                </>
            }
            morePanel={
                <OrganizationUnitFilter
                    id="customers-directory-org"
                    label="组织"
                    value={orgDraft}
                    onChange={setOrgDraft}
                    includeDescendants={descendantsDraft}
                    onDescendantsChange={setDescendantsDraft}
                />
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "个客户",
                loadingLabel: "正在加载客户…",
            })}
            chips={appliedChips}
            onClearChip={(key) => removeFilter(key as CustomerFilterKey)}
            onClearAll={clearAllFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
            idleHint="导出与当前查询结果一致"
        />
    )
}
