"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
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
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
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
type PatchUrl = (patch: Record<string, string | null>) => void

type MasterDataListToolbarProps = {
    isProductResource: boolean
    isSupplierResource: boolean
    resource: MasterDataResource
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    rowCount: number
    hasActiveFilters: boolean
    clearAllFilters: () => void
    patchUrl: PatchUrl
    resetPagination: () => void
    q: string
    lifecycleStatus: "enabled" | "disabled" | "all"
    revisionTiming: "current" | "future" | "all"
    changeLifecycle: (next: "enabled" | "disabled" | "all") => void
    changeRevisionTiming: (next: "current" | "future" | "all") => void
    productFilterPanelOpen: boolean
    setProductFilterPanelOpen: SetState<boolean>
    hasStructuredProductFilters: boolean
    applyProductFilters: () => void
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
    resource,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    rowCount,
    hasActiveFilters,
    clearAllFilters,
    patchUrl,
    resetPagination,
    q,
    lifecycleStatus,
    revisionTiming,
    changeLifecycle,
    changeRevisionTiming,
    productFilterPanelOpen,
    setProductFilterPanelOpen,
    hasStructuredProductFilters,
    applyProductFilters,
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
                                    <OptionCombobox
                                        className="w-full"
                                        value={productCategoryIdDraft}
                                        aria-label="商品分类"
                                        onValueChange={
                                            setProductCategoryIdDraft
                                        }
                                        options={
                                            productFilterOptionsQuery.data
                                                ?.categories ?? []
                                        }
                                        loading={
                                            productFilterOptionsQuery.isPending
                                        }
                                        placeholder="全部分类"
                                        searchPlaceholder="搜索分类名称或代码"
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
        <ListToolbar
            search={
                <form
                    onSubmit={(e) => {
                        e.preventDefault()
                        if (searchDraft.trim() === q) return
                        patchUrl({
                            q: searchDraft.trim() || null,
                            page: null,
                        })
                        resetPagination()
                    }}
                >
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
                        <InputGroupAddon align="inline-end">
                            <InputGroupButton
                                type="submit"
                                aria-label="执行搜索"
                            >
                                搜索
                            </InputGroupButton>
                        </InputGroupAddon>
                    </InputGroup>
                </form>
            }
            filters={
                <>
                    <ToggleGroup
                        value={[lifecycleStatus]}
                        onValueChange={(values) => {
                            const next =
                                (values[0] as
                                    | typeof lifecycleStatus
                                    | undefined) ?? "all"
                            changeLifecycle(next)
                        }}
                        variant="outline"
                        size="sm"
                        spacing={0}
                        aria-label={masterDataCopy.filterLifecycleAria}
                    >
                        <ToggleGroupItem value="all">全部</ToggleGroupItem>
                        <ToggleGroupItem value="enabled">
                            {masterDataCopy.lifecycleEnabled}
                        </ToggleGroupItem>
                        <ToggleGroupItem value="disabled">
                            {masterDataCopy.lifecycleDisabled}
                        </ToggleGroupItem>
                    </ToggleGroup>
                    <OptionCombobox
                        className="w-[10.5rem]"
                        value={revisionTiming}
                        aria-label={masterDataCopy.filterVersionAria}
                        onValueChange={(v) => {
                            changeRevisionTiming(
                                (v ?? "all") as typeof revisionTiming,
                            )
                        }}
                        options={[
                            {
                                value: "all",
                                label: masterDataCopy.versionAll,
                            },
                            {
                                value: "current",
                                label: masterDataCopy.versionCurrent,
                            },
                            {
                                value: "future",
                                label: masterDataCopy.versionFuture,
                            },
                        ]}
                        size="sm"
                        allowClear={false}
                        placeholder={masterDataCopy.versionAll}
                    />
                </>
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
    )
}

export { MasterDataListToolbar }
