"use client"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { PermissionGuard } from "@/features/auth/components/permission-guard"

import * as React from "react"

import { CategoryCombobox, OptionCombobox } from "@/components/business"
import { DirectoryFilterControl } from "@/features/master-data/components/list/directory-filter-control"
import {
    DirectoryOptionFilter,
    brandDirectoryFilterBindings,
    supplierDirectoryFilterBindings,
} from "@/features/master-data/components/list/directory-option-filter"
import { GrossPriceRangeInputs } from "@/features/master-data/components/list/gross-price-range-inputs"
import {
    directoryOptionEmptyLabel,
    productFilterDirectoryState,
} from "@/features/master-data/lib/product-filter-directory"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceInlineFilter,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
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
]

const MORE_CHIP_KEYS = [
    "productBrandId",
    "productSupplierId",
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
    const account = useAccountProfileQuery()
    const permissions = account.isSuccess ? account.data.permissions : undefined
    const canFilterDirectory =
        hasPermission(permissions, "product_brand:list") ||
        hasPermission(permissions, "supplier:list")
    const directory = productFilterDirectoryState(productFilterOptionsQuery)
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
            morePresentation="inline"
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
                    <PermissionGuard permission="product_category:list">
                        <ListWorkspaceInlineFilter
                            htmlFor="master-data-list-sellable-list-toolbar-categorycombobox-1"
                            label="分类"
                        >
                            <DirectoryFilterControl
                                id="master-data-list-sellable-list-toolbar-categorycombobox-1"
                                className="w-full sm:w-60"
                                failed={directory.failed}
                                unavailable={directory.categoryUnavailable}
                                noScope={directory.categoryNoScope}
                                error={directory.error}
                                onRetry={directory.refetch}
                            >
                                <CategoryCombobox
                                    onRefresh={() =>
                                        productFilterOptionsQuery.refetch({
                                            throwOnError: true,
                                        })
                                    }
                                    id="master-data-list-sellable-list-toolbar-categorycombobox-1"
                                    className="w-full"
                                    aria-label="商品分类"
                                    categories={directory.categories}
                                    value={productCategoryIdDraft ?? undefined}
                                    onValueChange={(id) =>
                                        setProductCategoryIdDraft(id ?? null)
                                    }
                                    loading={directory.fetching}
                                    retainMissingSelection={false}
                                    placeholder="全部分类"
                                    emptyLabel={directoryOptionEmptyLabel({
                                        unavailable:
                                            directory.categoryUnavailable,
                                        unavailableLabel:
                                            "当前账号无分类查询权限",
                                        noScope: directory.categoryNoScope,
                                        failed: directory.failed,
                                        emptyLabel: "没有符合条件的分类",
                                    })}
                                />
                            </DirectoryFilterControl>
                        </ListWorkspaceInlineFilter>
                    </PermissionGuard>
                </>
            }
            morePanel={
                <div className="grid min-w-0 gap-5 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
                    {canFilterDirectory ? (
                        <fieldset className="min-w-0">
                            <legend className="mb-3 text-xs font-medium">
                                品牌与供货
                            </legend>
                            <div className="grid min-w-0 gap-3 sm:grid-cols-3">
                                <DirectoryOptionFilter
                                    {...brandDirectoryFilterBindings(
                                        directory,
                                        () =>
                                            productFilterOptionsQuery.refetch({
                                                throwOnError: true,
                                            }),
                                    )}
                                    id="master-data-list-sellable-list-toolbar-optioncombobox-1"
                                    comboboxClassName="w-full"
                                    ariaLabel="商品品牌"
                                    value={productBrandIdDraft}
                                    onValueChange={setProductBrandIdDraft}
                                />
                                <DirectoryOptionFilter
                                    {...supplierDirectoryFilterBindings(
                                        directory,
                                        () =>
                                            productFilterOptionsQuery.refetch({
                                                throwOnError: true,
                                            }),
                                    )}
                                    id="master-data-list-sellable-list-toolbar-optioncombobox-2"
                                    comboboxClassName="w-full"
                                    ariaLabel="供应商"
                                    value={productSupplierIdDraft}
                                    onValueChange={setProductSupplierIdDraft}
                                />
                            </div>
                        </fieldset>
                    ) : null}
                    <fieldset className="min-w-0 lg:border-l lg:pl-5">
                        <legend className="mb-3 text-xs font-medium">
                            一件代发价格
                        </legend>
                        <GrossPriceRangeInputs
                            minId="master-data-list-sellable-list-toolbar-input-2"
                            maxId="master-data-list-sellable-list-toolbar-input-3"
                            errorId={priceErrorId}
                            inputClassName="w-0 min-w-0 flex-1"
                            inputRef={priceInputRef}
                            min={productSalesPriceMinDraft}
                            max={productSalesPriceMaxDraft}
                            error={productSalesPriceError}
                            onMinChange={(value) => {
                                setProductSalesPriceMinDraft(value)
                                setProductSalesPriceError(null)
                            }}
                            onMaxChange={(value) => {
                                setProductSalesPriceMaxDraft(value)
                                setProductSalesPriceError(null)
                            }}
                        />
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
