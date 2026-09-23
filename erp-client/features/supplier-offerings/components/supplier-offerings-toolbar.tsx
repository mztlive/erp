"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Input } from "@/components/ui/input"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"
import {
    CompanySkuSearchCombobox,
    SupplierSearchCombobox,
} from "@/features/entity-selectors"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
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
    { value: "MANUAL", label: SOURCE_TYPE_LABELS.MANUAL },
    { value: "EXCEL", label: SOURCE_TYPE_LABELS.EXCEL },
    { value: "API", label: SOURCE_TYPE_LABELS.API },
] as const

const AVAILABILITY_STATUS_FILTER_OPTIONS = [
    { value: "AVAILABLE", label: AVAILABILITY_STATUS_LABELS.AVAILABLE },
    { value: "UNAVAILABLE", label: AVAILABILITY_STATUS_LABELS.UNAVAILABLE },
    { value: "STOPPED", label: AVAILABILITY_STATUS_LABELS.STOPPED },
    { value: "STALE", label: AVAILABILITY_STATUS_LABELS.STALE },
] as const

const MORE_CHIP_KEYS: readonly SupplierOfferingFilterKey[] = [
    "sourceType",
    "skuId",
    "skuNo",
    "productNo",
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
    onCancelMoreFilters: () => void
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
    onCancelMoreFilters,
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
            morePresentation="popover"
            moreSize="wide"
            className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
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
            onToggleMore={() =>
                filterPanelOpen
                    ? onCancelMoreFilters()
                    : onFilterPanelOpenChange(true)
            }
            moreButtonId="supplier-offerings-toolbar-filter-toggle"
            morePanelId="supplier-offerings-toolbar-more-panel"
            morePanelAriaLabel="供应商供给更多筛选条件"
            onResetMore={onResetMoreFilters}
            resetMoreButtonId="supplier-offerings-toolbar-reset-more"
            primaryFilters={
                <>
                    <SupplierSearchCombobox
                        id="supplier-offerings-toolbar-supplier-select"
                        className="w-56 max-w-full min-w-0"
                        filterLabel="供应商"
                        value={supplierIdDraft ?? undefined}
                        onValueChange={(value) =>
                            onSupplierIdDraftChange(value ?? null)
                        }
                        placeholder="全部"
                        aria-label="供应商"
                    />
                    <OptionCombobox
                        id="supplier-offerings-toolbar-filter-availability"
                        className="w-48 max-w-full"
                        filterLabel="当前可供"
                        aria-label="当前可供"
                        value={
                            availabilityStatusDraft === "all"
                                ? null
                                : availabilityStatusDraft
                        }
                        options={AVAILABILITY_STATUS_FILTER_OPTIONS}
                        onValueChange={(value) => {
                            if (
                                value !== null &&
                                !AVAILABILITY_STATUS_FILTER_OPTIONS.some(
                                    (option) => option.value === value,
                                )
                            ) {
                                return
                            }
                            onAvailabilityStatusDraftChange(
                                (value ?? "all") as AvailabilityStatusFilter,
                            )
                        }}
                        placeholder="全部"
                    />
                </>
            }
            morePanel={
                <div className="space-y-5">
                    <fieldset className="min-w-0 space-y-3">
                        <legend className="mb-1 text-sm font-medium">
                            人员
                        </legend>
                        <div className="grid min-w-0 gap-3 sm:grid-cols-2">
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
                        </div>
                    </fieldset>
                    <fieldset className="min-w-0 border-t pt-4">
                        <legend className="pr-2 text-sm font-medium">
                            商品
                        </legend>
                        <div className="grid min-w-0 gap-3 sm:grid-cols-2">
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
                                        className="w-full min-w-0"
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
                                    className="w-full min-w-0"
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
                                    className="w-full min-w-0"
                                    value={productNoDraft}
                                    onChange={(event) =>
                                        onProductNoDraftChange(
                                            event.target.value,
                                        )
                                    }
                                    autoComplete="off"
                                    placeholder="如 P-1001"
                                    aria-label="SPU 编号"
                                />
                            </ListWorkspaceFilterField>
                        </div>
                    </fieldset>
                    <fieldset className="min-w-0 border-t pt-4">
                        <legend className="pr-2 text-sm font-medium">
                            登记来源
                        </legend>
                        <div className="grid min-w-0 gap-3 sm:grid-cols-2">
                            <OptionCombobox
                                id="supplier-offerings-toolbar-filter-source-type"
                                className="w-full min-w-0"
                                value={
                                    sourceTypeDraft === "all"
                                        ? null
                                        : sourceTypeDraft
                                }
                                options={SOURCE_TYPE_FILTER_OPTIONS}
                                onValueChange={(value) => {
                                    if (
                                        value !== null &&
                                        !SOURCE_TYPE_FILTER_OPTIONS.some(
                                            (option) => option.value === value,
                                        )
                                    ) {
                                        return
                                    }
                                    onSourceTypeDraftChange(
                                        (value ??
                                            "all") as OfferingSourceFilter,
                                    )
                                }}
                                placeholder="全部"
                                aria-label="登记来源"
                            />
                        </div>
                    </fieldset>
                    <fieldset className="min-w-0 border-t pt-4">
                        <legend className="sr-only">业务组织</legend>
                        <OrganizationUnitFilter
                            id="supplier-offerings-toolbar-org"
                            label="业务组织"
                            value={orgUnitIdsDraft}
                            onChange={onOrgUnitIdsDraftChange}
                            includeDescendants={includeDescendantsDraft}
                            onDescendantsChange={
                                onIncludeDescendantsDraftChange
                            }
                        />
                    </fieldset>
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
