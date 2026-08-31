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
import type { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"
import type {
    SellableFilterKey,
    SellableSupplyPresetSelection,
} from "@/features/master-data/hooks/use-sellable-list-filters"
import { masterDataSearchPlaceholder } from "@/features/master-data/lib/copy"
import { PRODUCT_KIND_RADIO_FILTER_OPTIONS } from "@/features/master-data/lib/list-filters"
import type { ProductKind } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

export type SellableAppliedChip = Readonly<{
    key: SellableFilterKey
    label: string
}>

type SupplyPresetCounts = Readonly<
    Record<SellableSupplyPresetSelection, number>
>

const SUPPLY_PRESET_OPTIONS: ReadonlyArray<{
    value: SellableSupplyPresetSelection
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "single-supplier", label: "单一供应商" },
    { value: "nationwide", label: "全国可供" },
]

export function SellableListToolbar({
    idPrefix,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    hasActiveFilters,
    clearAllFilters,
    appliedChips,
    removeFilter,
    supplyPreset,
    supplyPresetCounts,
    applySupplyPreset,
    sellableFilterPanelOpen,
    setSellableFilterPanelOpen,
    hasStructuredSellableFilters,
    applySellableFilters,
    resetMoreFilters,
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
    showSupplyPresetCounts = true,
    hiddenProductKinds,
    applyHint = "将同时应用上方关键词和以下筛选条件；结果也用于导出。",
}: {
    idPrefix?: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    hasActiveFilters: boolean
    clearAllFilters: () => void
    appliedChips: readonly SellableAppliedChip[]
    removeFilter: (key: SellableFilterKey) => void
    supplyPreset: SellableSupplyPresetSelection
    supplyPresetCounts: SupplyPresetCounts
    applySupplyPreset: (next: SellableSupplyPresetSelection) => void
    sellableFilterPanelOpen: boolean
    setSellableFilterPanelOpen: SetState<boolean>
    hasStructuredSellableFilters: boolean
    applySellableFilters: () => void
    resetMoreFilters: () => void
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
    showSupplyPresetCounts?: boolean
    hiddenProductKinds?: readonly ProductKind[]
    applyHint?: string
}) {
    const prefix = idPrefix ?? "master-data-list-sellable-list-toolbar"
    const panelId = React.useId()
    const priceErrorId = React.useId()
    const hasChips = hasActiveFilters && appliedChips.length > 0

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
                        id={`${prefix}-search-input`}
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
                        <div
                            role="group"
                            aria-label="供应快捷筛选"
                            className="flex h-control max-w-full items-stretch overflow-x-auto rounded-lg border bg-muted/40 p-0.5 [&_[data-slot=button]]:h-full [&_[data-slot=button]]:min-h-0"
                        >
                            {SUPPLY_PRESET_OPTIONS.map((option) => {
                                const active = supplyPreset === option.value
                                return (
                                    <Button
                                        id={`master-data-sellable-preset-${toAutomationIdSegment(option.value)}`}
                                        key={option.value}
                                        type="button"
                                        variant={active ? "secondary" : "ghost"}
                                        className={
                                            active
                                                ? "bg-card shadow-xs"
                                                : "shadow-none"
                                        }
                                        aria-pressed={active}
                                        onClick={() =>
                                            applySupplyPreset(option.value)
                                        }
                                    >
                                        {option.label}
                                        {showSupplyPresetCounts ? (
                                            <span className="num text-xs text-muted-foreground">
                                                {
                                                    supplyPresetCounts[
                                                        option.value
                                                    ]
                                                }
                                            </span>
                                        ) : null}
                                    </Button>
                                )
                            })}
                        </div>
                        <Button
                            id={`${prefix}-filter-trigger`}
                            type="button"
                            variant="outline"
                            aria-expanded={sellableFilterPanelOpen}
                            aria-controls={panelId}
                            onClick={() =>
                                setSellableFilterPanelOpen((open) => !open)
                            }
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            更多筛选
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
                    hasChips || sellableFilterPanelOpen ? (
                        <div className="w-full space-y-3">
                            {hasChips ? (
                                <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                                    <span className="text-xs text-muted-foreground">
                                        已筛选
                                    </span>
                                    {appliedChips.map((chip) => (
                                        <FilterChip
                                            key={chip.key}
                                            id={`master-data-sellable-toolbar-filter-${toAutomationIdSegment(chip.key)}`}
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                removeFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        id="master-data-list-sellable-list-toolbar-button-3"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={clearAllFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {sellableFilterPanelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="公司商品池更多筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        label="商品类型"
                                        value={productKindDraft}
                                        onValueChange={setProductKindDraft}
                                        options={PRODUCT_KIND_RADIO_FILTER_OPTIONS.filter(
                                            (option) =>
                                                option.value === "all" ||
                                                !hiddenProductKinds?.includes(
                                                    option.value as ProductKind,
                                                ),
                                        )}
                                    />
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                分类
                                            </span>
                                            <CategoryCombobox
                                                id="master-data-list-sellable-list-toolbar-categorycombobox-1"
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
                                                id="master-data-list-sellable-list-toolbar-optioncombobox-1"
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
                                                id="master-data-list-sellable-list-toolbar-optioncombobox-2"
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
                                                可供区域
                                            </span>
                                            <Input
                                                id="master-data-list-sellable-list-toolbar-input-1"
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
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm sm:col-span-2">
                                            <span className="text-muted-foreground">
                                                销售价
                                            </span>
                                            <div className="flex items-center gap-1.5">
                                                <Input
                                                    id="master-data-list-sellable-list-toolbar-input-2"
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
                                                    id="master-data-list-sellable-list-toolbar-input-3"
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
                                            {applyHint}
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                id="master-data-list-sellable-list-toolbar-button-4"
                                                type="button"
                                                variant="ghost"
                                                onClick={resetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button
                                                id="master-data-list-sellable-list-toolbar-button-5"
                                                type="submit"
                                            >
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
