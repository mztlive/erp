"use client"

import {
    FilterChip,
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    OptionCombobox,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    CompanySkuSearchCombobox,
    SupplierSearchCombobox,
} from "@/features/entity-selectors"

import type {
    SupplierOfferingAppliedChip,
    useSupplierOfferingsPageState,
} from "../hooks/use-supplier-offerings-page-state"
import { AVAILABILITY_STATUS_LABELS, SOURCE_TYPE_LABELS } from "../types"

const SOURCE_OPTIONS = [
    { value: "MANUAL", label: SOURCE_TYPE_LABELS.MANUAL },
    { value: "EXCEL", label: SOURCE_TYPE_LABELS.EXCEL },
    { value: "API", label: SOURCE_TYPE_LABELS.API },
] as const
const AVAILABILITY_OPTIONS = [
    { value: "AVAILABLE", label: AVAILABILITY_STATUS_LABELS.AVAILABLE },
    { value: "UNAVAILABLE", label: AVAILABILITY_STATUS_LABELS.UNAVAILABLE },
    { value: "STOPPED", label: AVAILABILITY_STATUS_LABELS.STOPPED },
    { value: "STALE", label: AVAILABILITY_STATUS_LABELS.STALE },
] as const

export function SupplierOfferingsToolbar({
    state,
    appliedChips,
    resultStatus,
}: {
    state: ReturnType<typeof useSupplierOfferingsPageState>
    appliedChips: readonly SupplierOfferingAppliedChip[]
    resultStatus: string | null
}) {
    const lockedSkuChip = state.skuLocked
        ? appliedChips.find(({ key }) => key === "skuId")
        : undefined
    const foldedChips = appliedChips.filter((chip) => chip !== lockedSkuChip)
    const showClearAll = state.hasClearableFilters

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            idPrefix="supplier-offerings-toolbar"
            formAriaLabel="供应商供给查询"
            onSubmit={state.applyFilters}
            search={
                <ListSearchField
                    id="supplier-offerings-toolbar-search"
                    searchInputRef={state.searchInputRef}
                    value={state.searchDraft}
                    onChange={state.setSearchDraft}
                    placeholder="订货编码、SKU 编号或名称"
                    aria-label="搜索供给"
                />
            }
            queryButtonId="supplier-offerings-toolbar-apply"
            moreCount={foldedChips.length}
            moreOpen={state.filterPanelOpen}
            onToggleMore={() =>
                state.filterPanelOpen
                    ? state.cancelMoreFilters()
                    : state.setFilterPanelOpen(true)
            }
            moreButtonId="supplier-offerings-toolbar-filter-toggle"
            morePanelId="supplier-offerings-toolbar-more-panel"
            morePanelAriaLabel="供应商供给更多筛选条件"
            onResetMore={state.resetMoreFilters}
            resetMoreButtonId="supplier-offerings-toolbar-reset-more"
            primaryFilters={
                <>
                    {state.canFilterSupplier ? (
                        <SupplierSearchCombobox
                            id="supplier-offerings-toolbar-supplier-select"
                            purpose="filter"
                            className="w-44 max-w-full min-w-0"
                            filterLabel="供应商"
                            value={state.supplierIdDraft ?? undefined}
                            onValueChange={(value) =>
                                state.setSupplierIdDraft(value ?? null)
                            }
                            placeholder="全部"
                            aria-label="供应商"
                        />
                    ) : null}
                    <OptionCombobox
                        id="supplier-offerings-toolbar-filter-availability"
                        className="w-36 max-w-full min-w-0"
                        filterLabel="当前可供"
                        aria-label="当前可供"
                        value={
                            state.availabilityStatusDraft === "all"
                                ? null
                                : state.availabilityStatusDraft
                        }
                        options={AVAILABILITY_OPTIONS}
                        onValueChange={(value) =>
                            state.setAvailabilityStatusDraft(
                                AVAILABILITY_OPTIONS.find(
                                    (option) => option.value === value,
                                )?.value ?? "all",
                            )
                        }
                        placeholder="全部"
                    />
                </>
            }
            extraPrimary={
                <>
                    {lockedSkuChip ? (
                        <FilterChip
                            id={`supplier-offerings-toolbar-chip-${toAutomationIdSegment(lockedSkuChip.key)}`}
                            label={lockedSkuChip.label}
                            clearLabel={`移除${lockedSkuChip.label}`}
                            onClear={() => state.removeFilter("skuId")}
                        />
                    ) : null}
                    {showClearAll && foldedChips.length === 0 ? (
                        <Button
                            id="supplier-offerings-toolbar-clear-all"
                            type="button"
                            variant="ghost"
                            size="sm"
                            className="font-normal text-muted-foreground hover:text-primary"
                            onClick={state.clearFilters}
                        >
                            清空
                        </Button>
                    ) : null}
                </>
            }
            morePanel={
                <ListFilterSections>
                    {state.canFilterOwner || state.canFilterProcurementOwner ? (
                        <ListFilterSection legend="人员">
                            {state.canFilterOwner ? (
                                <PersonDirectoryFilter
                                    id="supplier-offerings-toolbar-owner"
                                    label="维护人"
                                    value={state.ownerUserIdsDraft}
                                    onChange={state.setOwnerUserIdsDraft}
                                    category="business"
                                />
                            ) : null}
                            {state.canFilterProcurementOwner ? (
                                <PersonDirectoryFilter
                                    id="supplier-offerings-toolbar-procurement-owner"
                                    label="采购负责人"
                                    value={state.procurementOwnerUserIdsDraft}
                                    onChange={
                                        state.setProcurementOwnerUserIdsDraft
                                    }
                                    category="procurement"
                                />
                            ) : null}
                        </ListFilterSection>
                    ) : null}
                    <ListFilterSection legend="商品">
                        {!state.skuLocked && state.canFilterSku ? (
                            <ListWorkspaceFilterField
                                htmlFor="supplier-offerings-toolbar-sku-select"
                                label="公司 SKU"
                            >
                                <CompanySkuSearchCombobox
                                    id="supplier-offerings-toolbar-sku-select"
                                    value={state.skuIdDraft ?? undefined}
                                    onValueChange={(value) =>
                                        state.setSkuIdDraft(value ?? null)
                                    }
                                    placeholder="全部"
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
                                value={state.skuNoDraft}
                                onChange={(event) =>
                                    state.setSkuNoDraft(event.target.value)
                                }
                                autoComplete="off"
                                placeholder="如 SKU-001"
                            />
                        </ListWorkspaceFilterField>
                        <ListWorkspaceFilterField
                            htmlFor="supplier-offerings-toolbar-product-no"
                            label="SPU 编号"
                        >
                            <Input
                                id="supplier-offerings-toolbar-product-no"
                                className="w-full min-w-0"
                                value={state.productNoDraft}
                                onChange={(event) =>
                                    state.setProductNoDraft(event.target.value)
                                }
                                autoComplete="off"
                                placeholder="如 P-1001"
                            />
                        </ListWorkspaceFilterField>
                    </ListFilterSection>
                    <ListFilterSection legend="来源">
                        <ListWorkspaceFilterField
                            htmlFor="supplier-offerings-toolbar-filter-source-type"
                            label="登记来源"
                        >
                            <OptionCombobox
                                id="supplier-offerings-toolbar-filter-source-type"
                                className="w-full min-w-0"
                                value={
                                    state.sourceTypeDraft === "all"
                                        ? null
                                        : state.sourceTypeDraft
                                }
                                options={SOURCE_OPTIONS}
                                onValueChange={(value) =>
                                    state.setSourceTypeDraft(
                                        SOURCE_OPTIONS.find(
                                            (option) => option.value === value,
                                        )?.value ?? "all",
                                    )
                                }
                                placeholder="全部"
                                aria-label="登记来源"
                            />
                        </ListWorkspaceFilterField>
                    </ListFilterSection>
                </ListFilterSections>
            }
            resultStatus={resultStatus}
            chips={foldedChips.map((chip) => ({
                ...chip,
                onClear: () => state.removeFilter(chip.key),
            }))}
            onClearAll={showClearAll ? state.clearFilters : undefined}
            clearButtonId="supplier-offerings-toolbar-clear-all"
            hasPendingChanges={state.hasPendingChanges}
        />
    )
}
