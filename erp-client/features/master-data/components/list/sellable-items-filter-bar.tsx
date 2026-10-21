"use client"

import * as React from "react"
import { PRICE_FILTER_OPTIONS, parsePriceFilterKind } from "@/lib/price-filter"

import { CategoryCombobox, OptionCombobox } from "@/components/business"
import { DirectoryFilterControl } from "@/features/master-data/components/list/directory-filter-control"
import {
    directoryOptionEmptyLabel,
    productFilterDirectoryState,
    type ProductFilterDirectoryQuery,
} from "@/features/master-data/lib/product-filter-directory"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Input } from "@/components/ui/input"
import type { SellableAppliedChip } from "./sellable-list-toolbar"
import {
    SellableDeliveryFilterFields,
    type SellableDeliveryFilterControls,
} from "./sellable-delivery-filter-fields"
import type { useSellableListFilters } from "@/features/master-data/hooks/use-sellable-list-filters"
import { PRODUCT_KIND_FILTER_OPTIONS } from "@/features/master-data/lib/list-filters"
import type { ProductKind } from "@/features/master-data/types"

export type SellableItemsFilterControls = Pick<
    ReturnType<typeof useSellableListFilters>,
    | "applySellableFilters"
    | "cancelMoreFilters"
    | "clearAllFilters"
    | "hasPendingChanges"
    | "productBrandIdDraft"
    | "productCategoryIdDraft"
    | "productKindDraft"
    | "productPriceKindDraft"
    | "productSalesPriceError"
    | "productSalesPriceMaxDraft"
    | "productSalesPriceMinDraft"
    | "removeFilter"
    | "resetMoreFilters"
    | "searchDraft"
    | "sellableFilterPanelOpen"
    | "setProductBrandIdDraft"
    | "setProductCategoryIdDraft"
    | "setProductKindDraft"
    | "setProductPriceKindDraft"
    | "setProductSalesPriceError"
    | "setProductSalesPriceMaxDraft"
    | "setProductSalesPriceMinDraft"
    | "setSearchDraft"
    | "setSellableFilterPanelOpen"
>

export function SellableItemsFilterBar({
    idPrefix = "sellable-items-filter",
    formAriaLabel = "公司商品池查询",
    pendingHint = "条件已修改，待查询 · 导出仍按已生效条件",
    hiddenProductKinds,
    searchInputRef,
    filters: f,
    deliveryFilter,
    appliedChips,
    filterOptions,
    resultCount,
    loading,
    failed,
    statusActions,
}: {
    idPrefix?: string
    formAriaLabel?: string
    pendingHint?: string
    hiddenProductKinds?: readonly ProductKind[]
    searchInputRef: React.RefObject<HTMLInputElement | null>
    filters: SellableItemsFilterControls
    /** 公司商品池按配送地区筛选；不传则不显示（销售选品不支持）。 */
    deliveryFilter?: SellableDeliveryFilterControls
    appliedChips: readonly SellableAppliedChip[]
    filterOptions: ProductFilterDirectoryQuery
    resultCount?: number
    loading: boolean
    failed: boolean
    statusActions?: React.ReactNode
}) {
    const prefix = idPrefix
    const panelId = `${prefix}-more-panel`
    const priceErrorId = `${prefix}-price-error`
    const kindOptions = PRODUCT_KIND_FILTER_OPTIONS.filter(
        (option) => !hiddenProductKinds?.includes(option.value as ProductKind),
    )
    const directory = productFilterDirectoryState(filterOptions)
    const priceInputRef = React.useRef<HTMLInputElement>(null)
    const moreCount = appliedChips.filter(({ key }) =>
        ["productBrandId", "deliveryRegion", "salesPrice"].includes(key),
    ).length
    React.useEffect(() => {
        if (f.productSalesPriceError && f.sellableFilterPanelOpen)
            priceInputRef.current?.focus()
    }, [f.productSalesPriceError, f.sellableFilterPanelOpen])

    return (
        <ListWorkspaceFilterBar
            morePresentation="popover"
            moreSize="wide"
            className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
            idPrefix={prefix}
            formAriaLabel={formAriaLabel}
            onSubmit={f.applySellableFilters}
            queryButtonId={`${prefix}-query`}
            search={
                <ListSearchField
                    id={`${prefix}-search`}
                    searchInputRef={searchInputRef}
                    value={f.searchDraft}
                    onChange={f.setSearchDraft}
                    placeholder="搜索商品名称、SKU 或编号"
                    aria-label="搜索基础资料"
                />
            }
            moreCount={moreCount}
            moreOpen={f.sellableFilterPanelOpen}
            onToggleMore={() =>
                f.sellableFilterPanelOpen
                    ? f.cancelMoreFilters()
                    : f.setSellableFilterPanelOpen(true)
            }
            morePanelId={panelId}
            morePanelAriaLabel="更多筛选条件"
            moreButtonId={`${prefix}-more`}
            resetMoreButtonId={`${prefix}-reset-more`}
            clearButtonId={`${prefix}-clear`}
            onResetMore={f.resetMoreFilters}
            primaryFilters={
                <>
                    <OptionCombobox
                        id={`${prefix}-kind`}
                        className="w-48 max-w-full min-w-0"
                        filterLabel="商品类型"
                        aria-label="商品类型"
                        value={
                            f.productKindDraft === "all"
                                ? null
                                : f.productKindDraft
                        }
                        options={kindOptions}
                        placeholder="全部"
                        onValueChange={(value) =>
                            f.setProductKindDraft(
                                kindOptions.some(
                                    (option) => option.value === value,
                                )
                                    ? (value as ProductKind)
                                    : "all",
                            )
                        }
                    />
                    <DirectoryFilterControl
                        id={`${prefix}-category`}
                        className="w-56 max-w-full"
                        failed={directory.failed}
                        unavailable={directory.categoryUnavailable}
                        noScope={directory.categoryNoScope}
                        error={directory.error}
                        onRetry={directory.refetch}
                    >
                        <CategoryCombobox
                            onRefresh={() =>
                                filterOptions.refetch({ throwOnError: true })
                            }
                            id={`${prefix}-category`}
                            className="w-full min-w-0"
                            filterLabel="分类"
                            categories={directory.categories}
                            value={f.productCategoryIdDraft ?? undefined}
                            onValueChange={(value) =>
                                f.setProductCategoryIdDraft(value ?? null)
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
                </>
            }
            morePanel={
                <div className="grid min-w-0 gap-5">
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            品牌与供货
                        </legend>
                        <div className="grid min-w-0 gap-3 sm:grid-cols-2">
                            <ListWorkspaceFilterField
                                htmlFor={`${prefix}-brand`}
                                label="品牌"
                            >
                                <DirectoryFilterControl
                                    id={`${prefix}-brand`}
                                    failed={directory.failed}
                                    unavailable={directory.brandUnavailable}
                                    noScope={directory.brandNoScope}
                                    error={directory.error}
                                    onRetry={directory.refetch}
                                >
                                    <OptionCombobox
                                        onRefresh={() =>
                                            filterOptions.refetch({
                                                throwOnError: true,
                                            })
                                        }
                                        id={`${prefix}-brand`}
                                        className="w-full"
                                        aria-label="品牌"
                                        value={f.productBrandIdDraft}
                                        onValueChange={f.setProductBrandIdDraft}
                                        options={directory.brands}
                                        loading={directory.fetching}
                                        retainMissingSelection={false}
                                        placeholder="全部品牌"
                                        emptyLabel={directoryOptionEmptyLabel({
                                            unavailable:
                                                directory.brandUnavailable,
                                            unavailableLabel:
                                                "当前账号无品牌查询权限",
                                            noScope: directory.brandNoScope,
                                            failed: directory.failed,
                                            emptyLabel: "没有符合条件的品牌",
                                        })}
                                        searchPlaceholder="搜索品牌名称或代码"
                                    />
                                </DirectoryFilterControl>
                            </ListWorkspaceFilterField>
                        </div>
                    </fieldset>
                    {deliveryFilter ? (
                        <SellableDeliveryFilterFields
                            idPrefix={prefix}
                            filters={deliveryFilter}
                        />
                    ) : null}
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            价格筛选
                        </legend>
                        <ListWorkspaceFilterField
                            className="mb-3"
                            htmlFor={`${prefix}-price-kind`}
                            label="价格类型"
                        >
                            <OptionCombobox
                                id={`${prefix}-price-kind`}
                                className="w-full"
                                aria-label="价格类型"
                                value={f.productPriceKindDraft}
                                onValueChange={(value) =>
                                    f.setProductPriceKindDraft(
                                        parsePriceFilterKind(value),
                                    )
                                }
                                allowClear={false}
                                options={PRICE_FILTER_OPTIONS}
                                placeholder="一件代发价"
                            />
                        </ListWorkspaceFilterField>
                        <ListWorkspaceFilterField label="含税价格 · 元">
                            <div className="flex min-w-0 items-center gap-2">
                                <Input
                                    id={`${prefix}-price-min`}
                                    ref={priceInputRef}
                                    className="h-control w-0 min-w-0 flex-1"
                                    aria-label="最低价"
                                    placeholder="最低价"
                                    inputMode="decimal"
                                    autoComplete="off"
                                    value={f.productSalesPriceMinDraft}
                                    onChange={(event) => {
                                        f.setProductSalesPriceMinDraft(
                                            event.target.value,
                                        )
                                        f.setProductSalesPriceError(null)
                                    }}
                                    aria-invalid={Boolean(
                                        f.productSalesPriceError,
                                    )}
                                    aria-describedby={
                                        f.productSalesPriceError
                                            ? priceErrorId
                                            : undefined
                                    }
                                />
                                <span className="text-xs text-muted-foreground">
                                    至
                                </span>
                                <Input
                                    id={`${prefix}-price-max`}
                                    className="h-control w-0 min-w-0 flex-1"
                                    aria-label="最高价"
                                    placeholder="最高价"
                                    inputMode="decimal"
                                    autoComplete="off"
                                    value={f.productSalesPriceMaxDraft}
                                    onChange={(event) => {
                                        f.setProductSalesPriceMaxDraft(
                                            event.target.value,
                                        )
                                        f.setProductSalesPriceError(null)
                                    }}
                                    aria-invalid={Boolean(
                                        f.productSalesPriceError,
                                    )}
                                    aria-describedby={
                                        f.productSalesPriceError
                                            ? priceErrorId
                                            : undefined
                                    }
                                />
                            </div>
                            {f.productSalesPriceError ? (
                                <p
                                    id={priceErrorId}
                                    role="alert"
                                    className="text-xs text-destructive"
                                >
                                    {f.productSalesPriceError}
                                </p>
                            ) : null}
                        </ListWorkspaceFilterField>
                    </fieldset>
                </div>
            }
            resultStatus={
                loading || failed
                    ? listWorkspaceFilterStatusText({
                          loading,
                          failed,
                          resultCount,
                          noun: "件商品",
                          loadingLabel: "正在加载商品…",
                      })
                    : null
            }
            chips={appliedChips}
            onClearChip={(key) =>
                f.removeFilter(key as SellableAppliedChip["key"])
            }
            onClearAll={f.clearAllFilters}
            hasPendingChanges={f.hasPendingChanges}
            pendingHint={pendingHint}
            statusActions={statusActions}
        />
    )
}
