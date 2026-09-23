"use client"

import * as React from "react"

import { CategoryCombobox, OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Input } from "@/components/ui/input"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"
import type { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"
import type { ProductAppliedChip } from "@/features/master-data/hooks/use-product-list-state"
import type { useProductListFilters } from "@/features/master-data/hooks/use-product-list-filters"
import { masterDataSearchPlaceholder } from "@/features/master-data/lib/copy"
import {
    PRODUCT_COVERAGE_FILTER_OPTIONS,
    PRODUCT_KIND_FILTER_OPTIONS,
    PRODUCT_LISTING_FILTER_OPTIONS,
    REVISION_TIMING_FILTER_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import type {
    ProductKind,
    ProductListingFilter,
    ProductSkuCoverageFilter,
} from "@/features/master-data/types"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"

const MORE_CHIP_KEYS = [
    "revisionTiming",
    "productListingStatus",
    "productSupplyCoverage",
    "productBrandId",
    "productSupplierId",
    "salesPrice",
    "ownerUserIds",
    "procurementOwnerUserIds",
    "orgUnitIds",
] as const

export function ProductListToolbar({
    idPrefix,
    searchInputRef,
    filters: f,
    appliedChips,
    productFilterOptionsQuery,
    ownerOptions,
    procurementOwnerOptions,
    resultCount,
    loading,
    failed,
}: {
    idPrefix?: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    filters: ReturnType<typeof useProductListFilters>
    appliedChips: readonly ProductAppliedChip[]
    productFilterOptionsQuery: ReturnType<typeof useProductFilterOptionsQuery>
    ownerOptions: ReadonlyArray<{ value: string; label: string }>
    procurementOwnerOptions: ReadonlyArray<{ value: string; label: string }>
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
            morePresentation="popover"
            moreSize="wide"
            className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
            idPrefix={prefix}
            formAriaLabel="商品列表查询"
            onSubmit={f.applyProductFilters}
            queryButtonId={`${prefix}-query`}
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
            onToggleMore={() =>
                f.productFilterPanelOpen
                    ? f.cancelMoreFilters()
                    : f.setProductFilterPanelOpen(true)
            }
            morePanelId={panelId}
            morePanelAriaLabel="商品列表更多筛选条件"
            moreButtonId={`${prefix}-filter-trigger`}
            resetMoreButtonId={`${prefix}-reset`}
            clearButtonId={`${prefix}-clear-filters`}
            onResetMore={f.resetMoreFilters}
            primaryFilters={
                <>
                    <OptionCombobox
                        id={`${prefix}-kind`}
                        className="w-44 max-w-full min-w-0"
                        filterLabel="类型"
                        aria-label="类型"
                        value={
                            f.productKindDraft === "all"
                                ? null
                                : f.productKindDraft
                        }
                        options={PRODUCT_KIND_FILTER_OPTIONS}
                        placeholder="全部"
                        onValueChange={(value) =>
                            f.setProductKindDraft(
                                PRODUCT_KIND_FILTER_OPTIONS.some(
                                    (option) => option.value === value,
                                )
                                    ? (value as ProductKind)
                                    : "all",
                            )
                        }
                    />
                    <CategoryCombobox
                        id={`${prefix}-category`}
                        className="w-56 max-w-full min-w-0"
                        filterLabel="分类"
                        categories={
                            productFilterOptionsQuery.data?.categories ?? []
                        }
                        value={f.productCategoryIdDraft ?? undefined}
                        onValueChange={(id) =>
                            f.setProductCategoryIdDraft(id ?? null)
                        }
                        loading={productFilterOptionsQuery.isPending}
                        placeholder="全部"
                        emptyLabel={
                            productFilterOptionsQuery.data?.unavailable?.includes(
                                "categories",
                            )
                                ? "当前账号无分类查询权限"
                                : "没有符合条件的分类"
                        }
                    />
                </>
            }
            morePanel={
                <div className="grid min-w-0 gap-5">
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            人员
                        </legend>
                        <div className="grid min-w-0 gap-3 sm:grid-cols-2">
                            <ResponsibleUserFilter
                                id={`${prefix}-owner`}
                                label="维护人"
                                value={f.ownerUserIdsDraft}
                                onChange={f.setOwnerUserIdsDraft}
                                options={ownerOptions}
                            />
                            <ResponsibleUserFilter
                                id={`${prefix}-procurement-owner`}
                                label="采购负责人"
                                value={f.procurementOwnerUserIdsDraft}
                                onChange={f.setProcurementOwnerUserIdsDraft}
                                options={procurementOwnerOptions}
                            />
                        </div>
                    </fieldset>
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            范围
                        </legend>
                        <div className="grid min-w-0 gap-3">
                            <OrganizationUnitFilter
                                id={`${prefix}-org`}
                                label="业务组织"
                                value={f.orgUnitIdsDraft}
                                onChange={f.setOrgUnitIdsDraft}
                                includeDescendants={f.includeDescendantsDraft}
                                onDescendantsChange={
                                    f.setIncludeDescendantsDraft
                                }
                            />
                            <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-[minmax(0,0.72fr)_minmax(0,0.9fr)_minmax(0,1.25fr)]">
                                <ListWorkspaceFilterField
                                    htmlFor={`${prefix}-revision`}
                                    label="版本"
                                >
                                    <OptionCombobox
                                        id={`${prefix}-revision`}
                                        className="w-full min-w-0"
                                        aria-label="版本状态"
                                        value={
                                            f.revisionTimingDraft === "all"
                                                ? null
                                                : f.revisionTimingDraft
                                        }
                                        options={REVISION_TIMING_FILTER_OPTIONS}
                                        placeholder="全部"
                                        onValueChange={(value) =>
                                            f.setRevisionTimingDraft(
                                                value === "current" ||
                                                    value === "future"
                                                    ? value
                                                    : "all",
                                            )
                                        }
                                    />
                                </ListWorkspaceFilterField>
                                <ListWorkspaceFilterField
                                    htmlFor={`${prefix}-listing`}
                                    label="上架"
                                >
                                    <OptionCombobox
                                        id={`${prefix}-listing`}
                                        className="w-full min-w-0"
                                        aria-label="上架"
                                        value={
                                            f.productListingStatusDraft ===
                                            "all"
                                                ? null
                                                : f.productListingStatusDraft
                                        }
                                        options={PRODUCT_LISTING_FILTER_OPTIONS}
                                        placeholder="全部"
                                        onValueChange={(value) =>
                                            f.setProductListingStatusDraft(
                                                PRODUCT_LISTING_FILTER_OPTIONS.some(
                                                    (option) =>
                                                        option.value === value,
                                                )
                                                    ? (value as ProductListingFilter)
                                                    : "all",
                                            )
                                        }
                                    />
                                </ListWorkspaceFilterField>
                                <ListWorkspaceFilterField
                                    htmlFor={`${prefix}-coverage`}
                                    label="供给覆盖"
                                >
                                    <OptionCombobox
                                        id={`${prefix}-coverage`}
                                        className="w-full min-w-0"
                                        aria-label="供给覆盖"
                                        value={
                                            f.productSupplyCoverageDraft ===
                                            "all"
                                                ? null
                                                : f.productSupplyCoverageDraft
                                        }
                                        options={
                                            PRODUCT_COVERAGE_FILTER_OPTIONS
                                        }
                                        placeholder="全部"
                                        onValueChange={(value) =>
                                            f.setProductSupplyCoverageDraft(
                                                PRODUCT_COVERAGE_FILTER_OPTIONS.some(
                                                    (option) =>
                                                        option.value === value,
                                                )
                                                    ? (value as ProductSkuCoverageFilter)
                                                    : "all",
                                            )
                                        }
                                    />
                                </ListWorkspaceFilterField>
                            </div>
                        </div>
                    </fieldset>
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            商品
                        </legend>
                        <div className="grid min-w-0 gap-3 sm:grid-cols-2">
                            <ListWorkspaceFilterField
                                htmlFor={`${prefix}-brand`}
                                label="品牌"
                            >
                                <OptionCombobox
                                    id={`${prefix}-brand`}
                                    className="w-full min-w-0"
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
                                    emptyLabel={
                                        productFilterOptionsQuery.data?.unavailable?.includes(
                                            "brands",
                                        )
                                            ? "当前账号无品牌查询权限"
                                            : "没有符合条件的品牌"
                                    }
                                    searchPlaceholder="搜索品牌名称或代码"
                                />
                            </ListWorkspaceFilterField>
                            <ListWorkspaceFilterField
                                htmlFor={`${prefix}-supplier`}
                                label="供应商"
                            >
                                <OptionCombobox
                                    id={`${prefix}-supplier`}
                                    className="w-full min-w-0"
                                    value={f.productSupplierIdDraft}
                                    aria-label="供应商"
                                    onValueChange={f.setProductSupplierIdDraft}
                                    options={
                                        productFilterOptionsQuery.data
                                            ?.suppliers ?? []
                                    }
                                    loading={
                                        productFilterOptionsQuery.isPending
                                    }
                                    placeholder="全部供应商"
                                    emptyLabel={
                                        productFilterOptionsQuery.data?.unavailable?.includes(
                                            "suppliers",
                                        )
                                            ? "当前账号无供应商查询权限"
                                            : "没有符合条件的供应商"
                                    }
                                    searchPlaceholder="搜索供应商名称或代码"
                                />
                            </ListWorkspaceFilterField>
                            <ListWorkspaceFilterField
                                className="sm:col-span-2"
                                label="含税售价 · 元"
                            >
                                <div className="flex min-w-0 items-center gap-2">
                                    <Input
                                        id={`${prefix}-price-min`}
                                        ref={priceInputRef}
                                        className="h-control w-0 min-w-0 flex-1"
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
                                        className="h-control w-0 min-w-0 flex-1"
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
                        </div>
                    </fieldset>
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
