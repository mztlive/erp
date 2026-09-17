"use client"

import * as React from "react"

import { FixedOptionRadioFilter } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"
import {
    CompanySkuSearchCombobox,
    SupplierSearchCombobox,
} from "@/features/entity-selectors"
import type {
    AvailabilityStatusFilter,
    OfferingSourceFilter,
    SupplierOfferingAppliedChip,
    SupplierOfferingFilterKey,
} from "@/features/supplier-offerings/hooks/use-supplier-offerings-page-state"
import {
    AVAILABILITY_STATUS_LABELS,
    SOURCE_TYPE_LABELS,
} from "@/features/supplier-offerings/types"

const SOURCE_TYPE_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "MANUAL", label: SOURCE_TYPE_LABELS.MANUAL },
    { value: "EXCEL", label: SOURCE_TYPE_LABELS.EXCEL },
    { value: "API", label: SOURCE_TYPE_LABELS.API },
] as const

const AVAILABILITY_STATUS_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "AVAILABLE", label: AVAILABILITY_STATUS_LABELS.AVAILABLE },
    { value: "UNAVAILABLE", label: AVAILABILITY_STATUS_LABELS.UNAVAILABLE },
    { value: "STOPPED", label: AVAILABILITY_STATUS_LABELS.STOPPED },
    { value: "STALE", label: AVAILABILITY_STATUS_LABELS.STALE },
] as const

const MORE_CHIP_KEYS: readonly SupplierOfferingFilterKey[] = [
    "sourceType",
    "availabilityStatus",
    "skuId",
    "skuNo",
    "productNo",
    "supplierId",
    "ownerUserIds",
    "procurementOwnerUserIds",
    "orgUnitIds",
]

export type SupplierOfferingsToolbarProps = {
    searchInputRef: React.Ref<HTMLInputElement>
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    filterPanelOpen: boolean
    onFilterPanelOpenChange: (open: boolean) => void
    appliedChips: readonly SupplierOfferingAppliedChip[]
    removeFilter: (key: SupplierOfferingFilterKey) => void
    onApplyFilters: () => void
    onClearFilters: () => void
    onResetMoreFilters: () => void
    sourceTypeDraft: OfferingSourceFilter
    onSourceTypeDraftChange: (value: OfferingSourceFilter) => void
    availabilityStatusDraft: AvailabilityStatusFilter
    onAvailabilityStatusDraftChange: (value: AvailabilityStatusFilter) => void
    skuLocked: boolean
    skuIdDraft: string | null
    onSkuIdDraftChange: (value: string | null) => void
    skuNoDraft: string
    onSkuNoDraftChange: (value: string) => void
    productNoDraft: string
    onProductNoDraftChange: (value: string) => void
    supplierIdDraft: string | null
    onSupplierIdDraftChange: (value: string | null) => void
    ownerUserIdsDraft: string
    onOwnerUserIdsDraftChange: (value: string) => void
    ownerOptions: ReadonlyArray<{ value: string; label: string }>
    procurementOwnerUserIdsDraft: string
    onProcurementOwnerUserIdsDraftChange: (value: string) => void
    procurementOwnerOptions: ReadonlyArray<{ value: string; label: string }>
    orgUnitIdsDraft: string
    onOrgUnitIdsDraftChange: (value: string) => void
    includeDescendantsDraft: boolean
    onIncludeDescendantsDraftChange: (value: boolean) => void
    hasPendingChanges?: boolean
    resultCount?: number
    loading?: boolean
    failed?: boolean
}

export function SupplierOfferingsToolbar({
    searchInputRef,
    searchDraft,
    onSearchDraftChange,
    filterPanelOpen,
    onFilterPanelOpenChange,
    appliedChips,
    removeFilter,
    onApplyFilters,
    onClearFilters,
    onResetMoreFilters,
    sourceTypeDraft,
    onSourceTypeDraftChange,
    availabilityStatusDraft,
    onAvailabilityStatusDraftChange,
    skuLocked,
    skuIdDraft,
    onSkuIdDraftChange,
    skuNoDraft,
    onSkuNoDraftChange,
    productNoDraft,
    onProductNoDraftChange,
    supplierIdDraft,
    onSupplierIdDraftChange,
    ownerUserIdsDraft,
    onOwnerUserIdsDraftChange,
    ownerOptions,
    procurementOwnerUserIdsDraft,
    onProcurementOwnerUserIdsDraftChange,
    procurementOwnerOptions,
    orgUnitIdsDraft,
    onOrgUnitIdsDraftChange,
    includeDescendantsDraft,
    onIncludeDescendantsDraftChange,
    hasPendingChanges = false,
    resultCount,
    loading,
    failed,
}: SupplierOfferingsToolbarProps) {
    const moreCount = appliedChips.filter(({ key }) =>
        MORE_CHIP_KEYS.includes(key),
    ).length

    return (
        <ListWorkspaceFilterBar
            idPrefix="supplier-offerings-toolbar"
            formAriaLabel="供应商供给查询"
            onSubmit={onApplyFilters}
            search={
                <ListSearchField
                    id="supplier-offerings-toolbar-search"
                    searchInputRef={searchInputRef}
                    value={searchDraft}
                    onChange={onSearchDraftChange}
                    placeholder="供应商订货编码"
                    aria-label="搜索供给"
                />
            }
            queryButtonId="supplier-offerings-toolbar-apply"
            moreCount={moreCount}
            moreOpen={filterPanelOpen}
            onToggleMore={() => onFilterPanelOpenChange(!filterPanelOpen)}
            moreButtonId="supplier-offerings-toolbar-filter-toggle"
            morePanelId="supplier-offerings-toolbar-more-panel"
            morePanelAriaLabel="供应商供给更多筛选条件"
            onResetMore={onResetMoreFilters}
            resetMoreButtonId="supplier-offerings-toolbar-reset-more"
            morePanel={
                <div className="grid min-w-0 gap-5">
                    <ResponsibleUserFilter
                        id="supplier-offerings-toolbar-owner"
                        label="维护人"
                        value={ownerUserIdsDraft}
                        onChange={onOwnerUserIdsDraftChange}
                        options={ownerOptions}
                    />
                    <ResponsibleUserFilter
                        id="supplier-offerings-toolbar-procurement-owner"
                        label="采购负责人"
                        value={procurementOwnerUserIdsDraft}
                        onChange={onProcurementOwnerUserIdsDraftChange}
                        options={procurementOwnerOptions}
                    />
                    <ListWorkspaceFilterField
                        htmlFor="supplier-offerings-toolbar-org"
                        label="业务组织"
                    >
                        <Input
                            id="supplier-offerings-toolbar-org"
                            value={orgUnitIdsDraft}
                            onChange={(event) =>
                                onOrgUnitIdsDraftChange(event.target.value)
                            }
                            placeholder="组织 ID，逗号分隔"
                            aria-label="按供给业务组织筛选"
                        />
                        <label
                            htmlFor="supplier-offerings-toolbar-org-descendants"
                            className="mt-2 flex items-center gap-2 text-xs text-muted-foreground"
                        >
                            <Checkbox
                                id="supplier-offerings-toolbar-org-descendants"
                                checked={includeDescendantsDraft}
                                onCheckedChange={(checked) =>
                                    onIncludeDescendantsDraftChange(
                                        checked === true,
                                    )
                                }
                            />
                            包含下级组织
                        </label>
                    </ListWorkspaceFilterField>
                    <FixedOptionRadioFilter
                        idPrefix="supplier-offerings-toolbar-filter-source-type"
                        label="登记来源"
                        value={sourceTypeDraft}
                        onValueChange={onSourceTypeDraftChange}
                        options={SOURCE_TYPE_FILTER_OPTIONS}
                    />
                    <FixedOptionRadioFilter
                        idPrefix="supplier-offerings-toolbar-filter-availability"
                        label="当前可供"
                        value={availabilityStatusDraft}
                        onValueChange={onAvailabilityStatusDraftChange}
                        options={AVAILABILITY_STATUS_FILTER_OPTIONS}
                    />
                    <div className="grid min-w-0 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                        {!skuLocked ? (
                            <ListWorkspaceFilterField
                                htmlFor="supplier-offerings-toolbar-sku-select"
                                label="公司 SKU"
                            >
                                <CompanySkuSearchCombobox
                                    id="supplier-offerings-toolbar-sku-select"
                                    value={skuIdDraft ?? undefined}
                                    onValueChange={(value) =>
                                        onSkuIdDraftChange(value ?? null)
                                    }
                                    placeholder="全部公司 SKU"
                                    className="w-full"
                                    aria-label="公司 SKU"
                                />
                            </ListWorkspaceFilterField>
                        ) : null}
                        <ListWorkspaceFilterField
                            htmlFor="supplier-offerings-toolbar-sku-no"
                            label="SKU 编号"
                        >
                            <Input
                                id="supplier-offerings-toolbar-sku-no"
                                className="w-full"
                                value={skuNoDraft}
                                onChange={(event) =>
                                    onSkuNoDraftChange(event.target.value)
                                }
                                autoComplete="off"
                                placeholder="如 SKU-001"
                                aria-label="SKU 编号"
                            />
                        </ListWorkspaceFilterField>
                        <ListWorkspaceFilterField
                            htmlFor="supplier-offerings-toolbar-product-no"
                            label="SPU 编号"
                        >
                            <Input
                                id="supplier-offerings-toolbar-product-no"
                                className="w-full"
                                value={productNoDraft}
                                onChange={(event) =>
                                    onProductNoDraftChange(event.target.value)
                                }
                                autoComplete="off"
                                placeholder="如 P-1001"
                                aria-label="SPU 编号"
                            />
                        </ListWorkspaceFilterField>
                        <ListWorkspaceFilterField
                            htmlFor="supplier-offerings-toolbar-supplier-select"
                            label="供应商"
                        >
                            <SupplierSearchCombobox
                                id="supplier-offerings-toolbar-supplier-select"
                                value={supplierIdDraft ?? undefined}
                                onValueChange={(value) =>
                                    onSupplierIdDraftChange(value ?? null)
                                }
                                placeholder="全部供应商"
                                className="w-full"
                                aria-label="供应商"
                            />
                        </ListWorkspaceFilterField>
                    </div>
                </div>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "条供给",
                loadingLabel: "正在加载供给…",
            })}
            chips={appliedChips}
            onClearChip={(key) =>
                removeFilter(key as SupplierOfferingFilterKey)
            }
            onClearAll={onClearFilters}
            clearButtonId="supplier-offerings-toolbar-clear-all"
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询"
        />
    )
}
