"use client"

import { useEffect, useRef, type RefObject } from "react"

import { CategoryCombobox, OptionCombobox } from "@/components/business"
import {
    ListFilterRangeField,
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
} from "@/components/business/list-workspace"
import { Checkbox } from "@/components/ui/checkbox"
import type { useSellableListState } from "@/features/master-data/hooks/use-sellable-list-state"
import { PRODUCT_KIND_FILTER_OPTIONS } from "@/features/master-data/lib/list-filters"
import {
    directoryOptionEmptyLabel,
    productFilterDirectoryState,
} from "@/features/master-data/lib/product-filter-directory"
import { parseSellableDeliveryExtent } from "@/features/master-data/lib/sellable-delivery-filter"
import { RegionCascadeSelect } from "@/features/regions"
import { DELIVERY_EXTENT_OPTIONS } from "@/lib/delivery-coverage"
import { PRODUCT_KIND_VALUES } from "@/features/master-data/types"
import { PRICE_FILTER_OPTIONS, parsePriceFilterKind } from "@/lib/price-filter"

import { DirectoryFilterControl } from "./directory-filter-control"
import {
    brandDirectoryFilterBindings,
    DirectoryOptionFilter,
} from "./directory-option-filter"
import type { SellableAppliedChip } from "./sellable-list-toolbar"

const prefix = "sellable-items-filter"
const foldedChipKeys = ["productBrandId", "deliveryRegion", "salesPrice"]

type ListState = ReturnType<typeof useSellableListState>

export function SellableItemsListToolbar({
    searchInputRef,
    filters,
    appliedChips,
    filterOptions,
    loading,
    failed,
}: {
    searchInputRef: RefObject<HTMLInputElement | null>
    filters: ListState["filters"]
    appliedChips: ListState["appliedChips"]
    filterOptions: ListState["productFilterOptionsQuery"]
    loading: boolean
    failed: boolean
}) {
    const directory = productFilterDirectoryState(filterOptions)
    const refreshDirectory = () => filterOptions.refetch({ throwOnError: true })
    const priceInputRef = useRef<HTMLInputElement>(null)
    const { productSalesPriceError, sellableFilterPanelOpen } = filters
    useEffect(() => {
        if (productSalesPriceError && sellableFilterPanelOpen) {
            priceInputRef.current?.focus()
        }
    }, [productSalesPriceError, sellableFilterPanelOpen])

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            idPrefix={prefix}
            formAriaLabel="公司商品池查询"
            onSubmit={filters.applySellableFilters}
            resetMoreButtonId={`${prefix}-reset-more`}
            search={
                <ListSearchField
                    id={`${prefix}-search`}
                    searchInputRef={searchInputRef}
                    value={filters.searchDraft}
                    onChange={filters.setSearchDraft}
                    placeholder="搜索商品名称、SKU 或编号"
                    aria-label="搜索基础资料"
                />
            }
            primaryFilters={
                <>
                    <OptionCombobox
                        id={`${prefix}-kind`}
                        className="w-36 max-w-full min-w-0"
                        filterLabel="商品类型"
                        aria-label="商品类型"
                        value={
                            filters.productKindDraft === "all"
                                ? null
                                : filters.productKindDraft
                        }
                        options={PRODUCT_KIND_FILTER_OPTIONS}
                        placeholder="全部"
                        onValueChange={(value) =>
                            filters.setProductKindDraft(
                                PRODUCT_KIND_VALUES.find(
                                    (kind) => kind === value,
                                ) ?? "all",
                            )
                        }
                    />
                    {filters.canFilterCategory ? (
                        <DirectoryFilterControl
                            id={`${prefix}-category`}
                            className="w-44 max-w-full"
                            failed={directory.failed}
                            unavailable={directory.categoryUnavailable}
                            noScope={directory.categoryNoScope}
                            error={directory.error}
                            onRetry={directory.refetch}
                        >
                            <CategoryCombobox
                                onRefresh={refreshDirectory}
                                id={`${prefix}-category`}
                                className="w-full min-w-0"
                                filterLabel="分类"
                                categories={directory.categories}
                                value={
                                    filters.productCategoryIdDraft ?? undefined
                                }
                                onValueChange={(value) =>
                                    filters.setProductCategoryIdDraft(
                                        value ?? null,
                                    )
                                }
                                loading={directory.fetching}
                                retainMissingSelection={false}
                                placeholder="全部"
                                emptyLabel={directoryOptionEmptyLabel({
                                    unavailable: directory.categoryUnavailable,
                                    unavailableLabel: "当前账号无分类查询权限",
                                    noScope: directory.categoryNoScope,
                                    failed: directory.failed,
                                    emptyLabel: "没有符合条件的分类",
                                })}
                            />
                        </DirectoryFilterControl>
                    ) : null}
                </>
            }
            moreOpen={sellableFilterPanelOpen}
            onToggleMore={() => {
                if (sellableFilterPanelOpen) filters.cancelMoreFilters()
                else filters.setSellableFilterPanelOpen(true)
            }}
            morePanelAriaLabel="更多筛选条件"
            onResetMore={filters.resetMoreFilters}
            morePanel={
                <ListFilterSections>
                    {filters.canFilterBrand && !directory.brandUnavailable ? (
                        <ListFilterSection legend="品牌">
                            <DirectoryOptionFilter
                                {...brandDirectoryFilterBindings(
                                    directory,
                                    refreshDirectory,
                                )}
                                id={`${prefix}-brand`}
                                comboboxClassName="w-full"
                                ariaLabel="品牌"
                                value={filters.productBrandIdDraft}
                                onValueChange={filters.setProductBrandIdDraft}
                            />
                        </ListFilterSection>
                    ) : null}
                    <DeliveryFilters filters={filters} />
                    <ListFilterSection legend="价格筛选">
                        <ListWorkspaceFilterField
                            htmlFor={`${prefix}-price-kind`}
                            label="价格类型"
                        >
                            <OptionCombobox
                                id={`${prefix}-price-kind`}
                                className="w-full min-w-0"
                                aria-label="价格类型"
                                value={filters.productPriceKindDraft}
                                onValueChange={(value) =>
                                    filters.setProductPriceKindDraft(
                                        parsePriceFilterKind(value),
                                    )
                                }
                                allowClear={false}
                                options={PRICE_FILTER_OPTIONS}
                                placeholder="一件代发价"
                            />
                        </ListWorkspaceFilterField>
                        <ListFilterRangeField
                            label="含税价"
                            minId={`${prefix}-price-min`}
                            maxId={`${prefix}-price-max`}
                            errorId={`${prefix}-price-error`}
                            minAriaLabel="最低价"
                            maxAriaLabel="最高价"
                            minPlaceholder="最低价"
                            maxPlaceholder="最高价"
                            unit="元"
                            inputRef={priceInputRef}
                            min={filters.productSalesPriceMinDraft}
                            max={filters.productSalesPriceMaxDraft}
                            error={productSalesPriceError}
                            fieldClassName="md:col-span-2"
                            onMinChange={(value) => {
                                filters.setProductSalesPriceMinDraft(value)
                                filters.setProductSalesPriceError(null)
                            }}
                            onMaxChange={(value) => {
                                filters.setProductSalesPriceMaxDraft(value)
                                filters.setProductSalesPriceError(null)
                            }}
                        />
                    </ListFilterSection>
                </ListFilterSections>
            }
            resultStatus={failed ? "查询未完成" : loading ? "查询中…" : null}
            chips={appliedChips}
            foldedChipKeys={foldedChipKeys}
            onClearChip={(key) =>
                filters.removeFilter(key as SellableAppliedChip["key"])
            }
            onClearAll={filters.clearAllFilters}
            hasPendingChanges={filters.hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
        />
    )
}

function DeliveryFilters({ filters: f }: { filters: ListState["filters"] }) {
    const disabled = !f.deliveryRegionDraft
    return (
        <ListFilterSection legend="配送地区">
            <ListWorkspaceFilterField
                htmlFor={`${prefix}-delivery-region-province`}
                label="能送到"
                className="md:col-span-2"
            >
                <RegionCascadeSelect
                    idPrefix={`${prefix}-delivery-region`}
                    ariaLabel="配送地区"
                    value={f.deliveryRegionDraft}
                    onValueChange={f.setDeliveryRegionDraft}
                />
            </ListWorkspaceFilterField>
            <ListWorkspaceFilterField
                htmlFor={`${prefix}-delivery-extent`}
                label="范围"
            >
                <OptionCombobox
                    id={`${prefix}-delivery-extent`}
                    className="w-full min-w-0"
                    aria-label="配送范围"
                    value={f.deliveryExtentDraft}
                    onValueChange={(value) =>
                        f.setDeliveryExtentDraft(
                            parseSellableDeliveryExtent(value),
                        )
                    }
                    allowClear={false}
                    disabled={disabled}
                    options={DELIVERY_EXTENT_OPTIONS}
                    placeholder="全部可送"
                />
            </ListWorkspaceFilterField>
            <label
                htmlFor={`${prefix}-delivery-freight-extra`}
                className="flex min-h-control items-center gap-2 text-body-compact"
            >
                <Checkbox
                    id={`${prefix}-delivery-freight-extra`}
                    checked={f.freightExtraDraft}
                    disabled={disabled}
                    onCheckedChange={(checked) =>
                        f.setFreightExtraDraft(checked === true)
                    }
                />
                包含需另付运费的地区
            </label>
        </ListFilterSection>
    )
}
