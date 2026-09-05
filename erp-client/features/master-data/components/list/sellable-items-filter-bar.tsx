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
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { cn } from "@/lib/utils"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { ListSearchField } from "./list-search-field"
import type { SellableAppliedChip } from "./sellable-list-toolbar"
import type { useSellableListFilters } from "@/features/master-data/hooks/use-sellable-list-filters"
import type { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"
import { PRODUCT_KIND_RADIO_FILTER_OPTIONS } from "@/features/master-data/lib/list-filters"

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
        <form
            aria-label="公司商品池查询"
            onSubmit={(event) => {
                event.preventDefault()
                f.applySellableFilters()
            }}
        >
            <ListToolbar
                className="gap-4 [&_[data-slot=list-toolbar-filters]]:self-start max-sm:[&_[data-slot=list-toolbar-primary]]:sticky max-sm:[&_[data-slot=list-toolbar-primary]]:top-0 max-sm:[&_[data-slot=list-toolbar-primary]]:z-10 max-sm:[&_[data-slot=list-toolbar-primary]]:bg-card max-sm:[&_[data-slot=list-toolbar-primary]]:pb-2 [&_[data-slot=list-toolbar-query-tools]]:sm:flex-wrap [&_[data-slot=list-toolbar-search]]:lg:w-96"
                search={
                    <ListSearchField
                        id={`${prefix}-search`}
                        searchInputRef={searchInputRef}
                        value={f.searchDraft}
                        onChange={f.setSearchDraft}
                        placeholder="搜索商品名称、SKU 或编号"
                    />
                }
                filters={
                    <>
                        <Button id={`${prefix}-query`} type="submit">
                            <SearchIcon aria-hidden="true" />
                            查询
                        </Button>
                        <Button
                            id={`${prefix}-more`}
                            type="button"
                            variant="ghost"
                            aria-expanded={f.sellableFilterPanelOpen}
                            aria-controls={panelId}
                            onClick={() =>
                                f.setSellableFilterPanelOpen((open) => !open)
                            }
                        >
                            <FilterIcon aria-hidden="true" />
                            更多筛选
                            {moreCount > 0 ? (
                                <span
                                    className="rounded bg-muted px-1.5 text-xs tabular-nums"
                                    aria-label={`${moreCount} 项已生效`}
                                >
                                    {moreCount}
                                </span>
                            ) : null}
                            <ChevronDownIcon
                                aria-hidden="true"
                                className={cn(
                                    "transition-transform",
                                    f.sellableFilterPanelOpen && "rotate-180",
                                )}
                            />
                        </Button>
                    </>
                }
                secondary={
                    <div className="w-full min-w-0 space-y-4">
                        <div className="flex min-w-0 flex-col gap-3 lg:flex-row lg:items-center lg:gap-6">
                            <FixedOptionRadioFilter
                                idPrefix={`${prefix}-kind`}
                                label="商品类型"
                                variant="quiet"
                                value={f.productKindDraft}
                                onValueChange={f.setProductKindDraft}
                                options={PRODUCT_KIND_RADIO_FILTER_OPTIONS}
                            />
                            <div className="flex min-w-0 items-center gap-3 lg:border-l lg:pl-6">
                                <label
                                    htmlFor={`${prefix}-category`}
                                    className="shrink-0 text-sm text-muted-foreground"
                                >
                                    分类
                                </label>
                                <CategoryCombobox
                                    id={`${prefix}-category`}
                                    className="w-full sm:w-60"
                                    aria-label="商品分类"
                                    categories={
                                        filterOptions.data?.categories ?? []
                                    }
                                    value={
                                        f.productCategoryIdDraft ?? undefined
                                    }
                                    onValueChange={(value) =>
                                        f.setProductCategoryIdDraft(
                                            value ?? null,
                                        )
                                    }
                                    loading={filterOptions.isPending}
                                    placeholder="全部分类"
                                />
                            </div>
                        </div>
                        {f.sellableFilterPanelOpen ? (
                            <section
                                id={panelId}
                                aria-label="更多筛选条件"
                                className="rounded-xl border border-border/70 bg-muted/25 p-4"
                            >
                                <div className="grid min-w-0 gap-5 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
                                    <fieldset className="min-w-0">
                                        <legend className="mb-3 text-xs font-medium">
                                            品牌与供货
                                        </legend>
                                        <div className="grid min-w-0 gap-3 sm:grid-cols-3">
                                            <div className="min-w-0 space-y-1.5">
                                                <label
                                                    htmlFor={`${prefix}-brand`}
                                                    className="text-xs text-muted-foreground"
                                                >
                                                    品牌
                                                </label>
                                                <OptionCombobox
                                                    id={`${prefix}-brand`}
                                                    className="w-full"
                                                    aria-label="品牌"
                                                    value={
                                                        f.productBrandIdDraft
                                                    }
                                                    onValueChange={
                                                        f.setProductBrandIdDraft
                                                    }
                                                    options={
                                                        filterOptions.data
                                                            ?.brands ?? []
                                                    }
                                                    loading={
                                                        filterOptions.isPending
                                                    }
                                                    placeholder="全部品牌"
                                                    searchPlaceholder="搜索品牌名称或代码"
                                                />
                                            </div>
                                            <div className="min-w-0 space-y-1.5">
                                                <label
                                                    htmlFor={`${prefix}-supplier`}
                                                    className="text-xs text-muted-foreground"
                                                >
                                                    供应商
                                                </label>
                                                <OptionCombobox
                                                    id={`${prefix}-supplier`}
                                                    className="w-full"
                                                    aria-label="供应商"
                                                    value={
                                                        f.productSupplierIdDraft
                                                    }
                                                    onValueChange={
                                                        f.setProductSupplierIdDraft
                                                    }
                                                    options={
                                                        filterOptions.data
                                                            ?.suppliers ?? []
                                                    }
                                                    loading={
                                                        filterOptions.isPending
                                                    }
                                                    placeholder="全部供应商"
                                                    searchPlaceholder="搜索供应商名称或代码"
                                                />
                                            </div>
                                            <div className="min-w-0 space-y-1.5">
                                                <label
                                                    htmlFor={`${prefix}-region`}
                                                    className="text-xs text-muted-foreground"
                                                >
                                                    可供区域
                                                </label>
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
                                            </div>
                                        </div>
                                    </fieldset>
                                    <fieldset className="min-w-0 lg:border-l lg:pl-5">
                                        <legend className="mb-3 text-xs font-medium">
                                            销售价格
                                        </legend>
                                        <div className="space-y-1.5">
                                            <div className="text-xs text-muted-foreground">
                                                含税售价 · 元
                                            </div>
                                            <div className="flex min-w-0 items-center gap-2">
                                                <Input
                                                    id={`${prefix}-price-min`}
                                                    ref={priceInputRef}
                                                    className="w-0 min-w-0 flex-1"
                                                    aria-label="最低销售价"
                                                    placeholder="最低价"
                                                    inputMode="decimal"
                                                    autoComplete="off"
                                                    value={
                                                        f.productSalesPriceMinDraft
                                                    }
                                                    onChange={(event) => {
                                                        f.setProductSalesPriceMinDraft(
                                                            event.target.value,
                                                        )
                                                        f.setProductSalesPriceError(
                                                            null,
                                                        )
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
                                                    className="w-0 min-w-0 flex-1"
                                                    aria-label="最高销售价"
                                                    placeholder="最高价"
                                                    inputMode="decimal"
                                                    autoComplete="off"
                                                    value={
                                                        f.productSalesPriceMaxDraft
                                                    }
                                                    onChange={(event) => {
                                                        f.setProductSalesPriceMaxDraft(
                                                            event.target.value,
                                                        )
                                                        f.setProductSalesPriceError(
                                                            null,
                                                        )
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
                                        </div>
                                    </fieldset>
                                </div>
                                <div className="mt-4 flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-3">
                                    <span className="text-xs text-muted-foreground">
                                        可组合多个条件，点击「查询」后统一生效。
                                    </span>
                                    <Button
                                        id={`${prefix}-reset-more`}
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        onClick={f.resetMoreFilters}
                                    >
                                        重置更多条件
                                    </Button>
                                </div>
                            </section>
                        ) : null}
                        <div className="flex flex-wrap items-center gap-x-3 gap-y-2 border-t border-border/60 pt-3 text-xs">
                            <span
                                role="status"
                                className="shrink-0 text-muted-foreground"
                            >
                                {loading
                                    ? "查询中…"
                                    : failed
                                      ? "查询未完成"
                                      : resultCount === undefined
                                        ? "正在加载商品…"
                                        : `共 ${resultCount} 件商品`}
                            </span>
                            {appliedChips.length ? (
                                <>
                                    <span className="text-muted-foreground">
                                        已生效
                                    </span>
                                    {appliedChips.map((chip) => (
                                        <FilterChip
                                            key={chip.key}
                                            id={`${prefix}-chip-${toAutomationIdSegment(chip.key)}`}
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                f.removeFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        id={`${prefix}-clear`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={f.clearAllFilters}
                                    >
                                        清除全部
                                    </Button>
                                </>
                            ) : null}
                            <span
                                role="status"
                                className={cn(
                                    "sm:ml-auto",
                                    f.hasPendingChanges
                                        ? "font-medium text-warning-soft-foreground"
                                        : "text-muted-foreground",
                                )}
                            >
                                {f.hasPendingChanges
                                    ? "条件已修改，待查询 · 导出仍按已生效条件"
                                    : "导出与当前查询结果一致"}
                            </span>
                        </div>
                    </div>
                }
            />
        </form>
    )
}
