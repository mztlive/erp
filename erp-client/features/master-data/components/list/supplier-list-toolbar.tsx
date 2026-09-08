"use client"

import * as React from "react"

import {
    FixedOptionCheckboxFilter,
    FixedOptionRadioFilter,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
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

const MORE_CHIP_KEYS = [
    "supplierCapabilityCodes",
    "supplierQualificationTypes",
] as const

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
            idPrefix={prefix}
            formAriaLabel="供应商与资质查询"
            onSubmit={f.applySupplierFilters}
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
            onToggleMore={() => f.setSupplierFilterPanelOpen((open) => !open)}
            morePanelId={panelId}
            morePanelAriaLabel="供应商与资质更多筛选条件"
            moreButtonId={`${prefix}-filter-trigger`}
            resetMoreButtonId={`${prefix}-reset`}
            clearButtonId={`${prefix}-clear-filters`}
            onResetMore={f.resetMoreFilters}
            commonFilters={
                <>
                    <FixedOptionRadioFilter
                        id={`${prefix}-filter-qualification-health`}
                        label="资质状态"
                        variant="quiet"
                        value={f.supplierQualificationHealthDraft}
                        onValueChange={f.setSupplierQualificationHealthDraft}
                        options={SUPPLIER_QUALIFICATION_HEALTH_OPTIONS}
                        aria-label="资质状态"
                    />
                </>
            }
            morePanel={
                <div className="grid min-w-0 gap-5">
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            供应能力
                        </legend>
                        <FixedOptionCheckboxFilter
                            id={`${prefix}-filter-capability`}
                            label="供应能力"
                            value={f.supplierCapabilityCodesDraft}
                            onValueChange={f.setSupplierCapabilityCodesDraft}
                            options={SUPPLIER_CAPABILITY_OPTIONS}
                            aria-label="供应能力，可多选"
                        />
                    </fieldset>
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            资质类型
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
