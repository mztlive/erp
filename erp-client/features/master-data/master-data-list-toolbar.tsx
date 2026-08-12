"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    CategoryCombobox,
    FixedOptionCheckboxFilter,
    FixedOptionRadioFilter,
    ListToolbar,
    OptionCombobox,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import {
    masterDataCopy,
    masterDataSearchPlaceholder,
} from "@/features/master-data/copy"
import { resourceLabel } from "@/features/master-data/data"
import {
    LIFECYCLE_RADIO_FILTER_OPTIONS,
    PRODUCT_COVERAGE_RADIO_FILTER_OPTIONS,
    PRODUCT_KIND_RADIO_FILTER_OPTIONS,
    PRODUCT_LISTING_RADIO_FILTER_OPTIONS,
    REVISION_TIMING_RADIO_FILTER_OPTIONS,
    SUPPLIER_CAPABILITY_OPTIONS,
    SUPPLIER_QUALIFICATION_HEALTH_OPTIONS,
    SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
} from "@/features/master-data/master-data-list-presentation"
import type { useProductFilterOptionsQuery } from "@/features/master-data/queries"
import type {
    MasterDataResource,
    ProductKind,
    ProductListingFilter,
    ProductSkuCoverageFilter,
    SupplierQualificationHealth,
} from "@/features/master-data/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

type MasterDataListToolbarProps = {
    isProductResource: boolean
    isSupplierResource: boolean
    isSellableResource: boolean
    resource: MasterDataResource
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    commitSearch: () => void
    rowCount: number
    hasActiveFilters: boolean
    clearAllFilters: () => void
    /** 通用列表（品牌/计量单位/卡券类目/仓库）显式提交筛选面板（§3.6）。 */
    filterPanelOpen: boolean
    setFilterPanelOpen: SetState<boolean>
    hasStructuredListFilters: boolean
    applyListFilters: () => void
    productFilterPanelOpen: boolean
    setProductFilterPanelOpen: SetState<boolean>
    hasStructuredProductFilters: boolean
    applyProductFilters: () => void
    /** 公司商品池：§3.6 显式提交筛选面板。 */
    sellableFilterPanelOpen: boolean
    setSellableFilterPanelOpen: SetState<boolean>
    hasStructuredSellableFilters: boolean
    applySellableFilters: () => void
    supplyRegionDraft: string
    setSupplyRegionDraft: SetState<string>
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
    supplierFilterPanelOpen: boolean
    setSupplierFilterPanelOpen: SetState<boolean>
    hasStructuredSupplierFilters: boolean
    applySupplierFilters: () => void
    supplierQualificationHealthDraft: SupplierQualificationHealth | "all"
    setSupplierQualificationHealthDraft: SetState<
        SupplierQualificationHealth | "all"
    >
    supplierCapabilityCodesDraft: string[]
    setSupplierCapabilityCodesDraft: SetState<string[]>
    supplierQualificationTypesDraft: string[]
    setSupplierQualificationTypesDraft: SetState<string[]>
}

function MasterDataListToolbar({
    isProductResource,
    isSupplierResource,
    isSellableResource,
    resource,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    commitSearch,
    rowCount,
    hasActiveFilters,
    clearAllFilters,
    filterPanelOpen,
    setFilterPanelOpen,
    hasStructuredListFilters,
    applyListFilters,
    productFilterPanelOpen,
    setProductFilterPanelOpen,
    hasStructuredProductFilters,
    applyProductFilters,
    sellableFilterPanelOpen,
    setSellableFilterPanelOpen,
    hasStructuredSellableFilters,
    applySellableFilters,
    supplyRegionDraft,
    setSupplyRegionDraft,
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
    supplierFilterPanelOpen,
    setSupplierFilterPanelOpen,
    hasStructuredSupplierFilters,
    applySupplierFilters,
    supplierQualificationHealthDraft,
    setSupplierQualificationHealthDraft,
    supplierCapabilityCodesDraft,
    setSupplierCapabilityCodesDraft,
    supplierQualificationTypesDraft,
    setSupplierQualificationTypesDraft,
}: MasterDataListToolbarProps) {
    if (isSellableResource) {
        return (
            <form
                onSubmit={(event) => {
                    event.preventDefault()
                    applySellableFilters()
                }}
            >
                <ListToolbar
                    search={
                        <InputGroup>
                            <InputGroupAddon>
                                <SearchIcon aria-hidden="true" />
                            </InputGroupAddon>
                            <InputGroupInput
                                ref={searchInputRef}
                                value={searchDraft}
                                onChange={(event) =>
                                    setSearchDraft(event.target.value)
                                }
                                placeholder={masterDataSearchPlaceholder(
                                    resource,
                                )}
                                aria-label={masterDataCopy.searchAria}
                            />
                        </InputGroup>
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
                                    aria-label={
                                        masterDataCopy.filterProductKindAria
                                    }
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
                                    <div className="flex min-w-0 flex-col gap-1.5 text-sm sm:col-span-2 lg:col-span-2">
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
                            </div>
                        ) : undefined
                    }
                    actions={
                        <>
                            <span
                                className="text-xs text-muted-foreground"
                                aria-live="polite"
                            >
                                共 {rowCount} 条
                            </span>
                            {hasActiveFilters ? (
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="ghost"
                                    onClick={clearAllFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : null}
                        </>
                    }
                />
            </form>
        )
    }

    return isProductResource ? (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applyProductFilters()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(e) => setSearchDraft(e.target.value)}
                            placeholder={masterDataSearchPlaceholder(resource)}
                            aria-label={masterDataCopy.searchAria}
                        />
                    </InputGroup>
                }
                filters={
                    <>
                        {!productFilterPanelOpen ? (
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
                            aria-expanded={productFilterPanelOpen}
                            onClick={() =>
                                setProductFilterPanelOpen((open) => !open)
                            }
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            高级筛选
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
                    </>
                }
                secondary={
                    productFilterPanelOpen ? (
                        <div
                            className="flex w-full flex-col gap-3 rounded-lg border border-border/60 bg-muted/30 px-3 py-3"
                            aria-label="商品筛选条件"
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
                                aria-label={masterDataCopy.filterLifecycleAria}
                            />
                            <FixedOptionRadioFilter
                                label="版本"
                                value={revisionTimingDraft}
                                onValueChange={setRevisionTimingDraft}
                                options={REVISION_TIMING_RADIO_FILTER_OPTIONS}
                                aria-label={masterDataCopy.filterVersionAria}
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
                                            aria-describedby="product-sales-price-error"
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
                                            aria-describedby="product-sales-price-error"
                                        />
                                    </div>
                                    {productSalesPriceError ? (
                                        <span
                                            id="product-sales-price-error"
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
                    <>
                        <span
                            className="text-xs text-muted-foreground"
                            aria-live="polite"
                        >
                            {resourceLabel(resource)} · {rowCount} 条
                        </span>
                        {hasActiveFilters ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={clearAllFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null}
                    </>
                }
            />
        </form>
    ) : isSupplierResource ? (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applySupplierFilters()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(event) =>
                                setSearchDraft(event.target.value)
                            }
                            placeholder={masterDataSearchPlaceholder(resource)}
                            aria-label={masterDataCopy.searchAria}
                        />
                    </InputGroup>
                }
                filters={
                    <>
                        {!supplierFilterPanelOpen ? (
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
                            aria-expanded={supplierFilterPanelOpen}
                            onClick={() =>
                                setSupplierFilterPanelOpen((open) => !open)
                            }
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            高级筛选
                            {hasStructuredSupplierFilters ? (
                                <Badge variant="info">已启用</Badge>
                            ) : null}
                            <ChevronDownIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                                className={
                                    supplierFilterPanelOpen
                                        ? "rotate-180 transition-transform"
                                        : "transition-transform"
                                }
                            />
                        </Button>
                    </>
                }
                secondary={
                    supplierFilterPanelOpen ? (
                        <div
                            className="flex w-full flex-col gap-3 rounded-lg border border-border/60 bg-muted/30 px-3 py-3"
                            aria-label="供应商筛选条件"
                        >
                            <FixedOptionRadioFilter
                                label="启停"
                                value={lifecycleStatusDraft}
                                onValueChange={setLifecycleStatusDraft}
                                options={LIFECYCLE_RADIO_FILTER_OPTIONS}
                                aria-label={masterDataCopy.filterLifecycleAria}
                            />
                            <FixedOptionRadioFilter
                                label="资质状态"
                                value={supplierQualificationHealthDraft}
                                onValueChange={
                                    setSupplierQualificationHealthDraft
                                }
                                options={SUPPLIER_QUALIFICATION_HEALTH_OPTIONS}
                                aria-label="资质状态"
                            />
                            <FixedOptionCheckboxFilter
                                label="供应能力"
                                value={supplierCapabilityCodesDraft}
                                onValueChange={setSupplierCapabilityCodesDraft}
                                options={SUPPLIER_CAPABILITY_OPTIONS}
                                aria-label="供应能力，可多选"
                            />
                            <FixedOptionCheckboxFilter
                                label="资质类型"
                                value={supplierQualificationTypesDraft}
                                onValueChange={
                                    setSupplierQualificationTypesDraft
                                }
                                options={SUPPLIER_QUALIFICATION_TYPE_OPTIONS}
                                aria-label="资质类型，可多选"
                            />

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
                    <>
                        <span
                            className="text-xs text-muted-foreground"
                            aria-live="polite"
                        >
                            {resourceLabel(resource)} · {rowCount} 条
                        </span>
                        {hasActiveFilters ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={clearAllFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null}
                    </>
                }
            />
        </form>
    ) : (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applyListFilters()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(e) => setSearchDraft(e.target.value)}
                            placeholder={masterDataSearchPlaceholder(resource)}
                            aria-label={masterDataCopy.searchAria}
                        />
                    </InputGroup>
                }
                filters={
                    <>
                        {!filterPanelOpen ? (
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
                            aria-expanded={filterPanelOpen}
                            onClick={() => setFilterPanelOpen((open) => !open)}
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            高级筛选
                            {hasStructuredListFilters ? (
                                <Badge variant="info">已启用</Badge>
                            ) : null}
                            <ChevronDownIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                                className={
                                    filterPanelOpen
                                        ? "rotate-180 transition-transform"
                                        : "transition-transform"
                                }
                            />
                        </Button>
                    </>
                }
                secondary={
                    filterPanelOpen ? (
                        <div
                            className="flex w-full flex-col gap-3 rounded-lg border border-border/60 bg-muted/30 px-3 py-3"
                            aria-label="列表筛选条件"
                        >
                            <FixedOptionRadioFilter
                                label="启停"
                                value={lifecycleStatusDraft}
                                onValueChange={setLifecycleStatusDraft}
                                options={LIFECYCLE_RADIO_FILTER_OPTIONS}
                                aria-label={masterDataCopy.filterLifecycleAria}
                            />
                            <FixedOptionRadioFilter
                                label="版本"
                                value={revisionTimingDraft}
                                onValueChange={setRevisionTimingDraft}
                                options={REVISION_TIMING_RADIO_FILTER_OPTIONS}
                                aria-label={masterDataCopy.filterVersionAria}
                            />

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
                    <>
                        <span
                            className="text-xs text-muted-foreground"
                            aria-live="polite"
                        >
                            {resourceLabel(resource)} · {rowCount} 条
                        </span>
                        {hasActiveFilters ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={clearAllFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null}
                    </>
                }
            />
        </form>
    )
}

export { MasterDataListToolbar }
