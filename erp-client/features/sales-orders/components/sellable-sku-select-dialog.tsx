"use client"

import * as React from "react"
import type { RowSelectionState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    FilterChip,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { SellableListToolbar } from "@/features/master-data/components/list/sellable-list-toolbar"
import type { SellableAppliedChip } from "@/features/master-data/components/list/sellable-list-toolbar"
import { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"
import { SELLABLE_SUPPLY_PRESET_LABELS } from "@/features/master-data/lib/sellable-supply-preset"
import { useSellableSkuPickerColumns } from "@/features/sales-orders/components/sellable-sku-picker-columns"
import { useSellableSkuPickerQuery } from "@/features/sales-orders/hooks/use-sellable-sku-picker-query"
import {
    PRODUCT_KIND_LABELS,
    type MasterDataListItem,
    type ProductKind,
} from "@/features/master-data/types"
import { useSellableSkuPickerFilters } from "@/features/sales-orders/hooks/use-sellable-sku-picker-filters"
import {
    sellableItemToPick,
    type SellableSkuPick,
} from "@/features/sales-orders/lib/sellable-sku-pick"
import { toSellablePickerListQuery } from "@/features/sales-orders/lib/sellable-sku-picker-query"
import { toAutomationIdSegment } from "@/lib/automation-id"

export type SellableSkuSelectDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    multiple?: boolean
    excludeProductKind?: ProductKind
    title?: string
    description?: string
    confirmLabel?: string
    onConfirm: (picks: readonly SellableSkuPick[]) => void
}

const EMPTY_SUPPLY_PRESET_COUNTS = {
    all: 0,
    "single-supplier": 0,
    nationwide: 0,
} as const

function compactSelection(selection: RowSelectionState): RowSelectionState {
    return Object.fromEntries(
        Object.entries(selection).filter(([, selected]) => Boolean(selected)),
    )
}

export function SellableSkuSelectDialog({
    open,
    onOpenChange,
    multiple = true,
    excludeProductKind,
    title = "选择商品",
    description = "用分类、品牌、供应商、区域和售价从公司商品池筛选，不必只靠关键字。",
    confirmLabel,
    onConfirm,
}: SellableSkuSelectDialogProps) {
    const filters = useSellableSkuPickerFilters()
    const columns = useSellableSkuPickerColumns()
    const productFilterOptionsQuery = useProductFilterOptionsQuery(open)
    const listQueryInput = React.useMemo(
        () =>
            toSellablePickerListQuery(
                {
                    q: filters.q,
                    supplyPreset: filters.supplyPreset ?? "all",
                    productKind: filters.productKind,
                    productCategoryId: filters.productCategoryId,
                    productBrandId: filters.productBrandId,
                    productSupplierId: filters.productSupplierId,
                    supplyRegion: filters.supplyRegion,
                    productSalesPriceMin: filters.productSalesPriceMin,
                    productSalesPriceMax: filters.productSalesPriceMax,
                },
                filters.pagination,
            ),
        [filters],
    )
    const listQuery = useSellableSkuPickerQuery(listQueryInput, open)
    const [rowSelection, setRowSelection] = React.useState<RowSelectionState>(
        {},
    )
    const selectedItemsRef = React.useRef(new Map<string, MasterDataListItem>())
    const [selectedItems, setSelectedItems] = React.useState<
        MasterDataListItem[]
    >([])

    const rows = React.useMemo(() => {
        const source = listQuery.data?.rows ?? []
        if (!excludeProductKind) return [...source]
        return source.filter(
            (row) =>
                row.productKind?.toUpperCase() !==
                excludeProductKind.toUpperCase(),
        )
    }, [excludeProductKind, listQuery.data?.rows])

    React.useEffect(() => {
        if (!open) return
        selectedItemsRef.current = new Map()
        setSelectedItems([])
        setRowSelection({})
    }, [open])

    const selectedCategoryLabel = React.useMemo(
        () =>
            productFilterOptionsQuery.data?.categories.find(
                (option) => option.categoryId === filters.productCategoryId,
            )?.categoryName ?? filters.productCategoryId,
        [filters.productCategoryId, productFilterOptionsQuery.data?.categories],
    )
    const selectedBrandLabel = React.useMemo(
        () =>
            productFilterOptionsQuery.data?.brands.find(
                (option) => option.value === filters.productBrandId,
            )?.label ?? filters.productBrandId,
        [filters.productBrandId, productFilterOptionsQuery.data?.brands],
    )
    const selectedSupplierLabel = React.useMemo(
        () =>
            (productFilterOptionsQuery.data?.suppliers ?? []).find(
                (option) => option.value === filters.productSupplierId,
            )?.label ?? filters.productSupplierId,
        [filters.productSupplierId, productFilterOptionsQuery.data?.suppliers],
    )

    const appliedChips = React.useMemo<readonly SellableAppliedChip[]>(() => {
        const chips: SellableAppliedChip[] = []
        if (filters.q.trim()) {
            chips.push({ key: "q", label: `搜索：${filters.q.trim()}` })
        }
        if (filters.supplyPreset) {
            chips.push({
                key: "supplyPreset",
                label: SELLABLE_SUPPLY_PRESET_LABELS[filters.supplyPreset],
            })
        }
        if (filters.productKind) {
            chips.push({
                key: "productKind",
                label: `类型：${PRODUCT_KIND_LABELS[filters.productKind]}`,
            })
        }
        if (filters.productCategoryId) {
            chips.push({
                key: "productCategoryId",
                label: `分类：${selectedCategoryLabel}`,
            })
        }
        if (filters.productBrandId) {
            chips.push({
                key: "productBrandId",
                label: `品牌：${selectedBrandLabel}`,
            })
        }
        if (filters.productSupplierId) {
            chips.push({
                key: "productSupplierId",
                label: `供应商：${selectedSupplierLabel}`,
            })
        }
        if (filters.supplyRegion) {
            chips.push({
                key: "supplyRegion",
                label: `可供区域：${filters.supplyRegion}`,
            })
        }
        if (filters.productSalesPriceMin || filters.productSalesPriceMax) {
            const minimum = filters.productSalesPriceMin ?? "不限"
            const maximum = filters.productSalesPriceMax ?? "不限"
            chips.push({
                key: "salesPrice",
                label: `销售价：${minimum} 至 ${maximum}`,
            })
        }
        return chips
    }, [
        filters.productBrandId,
        filters.productCategoryId,
        filters.productKind,
        filters.productSalesPriceMax,
        filters.productSalesPriceMin,
        filters.productSupplierId,
        filters.q,
        filters.supplyPreset,
        filters.supplyRegion,
        selectedBrandLabel,
        selectedCategoryLabel,
        selectedSupplierLabel,
    ])

    const hasActiveFilters =
        filters.q.trim() !== "" ||
        filters.hasStructuredSellableFilters ||
        filters.supplyPreset != null

    const handleRowSelectionChange = React.useCallback(
        (next: RowSelectionState) => {
            let resolved = compactSelection(next)
            if (!multiple) {
                const ids = Object.keys(resolved)
                const added = ids.find((id) => !rowSelection[id])
                const kept = added ?? ids[ids.length - 1]
                resolved = kept ? { [kept]: true } : {}
            }
            const nextItems = new Map<string, MasterDataListItem>()
            for (const [id, item] of selectedItemsRef.current) {
                if (resolved[id]) nextItems.set(id, item)
            }
            for (const row of rows) {
                if (resolved[row.stableId]) nextItems.set(row.stableId, row)
            }
            selectedItemsRef.current = nextItems
            setRowSelection(resolved)
            setSelectedItems([...nextItems.values()])
        },
        [multiple, rowSelection, rows],
    )

    const removeSelected = React.useCallback(
        (skuId: string) => {
            handleRowSelectionChange({ ...rowSelection, [skuId]: false })
        },
        [handleRowSelectionChange, rowSelection],
    )

    const handleConfirm = React.useCallback(() => {
        const picks = selectedItems.map(sellableItemToPick)
        if (picks.length === 0) return
        onConfirm(multiple ? picks : picks.slice(0, 1))
        onOpenChange(false)
    }, [multiple, onConfirm, onOpenChange, selectedItems])

    const resolvedConfirmLabel =
        confirmLabel ??
        (multiple ? `加入所选（${selectedItems.length}）` : "使用该商品")

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="sales-orders-sku-picker-close"
                className="flex max-h-[90vh] w-full flex-col gap-4 overflow-hidden sm:max-w-6xl"
                showCloseButton
            >
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                    <DialogDescription>{description}</DialogDescription>
                </DialogHeader>

                <div id="sales-orders-sku-picker-toolbar">
                    <SellableListToolbar
                        searchInputRef={filters.searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        hasActiveFilters={hasActiveFilters}
                        clearAllFilters={filters.clearAllFilters}
                        appliedChips={appliedChips}
                        removeFilter={filters.removeFilter}
                        supplyPreset={filters.supplyPreset ?? "all"}
                        supplyPresetCounts={EMPTY_SUPPLY_PRESET_COUNTS}
                        applySupplyPreset={filters.applySupplyPreset}
                        sellableFilterPanelOpen={
                            filters.sellableFilterPanelOpen
                        }
                        setSellableFilterPanelOpen={
                            filters.setSellableFilterPanelOpen
                        }
                        hasStructuredSellableFilters={
                            filters.hasStructuredSellableFilters
                        }
                        applySellableFilters={filters.applySellableFilters}
                        resetMoreFilters={filters.resetMoreFilters}
                        supplyRegionDraft={filters.supplyRegionDraft}
                        setSupplyRegionDraft={filters.setSupplyRegionDraft}
                        productKindDraft={filters.productKindDraft}
                        setProductKindDraft={filters.setProductKindDraft}
                        productCategoryIdDraft={filters.productCategoryIdDraft}
                        setProductCategoryIdDraft={
                            filters.setProductCategoryIdDraft
                        }
                        productBrandIdDraft={filters.productBrandIdDraft}
                        setProductBrandIdDraft={filters.setProductBrandIdDraft}
                        productSupplierIdDraft={filters.productSupplierIdDraft}
                        setProductSupplierIdDraft={
                            filters.setProductSupplierIdDraft
                        }
                        productSalesPriceMinDraft={
                            filters.productSalesPriceMinDraft
                        }
                        setProductSalesPriceMinDraft={
                            filters.setProductSalesPriceMinDraft
                        }
                        productSalesPriceMaxDraft={
                            filters.productSalesPriceMaxDraft
                        }
                        setProductSalesPriceMaxDraft={
                            filters.setProductSalesPriceMaxDraft
                        }
                        productSalesPriceError={filters.productSalesPriceError}
                        setProductSalesPriceError={
                            filters.setProductSalesPriceError
                        }
                        productFilterOptionsQuery={productFilterOptionsQuery}
                        showSupplyPresetCounts={false}
                        hiddenProductKinds={
                            excludeProductKind
                                ? [excludeProductKind]
                                : undefined
                        }
                        applyHint="将同时应用上方关键词和以下筛选条件。"
                    />
                </div>

                <div className="min-h-96 flex-1 overflow-hidden">
                    <DataTable
                        id="sales-orders-sku-picker-table"
                        data={rows}
                        columns={columns}
                        getRowId={(row) => row.stableId}
                        rowLabel={(row) =>
                            row.sellableItem
                                ? `${row.name} · ${row.sellableItem.specificationLabel}`
                                : row.name
                        }
                        rowCount={listQuery.data?.total ?? 0}
                        pagination={filters.pagination}
                        onPaginationChange={filters.changePagination}
                        rowSelection={rowSelection}
                        onRowSelectionChange={handleRowSelectionChange}
                        enableRowSelection
                        enableMultiRowSelection={multiple}
                        manualPagination
                        manualSorting
                        loading={listQuery.isFetching}
                        layout="flush"
                        showColumnVisibility={false}
                        defaultColumnPinning={{ left: ["name"] }}
                        pageSizeOptions={[20, 50, 100]}
                        errorState={
                            listQuery.isError ? (
                                <BusinessFailureState
                                    error={listQuery.error}
                                    onRetry={() => void listQuery.refetch()}
                                />
                            ) : undefined
                        }
                        emptyState={
                            !listQuery.isError &&
                            !listQuery.isPending &&
                            rows.length === 0 ? (
                                <BusinessEmptyState
                                    kind={
                                        hasActiveFilters ? "filter" : "no-data"
                                    }
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    title={
                                        hasActiveFilters
                                            ? "当前筛选无结果"
                                            : "还没有可销售的 SKU"
                                    }
                                    description={
                                        hasActiveFilters
                                            ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                            : "商品需要已上架、资料有效且存在有效供给，才会出现在这里。"
                                    }
                                    action={
                                        hasActiveFilters ? (
                                            <Button
                                                id="sales-orders-sku-picker-clear-filters"
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                onClick={
                                                    filters.clearAllFilters
                                                }
                                            >
                                                清除筛选
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                    />
                </div>

                {selectedItems.length > 0 ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <span className="text-xs text-muted-foreground">
                            已选 {selectedItems.length} 个
                        </span>
                        {selectedItems.map((item) => (
                            <FilterChip
                                key={item.stableId}
                                id={`sales-orders-sku-picker-selected-${toAutomationIdSegment(item.stableId)}`}
                                label={item.name}
                                clearLabel={`取消选择${item.name}`}
                                onClear={() => removeSelected(item.stableId)}
                            />
                        ))}
                    </div>
                ) : null}

                <DialogFooter>
                    <Button
                        id="sales-orders-sku-picker-cancel"
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        id="sales-orders-sku-picker-confirm"
                        type="button"
                        disabled={selectedItems.length === 0}
                        onClick={handleConfirm}
                    >
                        {resolvedConfirmLabel}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
