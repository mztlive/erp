"use client"

import * as React from "react"

import {
    FixedOptionCheckboxFilter,
    MultiOptionCombobox,
    OptionCombobox,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { masterDataSearchPlaceholder } from "@/features/master-data/lib/copy"
import {
    SUPPLIER_CAPABILITY_OPTIONS,
    SUPPLIER_QUALIFICATION_HEALTH_OPTIONS,
    SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import type { SupplierAppliedChip } from "@/features/master-data/hooks/use-supplier-list-state"
import type {
    SupplierFilterKey,
    useSupplierListFilters,
} from "@/features/master-data/hooks/use-supplier-list-filters"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"

const MORE_CHIP_KEYS = [
    "supplierQualificationTypes",
    "owner_user_ids",
    "capability_owner_user_ids",
    "org_unit_ids",
] as const
const QUALIFICATION_HEALTH_OPTIONS =
    SUPPLIER_QUALIFICATION_HEALTH_OPTIONS.filter(
        (option) => option.value !== "all",
    )

export function SupplierListToolbar({
    idPrefix,
    searchInputRef,
    filters: f,
    appliedChips,
    resultCount,
    loading,
    failed,
}: {
    idPrefix?: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    filters: ReturnType<typeof useSupplierListFilters>
    appliedChips: readonly SupplierAppliedChip[]
    resultCount?: number
    loading: boolean
    failed: boolean
}) {
    const prefix = idPrefix ?? "master-data-list-supplier-list-toolbar"
    const panelId = `${prefix}-more-panel`
    const moreCount = appliedChips.filter(({ key }) =>
        MORE_CHIP_KEYS.includes(key as (typeof MORE_CHIP_KEYS)[number]),
    ).length

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            moreSize="wide"
            className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
            idPrefix={prefix}
            formAriaLabel="供应商与资质查询"
            onSubmit={f.applySupplierFilters}
            queryButtonId={`${prefix}-query`}
            search={
                <ListSearchField
                    id={`${prefix}-search-input`}
                    searchInputRef={searchInputRef}
                    value={f.searchDraft}
                    onChange={f.setSearchDraft}
                    placeholder={masterDataSearchPlaceholder("suppliers")}
                    aria-label="搜索基础资料"
                />
            }
            moreCount={moreCount}
            moreOpen={f.supplierFilterPanelOpen}
            onToggleMore={() =>
                f.supplierFilterPanelOpen
                    ? f.cancelMoreFilters()
                    : f.setSupplierFilterPanelOpen(true)
            }
            morePanelId={panelId}
            morePanelAriaLabel="供应商与资质更多筛选条件"
            moreButtonId={`${prefix}-filter-trigger`}
            resetMoreButtonId={`${prefix}-reset`}
            clearButtonId={`${prefix}-clear-filters`}
            onResetMore={f.resetMoreFilters}
            primaryFilters={
                <>
                    <OptionCombobox
                        id={`${prefix}-filter-qualification-health`}
                        className="w-56 max-w-full min-w-0"
                        filterLabel="资质状态"
                        aria-label="资质状态"
                        value={
                            f.supplierQualificationHealthDraft === "all"
                                ? null
                                : f.supplierQualificationHealthDraft
                        }
                        options={QUALIFICATION_HEALTH_OPTIONS}
                        placeholder="全部"
                        onValueChange={(value) => {
                            const next = QUALIFICATION_HEALTH_OPTIONS.find(
                                (option) => option.value === value,
                            )
                            f.setSupplierQualificationHealthDraft(
                                next ? next.value : "all",
                            )
                        }}
                    />
                    <div className="w-56 max-w-full min-w-0">
                        <MultiOptionCombobox
                            id={`${prefix}-filter-capability`}
                            filterLabel="供应能力"
                            aria-label="供应能力，可多选"
                            value={f.supplierCapabilityCodesDraft}
                            onValueChange={f.setSupplierCapabilityCodesDraft}
                            options={SUPPLIER_CAPABILITY_OPTIONS}
                        />
                    </div>
                </>
            }
            morePanel={
                <div className="grid min-w-0 gap-5">
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            资质
                        </legend>
                        <FixedOptionCheckboxFilter
                            id={`${prefix}-filter-qualification-type`}
                            label="资质类型"
                            value={f.supplierQualificationTypesDraft}
                            onValueChange={f.setSupplierQualificationTypesDraft}
                            options={SUPPLIER_QUALIFICATION_TYPE_OPTIONS}
                            aria-label="资质类型，可多选"
                        />
                    </fieldset>
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            人员
                        </legend>
                        <div className="grid min-w-0 gap-3 sm:grid-cols-2">
                            <PersonDirectoryFilter
                                id={`${prefix}-filter-maintainer`}
                                label="维护人"
                                value={f.ownerUserIdsDraft}
                                onChange={f.setOwnerUserIdsDraft}
                                category="business"
                            />
                            <PersonDirectoryFilter
                                id={`${prefix}-filter-capability-owner`}
                                label="能力负责人"
                                value={f.capabilityOwnerUserIdsDraft}
                                onChange={f.setCapabilityOwnerUserIdsDraft}
                                category="business"
                            />
                        </div>
                    </fieldset>
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            范围
                        </legend>
                        <OrganizationUnitFilter
                            id={`${prefix}-filter-org`}
                            label="业务组织"
                            value={f.orgUnitIdsDraft}
                            onChange={f.setOrgUnitIdsDraft}
                            includeDescendants={f.includeDescendantsDraft}
                            onDescendantsChange={f.setIncludeDescendantsDraft}
                        />
                    </fieldset>
                </div>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "条",
            })}
            chips={appliedChips}
            onClearChip={(key) => f.removeFilter(key as SupplierFilterKey)}
            onClearAll={f.clearAllFilters}
            hasPendingChanges={f.hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
            idleHint="导出与当前查询结果一致"
        />
    )
}
