"use client"

import type * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import type { CustomerAppliedChip } from "@/features/customers/hooks/use-customer-center-directory-state"
import type { DirectoryStatus } from "@/features/customers/lib/directory-url"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import { selectedOrgUnitIds } from "@/features/organization/lib/organization-ids"
import { listFilterText } from "@/lib/ui-text"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const FOLDED_CHIP_KEYS = ["orgUnitIds"] as const

const STATUS_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "active", label: "启用" },
    { value: "disabled", label: "停用" },
] as const

/**
 * 客户中心目录工具条：负责销售和状态常驻，组织在更多筛选。
 */
export function CustomerCenterDirectoryToolbar({
    canFilterOwner,
    canFilterOrg,
    ownerDraft,
    setOwnerDraft,
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
    removeOrgFilter,
    applyFilters,
    resetMoreFilters,
    cancelMoreFilters,
    clearAllFilters,
    hasPendingChanges,
    hasActiveFilters,
    resultStatus,
}: {
    canFilterOwner: boolean
    canFilterOrg: boolean
    ownerDraft: string
    setOwnerDraft: (value: string) => void
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
    removeOrgFilter: () => void
    applyFilters: () => void
    resetMoreFilters: () => void
    cancelMoreFilters: () => void
    clearAllFilters: () => void
    hasPendingChanges: boolean
    hasActiveFilters: boolean
    resultStatus: string | null
}) {
    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
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
            moreOpen={panelOpen}
            onToggleMore={() =>
                panelOpen ? cancelMoreFilters() : setPanelOpen(true)
            }
            morePanelId="customers-directory-more-panel"
            morePanelAriaLabel="客户更多筛选条件"
            onResetMore={resetMoreFilters}
            primaryFilters={
                <>
                    {canFilterOwner && (
                        <div className="w-44 min-w-0 max-w-full empty:hidden">
                            <PersonDirectoryFilter
                                id="customers-directory-owner"
                                category="sales"
                                label="负责销售"
                                hideLabel
                                value={ownerDraft}
                                onChange={setOwnerDraft}
                                orgUnitIds={selectedOrgUnitIds(orgDraft)}
                                includeDescendants={descendantsDraft}
                            />
                        </div>
                    )}
                    <OptionCombobox
                        id="customers-directory-status"
                        className="w-32 min-w-0 max-w-full"
                        filterLabel="状态"
                        aria-label="状态"
                        allowClear={statusDraft !== "active"}
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
            extraPrimary={
                hasActiveFilters && appliedChips.length === 0 ? (
                    <Button
                        id="customers-directory-clear-all"
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={clearAllFilters}
                    >
                        {listFilterText.clear}
                    </Button>
                ) : undefined
            }
            morePanel={
                canFilterOrg ? (
                    <ListFilterSections>
                        <ListFilterSection legend="归属">
                            <OrganizationUnitFilter
                                compact
                                id="customers-directory-org"
                                label="组织"
                                value={orgDraft}
                                onChange={setOrgDraft}
                                includeDescendants={descendantsDraft}
                                onDescendantsChange={setDescendantsDraft}
                            />
                        </ListFilterSection>
                    </ListFilterSections>
                ) : undefined
            }
            resultStatus={resultStatus}
            chips={appliedChips}
            foldedChipKeys={FOLDED_CHIP_KEYS}
            onClearChip={removeOrgFilter}
            onClearAll={clearAllFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
        />
    )
}
