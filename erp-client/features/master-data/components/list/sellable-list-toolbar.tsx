"use client"

import * as React from "react"

import { CategoryCombobox, OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspaceInlineFilter,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"
import type {
    SellableFilterKey,
    SellableSupplyPresetSelection,
} from "@/features/master-data/hooks/use-sellable-list-filters"
import {
    masterDataCopy,
    masterDataSearchPlaceholder,
} from "@/features/master-data/lib/copy"
import { PRODUCT_KIND_FILTER_OPTIONS } from "@/features/master-data/lib/list-filters"
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

const MORE_CHIP_KEYS = [
    "productBrandId",
    "productSupplierId",
    "supplyRegion",
    "salesPrice",
] as const

export function SellableListToolbar({
    idPrefix,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    clearAllFilters,
    appliedChips,
    removeFilter,
    supplyPreset,
    supplyPresetCounts,
    applySupplyPreset,
    sellableFilterPanelOpen,
    setSellableFilterPanelOpen,
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
    showSupplyPreset = true,
    hiddenProductKinds,
    applyHint,
    variant = "default",
    actions,
    hasPendingChanges = false,
    resultCount,
    loading = false,
    failed = false,
}: {
    idPrefix?: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: SetState<string>
    hasActiveFilters?: boolean
    clearAllFilters: () => void
    appliedChips: readonly SellableAppliedChip[]
    removeFilter: (key: SellableFilterKey) => void
    supplyPreset: SellableSupplyPresetSelection
    supplyPresetCounts: SupplyPresetCounts
    applySupplyPreset: (next: SellableSupplyPresetSelection) => void
    sellableFilterPanelOpen: boolean
    setSellableFilterPanelOpen: SetState<boolean>
    hasStructuredSellableFilters?: boolean
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
    showSupplyPreset?: boolean
    hiddenProductKinds?: readonly ProductKind[]
    applyHint?: string
    variant?: "default" | "quiet"
    actions?: React.ReactNode
    hasPendingChanges?: boolean
    resultCount?: number
    loading?: boolean
    failed?: boolean
}) {
    const prefix = idPrefix ?? "master-data-list-sellable-list-toolbar"
    const panelId = `${prefix}-more-panel`
    const priceErrorId = `${prefix}-price-error`
    const priceInputRef = React.useRef<HTMLInputElement>(null)
    const moreCount = appliedChips.filter(({ key }) =>
        MORE_CHIP_KEYS.includes(key as (typeof MORE_CHIP_KEYS)[number]),
    ).length

    React.useEffect(() => {
        if (productSalesPriceError && sellableFilterPanelOpen) {
            priceInputRef.current?.focus()
        }
    }, [productSalesPriceError, sellableFilterPanelOpen])

    return (
        <ListWorkspaceFilterBar
            idPrefix={prefix}
            formAriaLabel="公司商品池查询"
            onSubmit={applySellableFilters}
            className={
                variant === "quiet"
                    ? "gap-0 [&_[data-slot=list-toolbar-primary]_[data-slot=separator]]:hidden"
                    : undefined
            }
            search={
                <ListSearchField
                    id={`${prefix}-search-input`}
                    searchInputRef={searchInputRef}
                    value={searchDraft}
                    onChange={setSearchDraft}
                    placeholder={masterDataSearchPlaceholder("sellable-items")}
                    aria-label={masterDataCopy.searchAria}
                />
            }
            queryButtonId="master-data-list-sellable-list-toolbar-button-5"
            moreButtonId={`${prefix}-filter-trigger`}
            resetMoreButtonId="master-data-list-sellable-list-toolbar-button-4"
            clearButtonId="master-data-list-sellable-list-toolbar-button-3"
            moreCount={moreCount}
            moreOpen={sellableFilterPanelOpen}
            onToggleMore={() => setSellableFilterPanelOpen((open) => !open)}
            morePanelId={panelId}
            morePanelAriaLabel="公司商品池更多筛选条件"
            moreHint={applyHint ?? "可组合多个条件，点击「查询」后统一生效。"}
            onResetMore={resetMoreFilters}
            extraPrimary={
                showSupplyPreset ? (
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
                                            {supplyPresetCounts[option.value]}
                                        </span>
                                    ) : null}
                                </Button>
                            )
                        })}
                    </div>
                ) : undefined
            }
            actions={actions}
            commonFilters={
                <>
                    <ListWorkspaceInlineFilter
                        htmlFor={`${prefix}-kind`}
                        label="商品类型"
                    >
                        <OptionCombobox
                            id={`${prefix}-kind`}
                            className="w-40 max-w-full min-w-0"
                            aria-label="商品类型"
                            value={
                                productKindDraft === "all"
                                    ? null
                                    : productKindDraft
                            }
                            options={PRODUCT_KIND_FILTER_OPTIONS.filter(
                                (option) =>
                                    !hiddenProductKinds?.includes(option.value),
                            )}
                            placeholder="全部"
                            onValueChange={(value) =>
                                setProductKindDraft(
                                    PRODUCT_KIND_FILTER_OPTIONS.some(
                                        (option) => option.value === value,
                                    )
                                        ? (value as ProductKind)
                                        : "all",
                                )
                            }
                        />
                    </ListWorkspaceInlineFilter>
                    <ListWorkspaceInlineFilter
                        htmlFor="master-data-list-sellable-list-toolbar-categorycombobox-1"
                        label="分类"
                    >
                        <CategoryCombobox
                            id="master-data-list-sellable-list-toolbar-categorycombobox-1"
                            className="w-full sm:w-60"
                            aria-label="商品分类"
                            categories={
                                productFilterOptionsQuery.data?.categories ?? []
                            }
                            value={productCategoryIdDraft ?? undefined}
                            onValueChange={(id) =>
                                setProductCategoryIdDraft(id ?? null)
                            }
                            loading={productFilterOptionsQuery.isPending}
                            placeholder="全部分类"
                        />
                    </ListWorkspaceInlineFilter>
                </>
            }
            morePanel={
                <div className="grid min-w-0 gap-5 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
                    <fieldset className="min-w-0">
                        <legend className="mb-3 text-xs font-medium">
                            品牌与供货
                        </legend>
                        <div className="grid min-w-0 gap-3 sm:grid-cols-3">
                            <ListWorkspaceFilterField
                                htmlFor="master-data-list-sellable-list-toolbar-optioncombobox-1"
                                label="品牌"
                            >
                                <OptionCombobox
                                    id="master-data-list-sellable-list-toolbar-optioncombobox-1"
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
                            </ListWorkspaceFilterField>
                            <ListWorkspaceFilterField
                                htmlFor="master-data-list-sellable-list-toolbar-optioncombobox-2"
                                label="供应商"
                            >
                                <OptionCombobox
                                    id="master-data-list-sellable-list-toolbar-optioncombobox-2"
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
                            </ListWorkspaceFilterField>
                            <ListWorkspaceFilterField
                                htmlFor="master-data-list-sellable-list-toolbar-input-1"
                                label="可供区域"
                            >
                                <Input
                                    id="master-data-list-sellable-list-toolbar-input-1"
                                    className="w-full"
                                    value={supplyRegionDraft}
                                    onChange={(event) =>
                                        setSupplyRegionDraft(event.target.value)
                                    }
                                    autoComplete="off"
                                    placeholder="如：全国"
                                    aria-label="可供区域"
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
                                    id="master-data-list-sellable-list-toolbar-input-2"
                                    ref={priceInputRef}
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
                                <span className="text-xs text-muted-foreground">
                                    至
                                </span>
                                <Input
                                    id="master-data-list-sellable-list-toolbar-input-3"
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
                                <p
                                    id={priceErrorId}
                                    className="text-xs text-destructive"
                                    role="alert"
                                >
                                    {productSalesPriceError}
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
            onClearChip={(key) => removeFilter(key as SellableFilterKey)}
            onClearAll={clearAllFilters}
            hasPendingChanges={hasPendingChanges}
            pendingHint="条件已修改，待查询"
            idleHint="结果与当前查询条件一致"
        />
    )
}
