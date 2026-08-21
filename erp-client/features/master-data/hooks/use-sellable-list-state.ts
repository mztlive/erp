"use client"

import * as React from "react"

import { useSellableListFilters } from "@/features/master-data/hooks/use-sellable-list-filters"
import {
    useMasterDataListQuery,
    useProductFilterOptionsQuery,
} from "@/features/master-data/hooks/queries"
import { useMasterDataListExport } from "@/features/master-data/hooks/use-master-data-list-export"
import {
    buildSellableFilterSnapshotLabel,
    buildSellableTableDescription,
} from "@/features/master-data/lib/master-data-list-summaries"
import { resourceLabel } from "@/features/master-data/lib/data"
import { PRODUCT_KIND_LABELS } from "@/features/master-data/types"
import type { SellableAppliedChip } from "@/features/master-data/components/list/sellable-list-toolbar"
import {
    filterBySellableSupplyPreset,
    SELLABLE_SUPPLY_PRESET_LABELS,
} from "@/features/master-data/lib/sellable-supply-preset"

export function useSellableListState(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const filters = useSellableListFilters(searchInputRef)
    const listQuery = useMasterDataListQuery({
        resource: "sellable-items",
        q: filters.q.trim() || undefined,
        productKind: filters.productKind,
        productCategoryId: filters.productCategoryId,
        productBrandId: filters.productBrandId,
        productSupplierId: filters.productSupplierId,
        supplyRegion: filters.supplyRegion,
        productSalesPriceMin: filters.productSalesPriceMin,
        productSalesPriceMax: filters.productSalesPriceMax,
    })
    const productFilterOptionsQuery = useProductFilterOptionsQuery(true)
    const { exportMeta, handleExport } = useMasterDataListExport()
    const [previewId, setPreviewId] = React.useState<string | null>(null)

    // DataTable 需要可变数组：整份结果直接交给表格做排序与分页
    const baseRows = React.useMemo(
        () => [...(listQuery.data?.rows ?? [])],
        [listQuery.data?.rows],
    )
    const rows = React.useMemo(
        () => filterBySellableSupplyPreset(baseRows, filters.supplyPreset),
        [baseRows, filters.supplyPreset],
    )
    const supplyPresetCounts = React.useMemo(
        () => ({
            all: baseRows.length,
            "single-supplier": filterBySellableSupplyPreset(
                baseRows,
                "single-supplier",
            ).length,
            nationwide: filterBySellableSupplyPreset(baseRows, "nationwide")
                .length,
        }),
        [baseRows],
    )
    const previewRow = React.useMemo(
        () => rows.find((row) => row.stableId === previewId) ?? null,
        [previewId, rows],
    )
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
    /** 所有已生效条件均可从 chip 单独撤销。 */
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

    const filterSnapshotLabel = React.useMemo(
        () =>
            buildSellableFilterSnapshotLabel({
                q: filters.q,
                productKind: filters.productKind,
                productSalesPriceMin: filters.productSalesPriceMin,
                productSalesPriceMax: filters.productSalesPriceMax,
                supplyRegion: filters.supplyRegion,
                selectedCategoryLabel,
                selectedBrandLabel,
                selectedSupplierLabel,
                supplyPresetLabel: filters.supplyPreset
                    ? SELLABLE_SUPPLY_PRESET_LABELS[filters.supplyPreset]
                    : undefined,
            }),
        [
            filters.productKind,
            filters.productSalesPriceMax,
            filters.productSalesPriceMin,
            filters.q,
            filters.supplyRegion,
            filters.supplyPreset,
            selectedBrandLabel,
            selectedCategoryLabel,
            selectedSupplierLabel,
        ],
    )
    const sellableTableDescription = React.useMemo(
        () =>
            buildSellableTableDescription({
                q: filters.q,
                productKind: filters.productKind,
                productSalesPriceMin: filters.productSalesPriceMin,
                productSalesPriceMax: filters.productSalesPriceMax,
                supplyRegion: filters.supplyRegion,
                selectedCategoryLabel,
                selectedBrandLabel,
                selectedSupplierLabel,
                supplyPresetLabel: filters.supplyPreset
                    ? SELLABLE_SUPPLY_PRESET_LABELS[filters.supplyPreset]
                    : undefined,
                rowCount: rows.length,
            }),
        [
            filters.productKind,
            filters.productSalesPriceMax,
            filters.productSalesPriceMin,
            filters.q,
            filters.supplyRegion,
            filters.supplyPreset,
            rows.length,
            selectedBrandLabel,
            selectedCategoryLabel,
            selectedSupplierLabel,
        ],
    )

    const onExport = React.useCallback(() => {
        if (!listQuery.data || rows.length === 0) return
        void handleExport(
            {
                resource: "sellable-items",
                q: filters.q.trim() || undefined,
                productKind: filters.productKind,
                productCategoryId: filters.productCategoryId,
                productBrandId: filters.productBrandId,
                productSupplierId: filters.productSupplierId,
                supplyRegion: filters.supplyRegion,
                productSalesPriceMin: filters.productSalesPriceMin,
                productSalesPriceMax: filters.productSalesPriceMax,
                sellableSupplyPreset: filters.supplyPreset,
            },
            filterSnapshotLabel,
            resourceLabel("sellable-items"),
        )
    }, [
        filterSnapshotLabel,
        filters,
        handleExport,
        listQuery.data,
        rows.length,
    ])

    return {
        filters,
        listQuery,
        productFilterOptionsQuery,
        exportMeta,
        previewId,
        setPreviewId,
        rows,
        supplyPresetCounts,
        previewRow,
        appliedChips,
        sellableTableDescription,
        onExport,
    }
}
