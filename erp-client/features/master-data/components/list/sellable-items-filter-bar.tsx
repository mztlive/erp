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
import type { SellableAppliedChip } from "./sellable-list-toolbar"
import type { useSellableListFilters } from "@/features/master-data/hooks/use-sellable-list-filters"
import type { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"
import { PRODUCT_KIND_FILTER_OPTIONS } from "@/features/master-data/lib/list-filters"
import type { ProductKind } from "@/features/master-data/types"

const prefix = "sellable-items-filter"
const panelId = `${prefix}-more-panel`
const priceErrorId = `${prefix}-price-error`

export function SellableItemsFilterBar({
    searchInputRef,
    filters: f,
    appliedChips,
    filterOptions,
    resultCount,
    loading,
    failed,
    idleHint = "导出与当前查询结果一致",
    statusActions,
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    filters: ReturnType<typeof useSellableListFilters>
    appliedChips: readonly SellableAppliedChip[]
    filterOptions: Pick<
        ReturnType<typeof useProductFilterOptionsQuery>,
        "data" | "isPending"
    >
    resultCount?: number
    loading: boolean
    failed: boolean
    idleHint?: string
    statusActions?: React.ReactNode
}) {
    const priceInputRef = React.useRef<HTMLInputElement>(null)
    const moreCount = appliedChips.filter(({ key }) =>
        [
            "productBrandId",
            "productSupplierId",
            "supplyRegion",
            "salesPrice",
        ].includes(key),
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
            formAriaLabel="公司商品池查询"
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
                        categories={filterOptions.data?.categories ?? []}
                        value={f.productCategoryIdDraft ?? undefined}
                        onValueChange={(value) =>
                            f.setProductCategoryIdDraft(value ?? null)
                        }
                        loading={filterOptions.isPending}
                        placeholder="全部"
                        emptyLabel={
                            filterOptions.data?.unavailable?.includes(
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
                                    aria-label="品牌"
                                    value={f.productBrandIdDraft}
                                    onValueChange={f.setProductBrandIdDraft}
                                    options={filterOptions.data?.brands ?? []}
                                    loading={filterOptions.isPending}
                                    placeholder="全部品牌"
                                    emptyLabel={
                                        filterOptions.data?.unavailable?.includes(
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
                                    className="w-full"
                                    aria-label="供应商"
                                    value={f.productSupplierIdDraft}
                                    onValueChange={f.setProductSupplierIdDraft}
                                    options={
                                        filterOptions.data?.suppliers ?? []
                                    }
                                    loading={filterOptions.isPending}
                                    placeholder="全部供应商"
                                    emptyLabel={
                                        filterOptions.data?.unavailable?.includes(
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
                                htmlFor={`${prefix}-region`}
                                label="可供区域"
                            >
                                <Input
                                    id={`${prefix}-region`}
                                    value={f.supplyRegionDraft}
                                    onChange={(event) =>
                                        f.setSupplyRegionDraft(
                                            event.target.value,
                                        )
                                    }
                                    placeholder="如：全国、北京"
                                    autoComplete="off"
                                />
                            </ListWorkspaceFilterField>
                        </div>
                    </fieldset>
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            销售价格
                        </legend>
                        <ListWorkspaceFilterField label="含税售价 · 元">
                            <div className="flex min-w-0 items-center gap-2">
                                <Input
                                    id={`${prefix}-price-min`}
                                    ref={priceInputRef}
                                    className="h-control w-0 min-w-0 flex-1"
                                    aria-label="最低销售价"
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
                                    aria-label="最高销售价"
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
            resultStatus={listWorkspaceFilterStatusText({
                loading,
                failed,
                resultCount,
                noun: "件商品",
                loadingLabel: "正在加载商品…",
            })}
            chips={appliedChips}
            onClearChip={(key) =>
                f.removeFilter(key as SellableAppliedChip["key"])
            }
            onClearAll={f.clearAllFilters}
            hasPendingChanges={f.hasPendingChanges}
            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
            idleHint={idleHint}
            statusActions={statusActions}
        />
    )
}
