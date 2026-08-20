"use client"

import * as React from "react"
import { CheckIcon, CirclePlusIcon, SearchIcon, XIcon } from "lucide-react"

import {
    CategoryCombobox,
    ListToolbar,
    OptionCombobox,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import { Separator } from "@/components/ui/separator"
import { ListSearchField } from "@/features/master-data/components/list/list-search-field"
import { masterDataSearchPlaceholder } from "@/features/master-data/lib/copy"
import { PRODUCT_KIND_RADIO_FILTER_OPTIONS } from "@/features/master-data/lib/list-filters"
import type { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"
import type { ProductKind } from "@/features/master-data/types"
import { cn } from "@/lib/utils"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export function SellableListToolbar({
    searchInputRef,
    searchDraft,
    setSearchDraft,
    hasActiveFilters,
    clearAllFilters,
    sellableFilterPanelOpen,
    setSellableFilterPanelOpen,
    hasAdvancedSellableFilters,
    applySellableFilters,
    applyProductKind,
    supplyRegionDraft,
    setSupplyRegionDraft,
    productKindDraft,
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
    sellableFilterPanelOpen: boolean
    setSellableFilterPanelOpen: SetState<boolean>
    hasAdvancedSellableFilters: boolean
    applySellableFilters: () => void
    applyProductKind: (nextKind: ProductKind | "all") => void
    supplyRegionDraft: string
    setSupplyRegionDraft: SetState<string>
    productKindDraft: ProductKind | "all"
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
    const selectedKindLabel = PRODUCT_KIND_RADIO_FILTER_OPTIONS.find(
        (option) => option.value === productKindDraft,
    )?.label

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applySellableFilters()
                setSellableFilterPanelOpen(false)
            }}
        >
            <ListToolbar
                search={
                    <div className="w-full sm:max-w-[16rem]">
                        <ListSearchField
                            searchInputRef={searchInputRef}
                            value={searchDraft}
                            onChange={setSearchDraft}
                            placeholder={masterDataSearchPlaceholder(
                                "sellable-items",
                            )}
                        />
                    </div>
                }
                filters={
                    <>
                        <Popover>
                            <PopoverTrigger
                                render={
                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        className="border-dashed"
                                        aria-label="商品类型"
                                    />
                                }
                            >
                                <CirclePlusIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                类型
                                {productKindDraft !== "all" &&
                                selectedKindLabel ? (
                                    <>
                                        <Separator
                                            orientation="vertical"
                                            className="mx-1 data-vertical:h-4"
                                        />
                                        <Badge
                                            variant="secondary"
                                            className="rounded-sm px-1 font-normal"
                                        >
                                            {selectedKindLabel}
                                        </Badge>
                                    </>
                                ) : null}
                            </PopoverTrigger>
                            <PopoverContent
                                align="start"
                                className="w-44 gap-1 rounded-lg p-1 shadow-md"
                            >
                                {PRODUCT_KIND_RADIO_FILTER_OPTIONS.map(
                                    (option) => (
                                        <Button
                                            key={option.value}
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            className="w-full justify-start font-normal"
                                            onClick={() =>
                                                applyProductKind(option.value)
                                            }
                                        >
                                            <CheckIcon
                                                className={cn(
                                                    "text-foreground",
                                                    productKindDraft ===
                                                        option.value
                                                        ? "opacity-100"
                                                        : "opacity-0",
                                                )}
                                                aria-hidden="true"
                                            />
                                            {option.label}
                                        </Button>
                                    ),
                                )}
                            </PopoverContent>
                        </Popover>
                        <Popover
                            open={sellableFilterPanelOpen}
                            onOpenChange={setSellableFilterPanelOpen}
                        >
                            <PopoverTrigger
                                render={
                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        className="border-dashed"
                                        aria-label="筛选"
                                    />
                                }
                            >
                                <CirclePlusIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                筛选
                                {hasAdvancedSellableFilters ? (
                                    <>
                                        <Separator
                                            orientation="vertical"
                                            className="mx-1 data-vertical:h-4"
                                        />
                                        <Badge
                                            variant="secondary"
                                            className="rounded-sm px-1 font-normal"
                                        >
                                            已启用
                                        </Badge>
                                    </>
                                ) : null}
                            </PopoverTrigger>
                            <PopoverContent
                                align="start"
                                className="w-[min(36rem,calc(100vw-2rem))] gap-3 rounded-lg p-3 shadow-md"
                            >
                                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
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
                                    </label>
                                    <label className="flex min-w-0 flex-col gap-1.5 text-sm">
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
                                            onValueChange={
                                                setProductSupplierIdDraft
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
                                    <div className="flex min-w-0 flex-col gap-1.5 text-sm sm:col-span-2">
                                        <span className="text-muted-foreground">
                                            销售价
                                        </span>
                                        <div className="flex items-center gap-1.5">
                                            <Input
                                                className="w-0 min-w-0 flex-1"
                                                value={
                                                    productSalesPriceMinDraft
                                                }
                                                onChange={(event) => {
                                                    setProductSalesPriceMinDraft(
                                                        event.target.value,
                                                    )
                                                    setProductSalesPriceError(
                                                        null,
                                                    )
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
                                                value={
                                                    productSalesPriceMaxDraft
                                                }
                                                onChange={(event) => {
                                                    setProductSalesPriceMaxDraft(
                                                        event.target.value,
                                                    )
                                                    setProductSalesPriceError(
                                                        null,
                                                    )
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
                            </PopoverContent>
                        </Popover>
                        {hasActiveFilters ? (
                            <Button
                                type="button"
                                variant="ghost"
                                size="sm"
                                onClick={clearAllFilters}
                            >
                                清除
                                <XIcon
                                    data-icon="inline-end"
                                    aria-hidden="true"
                                />
                            </Button>
                        ) : null}
                    </>
                }
            />
        </form>
    )
}
