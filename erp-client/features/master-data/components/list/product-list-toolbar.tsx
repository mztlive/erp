"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    CategoryCombobox,
    FilterChip,
    FixedOptionRadioFilter,
    ListToolbar,
    OptionCombobox,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ListSearchField } from "@/features/master-data/components/list/list-search-field"
import { masterDataSearchPlaceholder } from "@/features/master-data/lib/copy"
import {
    LIFECYCLE_RADIO_FILTER_OPTIONS,
    PRODUCT_COVERAGE_RADIO_FILTER_OPTIONS,
    PRODUCT_KIND_RADIO_FILTER_OPTIONS,
    PRODUCT_LISTING_RADIO_FILTER_OPTIONS,
    REVISION_TIMING_RADIO_FILTER_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import type { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"
import type { ProductAppliedChip } from "@/features/master-data/hooks/use-product-list-state"
import type { ProductFilterKey } from "@/features/master-data/hooks/use-product-list-filters"
import type {
    ProductKind,
    ProductListingFilter,
    ProductSkuCoverageFilter,
} from "@/features/master-data/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export function ProductListToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    hasActiveFilters,
    clearAllFilters,
    appliedChips,
    removeFilter,
    productFilterPanelOpen,
    setProductFilterPanelOpen,
    hasStructuredProductFilters,
    applyProductFilters,
    resetMoreFilters,
    productKindDraft,
    setProductKindDraft,
    lifecycleStatusDraft,
    setLifecycleStatusDraft,
    revisionTimingDraft,
    setRevisionTimingDraft,
    productListingStatusDraft,
    setProductListingStatusDraft,
    productSupplyCoverageDraft,
    setProductSupplyCoverageDraft,
    productCategoryIdDraft,
    setProductCategoryIdDraft,
    productBrandIdDraft,
    setProductBrandIdDraft,
    productSupplierIdDraft,
    setProductSupplierIdDraft,
    productSalesPriceMinDraft,
    setProductSalesPriceMinDraft,
    productSalesPriceMaxDraft,
    setProductSalesPriceMaxDraft,
    productSalesPriceError,
    setProductSalesPriceError,
    productFilterOptionsQuery,
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    hasActiveFilters: boolean
    clearAllFilters: () => void
    appliedChips: readonly ProductAppliedChip[]
    removeFilter: (key: ProductFilterKey) => void
    productFilterPanelOpen: boolean
    setProductFilterPanelOpen: SetState<boolean>
    hasStructuredProductFilters: boolean
    applyProductFilters: () => void
    resetMoreFilters: () => void
    productKindDraft: ProductKind | "all"
    setProductKindDraft: SetState<ProductKind | "all">
    lifecycleStatusDraft: "enabled" | "disabled" | "all"
    setLifecycleStatusDraft: SetState<"enabled" | "disabled" | "all">
    revisionTimingDraft: "current" | "future" | "all"
    setRevisionTimingDraft: SetState<"current" | "future" | "all">
    productListingStatusDraft: ProductListingFilter | "all"
    setProductListingStatusDraft: SetState<ProductListingFilter | "all">
    productSupplyCoverageDraft: ProductSkuCoverageFilter | "all"
    setProductSupplyCoverageDraft: SetState<ProductSkuCoverageFilter | "all">
    productCategoryIdDraft: string | null
    setProductCategoryIdDraft: SetState<string | null>
    productBrandIdDraft: string | null
    setProductBrandIdDraft: SetState<string | null>
    productSupplierIdDraft: string | null
    setProductSupplierIdDraft: SetState<string | null>
    productSalesPriceMinDraft: string
    setProductSalesPriceMinDraft: SetState<string>
    productSalesPriceMaxDraft: string
    setProductSalesPriceMaxDraft: SetState<string>
    productSalesPriceError: string | null
    setProductSalesPriceError: SetState<string | null>
    productFilterOptionsQuery: ReturnType<typeof useProductFilterOptionsQuery>
}) {
    const panelId = React.useId()
    const priceErrorId = React.useId()
    const hasChips = hasActiveFilters && appliedChips.length > 0

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applyProductFilters()
            }}
        >
            <ListToolbar
                search={
                    <ListSearchField
                        searchInputRef={searchInputRef}
                        value={searchDraft}
                        onChange={setSearchDraft}
                        placeholder={masterDataSearchPlaceholder("products")}
                    />
                }
                filters={
                    <Button
                        type="button"
                        variant="outline"
                        aria-expanded={productFilterPanelOpen}
                        aria-controls={panelId}
                        onClick={() =>
                            setProductFilterPanelOpen((open) => !open)
                        }
                    >
                        <FilterIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        更多筛选
                        {hasStructuredProductFilters ? (
                            <Badge variant="info">已启用</Badge>
                        ) : null}
                        <ChevronDownIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                            className={
                                productFilterPanelOpen
                                    ? "rotate-180 transition-transform"
                                    : "transition-transform"
                            }
                        />
                    </Button>
                }
                secondary={
                    hasChips || productFilterPanelOpen ? (
                        <div className="w-full space-y-3">
                            {hasChips ? (
                                <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                                    <span className="text-xs text-muted-foreground">
                                        已筛选
                                    </span>
                                    {appliedChips.map((chip) => (
                                        <FilterChip
                                            key={chip.key}
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                removeFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={clearAllFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {productFilterPanelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="商品列表更多筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        label="类型"
                                        value={productKindDraft}
                                        onValueChange={setProductKindDraft}
                                        options={PRODUCT_KIND_RADIO_FILTER_OPTIONS}
                                    />
                                    <FixedOptionRadioFilter
                                        label="启停"
                                        value={lifecycleStatusDraft}
                                        onValueChange={setLifecycleStatusDraft}
                                        options={LIFECYCLE_RADIO_FILTER_OPTIONS}
                                        aria-label="启用状态"
                                    />
                                    <FixedOptionRadioFilter
                                        label="版本"
                                        value={revisionTimingDraft}
                                        onValueChange={setRevisionTimingDraft}
                                        options={REVISION_TIMING_RADIO_FILTER_OPTIONS}
                                        aria-label="版本状态"
                                    />
                                    <FixedOptionRadioFilter
                                        label="上架"
                                        value={productListingStatusDraft}
                                        onValueChange={setProductListingStatusDraft}
                                        options={PRODUCT_LISTING_RADIO_FILTER_OPTIONS}
                                    />
                                    <FixedOptionRadioFilter
                                        label="供给覆盖"
                                        value={productSupplyCoverageDraft}
                                        onValueChange={setProductSupplyCoverageDraft}
                                        options={PRODUCT_COVERAGE_RADIO_FILTER_OPTIONS}
                                    />
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                分类
                                            </span>
                                            <CategoryCombobox
                                                className="w-full"
                                                categories={
                                                    productFilterOptionsQuery
                                                        .data?.categories ?? []
                                                }
                                                value={
                                                    productCategoryIdDraft ??
                                                    undefined
                                                }
                                                onValueChange={(id) =>
                                                    setProductCategoryIdDraft(
                                                        id ?? null,
                                                    )
                                                }
                                                loading={
                                                    productFilterOptionsQuery.isPending
                                                }
                                                placeholder="全部分类"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                品牌
                                            </span>
                                            <OptionCombobox
                                                className="w-full"
                                                value={productBrandIdDraft}
                                                aria-label="商品品牌"
                                                onValueChange={
                                                    setProductBrandIdDraft
                                                }
                                                options={
                                                    productFilterOptionsQuery
                                                        .data?.brands ?? []
                                                }
                                                loading={
                                                    productFilterOptionsQuery.isPending
                                                }
                                                placeholder="全部品牌"
                                                searchPlaceholder="搜索品牌名称或代码"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                供应商
                                            </span>
                                            <OptionCombobox
                                                className="w-full"
                                                value={productSupplierIdDraft}
                                                aria-label="供应商"
                                                onValueChange={
                                                    setProductSupplierIdDraft
                                                }
                                                options={
                                                    productFilterOptionsQuery
                                                        .data?.suppliers ?? []
                                                }
                                                loading={
                                                    productFilterOptionsQuery.isPending
                                                }
                                                placeholder="全部供应商"
                                                searchPlaceholder="搜索供应商名称或代码"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                销售价
                                            </span>
                                            <div className="flex items-center gap-1.5">
                                                <Input
                                                    className="w-0 min-w-0 flex-1"
                                                    value={productSalesPriceMinDraft}
                                                    onChange={(event) => {
                                                        setProductSalesPriceMinDraft(
                                                            event.target.value,
                                                        )
                                                        setProductSalesPriceError(null)
                                                    }}
                                                    inputMode="decimal"
                                                    autoComplete="off"
                                                    placeholder="最低价"
                                                    aria-label="最低销售价"
                                                    aria-invalid={Boolean(
                                                        productSalesPriceError,
                                                    )}
                                                    aria-describedby={
                                                        productSalesPriceError
                                                            ? priceErrorId
                                                            : undefined
                                                    }
                                                />
                                                <span className="text-muted-foreground">
                                                    至
                                                </span>
                                                <Input
                                                    className="w-0 min-w-0 flex-1"
                                                    value={productSalesPriceMaxDraft}
                                                    onChange={(event) => {
                                                        setProductSalesPriceMaxDraft(
                                                            event.target.value,
                                                        )
                                                        setProductSalesPriceError(null)
                                                    }}
                                                    inputMode="decimal"
                                                    autoComplete="off"
                                                    placeholder="最高价"
                                                    aria-label="最高销售价"
                                                    aria-invalid={Boolean(
                                                        productSalesPriceError,
                                                    )}
                                                    aria-describedby={
                                                        productSalesPriceError
                                                            ? priceErrorId
                                                            : undefined
                                                    }
                                                />
                                            </div>
                                            {productSalesPriceError ? (
                                                <span
                                                    id={priceErrorId}
                                                    className="text-xs text-destructive"
                                                    role="alert"
                                                >
                                                    {productSalesPriceError}
                                                </span>
                                            ) : null}
                                        </div>
                                    </div>
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                onClick={resetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button type="submit">
                                                <SearchIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                应用全部筛选
                                            </Button>
                                        </div>
                                    </div>
                                </div>
                            ) : null}
                        </div>
                    ) : undefined
                }
            />
        </form>
    )
}
