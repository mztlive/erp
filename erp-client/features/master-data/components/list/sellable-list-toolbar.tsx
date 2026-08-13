"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    CategoryCombobox,
    FixedOptionRadioFilter,
    ListToolbar,
    OptionCombobox,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ListSearchField } from "@/features/master-data/components/list/list-search-field"
import { ListToolbarCount } from "@/features/master-data/components/list/list-toolbar-actions"
import {
    masterDataCopy,
    masterDataSearchPlaceholder,
} from "@/features/master-data/lib/copy"
import { PRODUCT_KIND_RADIO_FILTER_OPTIONS } from "@/features/master-data/lib/list-filters"
import type { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"
import type { ProductKind } from "@/features/master-data/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export function SellableListToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    rowCount,
    hasActiveFilters,
    clearAllFilters,
    sellableFilterPanelOpen,
    setSellableFilterPanelOpen,
    hasStructuredSellableFilters,
    applySellableFilters,
    supplyRegionDraft,
    setSupplyRegionDraft,
    productKindDraft,
    setProductKindDraft,
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
    rowCount: number
    hasActiveFilters: boolean
    clearAllFilters: () => void
    sellableFilterPanelOpen: boolean
    setSellableFilterPanelOpen: SetState<boolean>
    hasStructuredSellableFilters: boolean
    applySellableFilters: () => void
    supplyRegionDraft: string
    setSupplyRegionDraft: SetState<string>
    productKindDraft: ProductKind | "all"
    setProductKindDraft: SetState<ProductKind | "all">
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
    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applySellableFilters()
            }}
        >
            <ListToolbar
                search={
                    <ListSearchField
                        searchInputRef={searchInputRef}
                        value={searchDraft}
                        onChange={setSearchDraft}
                        placeholder={masterDataSearchPlaceholder(
                            "sellable-items",
                        )}
                    />
                }
                filters={
                    <>
                        {!sellableFilterPanelOpen ? (
                            <Button type="submit" size="sm">
                                <SearchIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                搜索
                            </Button>
                        ) : null}
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            aria-expanded={sellableFilterPanelOpen}
                            onClick={() =>
                                setSellableFilterPanelOpen((open) => !open)
                            }
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            高级筛选
                            {hasStructuredSellableFilters ? (
                                <Badge variant="info">已启用</Badge>
                            ) : null}
                            <ChevronDownIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                                className={
                                    sellableFilterPanelOpen
                                        ? "rotate-180 transition-transform"
                                        : "transition-transform"
                                }
                            />
                        </Button>
                    </>
                }
                secondary={
                    sellableFilterPanelOpen ? (
                        <div
                            className="flex w-full flex-col gap-3 rounded-lg border border-border/60 bg-muted/30 px-3 py-3"
                            aria-label="公司商品池筛选条件"
                        >
                            <FixedOptionRadioFilter
                                label="类型"
                                value={productKindDraft}
                                onValueChange={setProductKindDraft}
                                options={PRODUCT_KIND_RADIO_FILTER_OPTIONS}
                                aria-label={masterDataCopy.filterProductKindAria}
                            />
                            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                <label className="flex min-w-0 flex-col gap-1.5 text-sm">
                                    <span className="text-muted-foreground">
                                        分类
                                    </span>
                                    <CategoryCombobox
                                        className="w-full"
                                        categories={
                                            productFilterOptionsQuery.data
                                                ?.categories ?? []
                                        }
                                        value={
                                            productCategoryIdDraft ?? undefined
                                        }
                                        onValueChange={(id) =>
                                            setProductCategoryIdDraft(id ?? null)
                                        }
                                        loading={
                                            productFilterOptionsQuery.isPending
                                        }
                                        placeholder="全部分类"
                                    />
                                </label>
                                <label className="flex min-w-0 flex-col gap-1.5 text-sm">
                                    <span className="text-muted-foreground">
                                        品牌
                                    </span>
                                    <OptionCombobox
                                        className="w-full"
                                        value={productBrandIdDraft}
                                        aria-label="商品品牌"
                                        onValueChange={setProductBrandIdDraft}
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
                                </label>
                                <label className="flex min-w-0 flex-col gap-1.5 text-sm">
                                    <span className="text-muted-foreground">
                                        供应商
                                    </span>
                                    <OptionCombobox
                                        className="w-full"
                                        value={productSupplierIdDraft}
                                        aria-label="供应商"
                                        onValueChange={setProductSupplierIdDraft}
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
                                </label>
                                <label className="flex min-w-0 flex-col gap-1.5 text-sm">
                                    <span className="text-muted-foreground">
                                        可供区域
                                    </span>
                                    <Input
                                        className="w-full"
                                        value={supplyRegionDraft}
                                        onChange={(event) =>
                                            setSupplyRegionDraft(
                                                event.target.value,
                                            )
                                        }
                                        autoComplete="off"
                                        placeholder="如：全国"
                                        aria-label="可供区域"
                                    />
                                </label>
                                <div className="flex min-w-0 flex-col gap-1.5 text-sm sm:col-span-2 lg:col-span-2">
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
                                            aria-describedby="sellable-sales-price-error"
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
                                            aria-describedby="sellable-sales-price-error"
                                        />
                                    </div>
                                    {productSalesPriceError ? (
                                        <span
                                            id="sellable-sales-price-error"
                                            className="text-xs text-destructive"
                                            role="alert"
                                        >
                                            {productSalesPriceError}
                                        </span>
                                    ) : null}
                                </div>
                            </div>
                            <div className="flex justify-end">
                                <Button type="submit" size="sm">
                                    <SearchIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    搜索
                                </Button>
                            </div>
                        </div>
                    ) : undefined
                }
                actions={
                    <ListToolbarCount
                        rowCount={rowCount}
                        hasActiveFilters={hasActiveFilters}
                        onClear={clearAllFilters}
                    />
                }
            />
        </form>
    )
}
