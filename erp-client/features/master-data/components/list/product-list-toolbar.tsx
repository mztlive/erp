"use client"

import * as React from "react"

import {
    CategoryCombobox,
    FixedOptionRadioFilter,
    OptionCombobox,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspaceInlineFilter,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Input } from "@/components/ui/input"
import type { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"
import type { ProductAppliedChip } from "@/features/master-data/hooks/use-product-list-state"
import type { useProductListFilters } from "@/features/master-data/hooks/use-product-list-filters"
import { masterDataSearchPlaceholder } from "@/features/master-data/lib/copy"
import {
    PRODUCT_COVERAGE_RADIO_FILTER_OPTIONS,
    PRODUCT_KIND_RADIO_FILTER_OPTIONS,
    PRODUCT_LISTING_RADIO_FILTER_OPTIONS,
    REVISION_TIMING_RADIO_FILTER_OPTIONS,
} from "@/features/master-data/lib/list-filters"

const MORE_CHIP_KEYS = [
    "revisionTiming",
    "productListingStatus",
    "productSupplyCoverage",
    "productBrandId",
    "productSupplierId",
    "salesPrice",
] as const

export function ProductListToolbar({
    idPrefix,
    searchInputRef,
    filters: f,
    appliedChips,
    productFilterOptionsQuery,
    resultCount,
    loading,
    failed,
}: {
    idPrefix?: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    filters: ReturnType<typeof useProductListFilters>
    appliedChips: readonly ProductAppliedChip[]
    productFilterOptionsQuery: ReturnType<typeof useProductFilterOptionsQuery>
    resultCount?: number
    loading: boolean
    failed: boolean
}) {
    const prefix = idPrefix ?? "master-data-list-product-list-toolbar"
    const panelId = `${prefix}-more-panel`
    const priceErrorId = `${prefix}-price-error`
    const priceInputRef = React.useRef<HTMLInputElement>(null)
    const moreCount = appliedChips.filter(({ key }) =>
        MORE_CHIP_KEYS.includes(key as (typeof MORE_CHIP_KEYS)[number]),
    ).length

    React.useEffect(() => {
        if (f.productSalesPriceError && f.productFilterPanelOpen) {
            priceInputRef.current?.focus()
        }
    }, [f.productSalesPriceError, f.productFilterPanelOpen])

    return (
        <ListWorkspaceFilterBar
            idPrefix={prefix}
            formAriaLabel="商品列表查询"
            onSubmit={f.applyProductFilters}
            search={
                <ListSearchField
                    id={`${prefix}-search-input`}
                    searchInputRef={searchInputRef}
                    value={f.searchDraft}
                    onChange={f.setSearchDraft}
                    placeholder={masterDataSearchPlaceholder("products")}
                    aria-label="搜索基础资料"
                />
            }
            moreCount={moreCount}
            moreOpen={f.productFilterPanelOpen}
            onToggleMore={() => f.setProductFilterPanelOpen((open) => !open)}
            morePanelId={panelId}
            morePanelAriaLabel="商品列表更多筛选条件"
            moreButtonId={`${prefix}-filter-trigger`}
            resetMoreButtonId={`${prefix}-reset`}
            clearButtonId={`${prefix}-clear-filters`}
            onResetMore={f.resetMoreFilters}
            commonFilters={
                <>
                    <FixedOptionRadioFilter
                        idPrefix={`${prefix}-kind`}
                        label="类型"
                        variant="quiet"
                        value={f.productKindDraft}
                        onValueChange={f.setProductKindDraft}
                        options={PRODUCT_KIND_RADIO_FILTER_OPTIONS}
                    />
                    <ListWorkspaceInlineFilter
                        htmlFor={`${prefix}-category`}
                        label="分类"
                    >
                        <CategoryCombobox
                            id={`${prefix}-category`}
                            className="w-full sm:w-60"
                            aria-label="商品分类"
                            categories={
                                productFilterOptionsQuery.data?.categories ?? []
                            }
                            value={f.productCategoryIdDraft ?? undefined}
                            onValueChange={(id) =>
                                f.setProductCategoryIdDraft(id ?? null)
                            }
                            loading={productFilterOptionsQuery.isPending}
                            placeholder="全部分类"
                        />
                    </ListWorkspaceInlineFilter>
                </>
            }
            morePanel={
                <div className="grid min-w-0 gap-5">
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            状态
                        </legend>
                        <div className="grid min-w-0 gap-3">
                            <FixedOptionRadioFilter
                                label="版本"
                                value={f.revisionTimingDraft}
                                onValueChange={f.setRevisionTimingDraft}
                                options={REVISION_TIMING_RADIO_FILTER_OPTIONS}
                                aria-label="版本状态"
                            />
                            <FixedOptionRadioFilter
                                label="上架"
                                value={f.productListingStatusDraft}
                                onValueChange={f.setProductListingStatusDraft}
                                options={PRODUCT_LISTING_RADIO_FILTER_OPTIONS}
                            />
                            <FixedOptionRadioFilter
                                label="供给覆盖"
                                value={f.productSupplyCoverageDraft}
                                onValueChange={f.setProductSupplyCoverageDraft}
                                options={PRODUCT_COVERAGE_RADIO_FILTER_OPTIONS}
                            />
                        </div>
                    </fieldset>
                    <div className="grid min-w-0 gap-5 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
                        <fieldset className="min-w-0">
                            <legend className="mb-3 text-xs font-medium">
                                品牌与供货
                            </legend>
                            <div className="grid min-w-0 gap-3 sm:grid-cols-2">
                                <ListWorkspaceFilterField
                                    htmlFor={`${prefix}-brand`}
                                    label="品牌"
                                >
                                    <OptionCombobox
                                        id={`${prefix}-brand`}
                                        className="w-full"
                                        value={f.productBrandIdDraft}
                                        aria-label="商品品牌"
                                        onValueChange={f.setProductBrandIdDraft}
                                        options={
                                            productFilterOptionsQuery.data
                                                ?.brands ?? []
                                        }
                                        loading={
                                            productFilterOptionsQuery.isPending
                                        }
                                        placeholder="全部品牌"
                                        searchPlaceholder="搜索品牌名称或代码"
                                    />
                                </ListWorkspaceFilterField>
                                <ListWorkspaceFilterField
                                    htmlFor={`${prefix}-supplier`}
                                    label="供应商"
                                >
                                    <OptionCombobox
                                        id={`${prefix}-supplier`}
                                        className="w-full"
                                        value={f.productSupplierIdDraft}
                                        aria-label="供应商"
                                        onValueChange={
                                            f.setProductSupplierIdDraft
                                        }
                                        options={
                                            productFilterOptionsQuery.data
                                                ?.suppliers ?? []
                                        }
                                        loading={
                                            productFilterOptionsQuery.isPending
                                        }
                                        placeholder="全部供应商"
                                        searchPlaceholder="搜索供应商名称或代码"
                                    />
                                </ListWorkspaceFilterField>
                            </div>
                        </fieldset>
                        <fieldset className="min-w-0 lg:border-l lg:pl-5">
                            <legend className="mb-3 text-xs font-medium">
                                销售价格
                            </legend>
                            <ListWorkspaceFilterField label="含税售价 · 元">
                                <div className="flex min-w-0 items-center gap-2">
                                    <Input
                                        id={`${prefix}-price-min`}
                                        ref={priceInputRef}
                                        className="w-0 min-w-0 flex-1"
                                        value={f.productSalesPriceMinDraft}
                                        onChange={(event) => {
                                            f.setProductSalesPriceMinDraft(
                                                event.target.value,
                                            )
                                            f.setProductSalesPriceError(null)
                                        }}
                                        inputMode="decimal"
                                        autoComplete="off"
                                        placeholder="最低价"
                                        aria-label="最低销售价"
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
                                        className="w-0 min-w-0 flex-1"
                                        value={f.productSalesPriceMaxDraft}
                                        onChange={(event) => {
                                            f.setProductSalesPriceMaxDraft(
                                                event.target.value,
                                            )
                                            f.setProductSalesPriceError(null)
                                        }}
                                        inputMode="decimal"
                                        autoComplete="off"
                                        placeholder="最高价"
                                        aria-label="最高销售价"
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
                                        className="text-xs text-destructive"
                                        role="alert"
                                    >
                                        {f.productSalesPriceError}
                                    </p>
                                ) : null}
                            </ListWorkspaceFilterField>
                        </fieldset>
                    </div>
                </div>
            }
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "条",
            })}
            chips={appliedChips}
            onClearChip={(key) =>
                f.removeFilter(key as ProductAppliedChip["key"])
            }
            onClearAll={f.clearAllFilters}
            hasPendingChanges={f.hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
            idleHint="导出与当前查询结果一致"
        />
    )
}
