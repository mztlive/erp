"use client"

import * as React from "react"
import { PRICE_FILTER_LABELS } from "@/lib/price-filter"

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
import {
    directorySelectionLabel,
    productFilterDirectoryState,
} from "@/features/master-data/lib/product-filter-directory"
import { PRODUCT_KIND_LABELS } from "@/features/master-data/types"
import type { SellableAppliedChip } from "@/features/master-data/components/list/sellable-list-toolbar"

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
        supplyRegion: filters.supplyRegion,
        productPriceKind: filters.productPriceKind,
        productSalesPriceMin: filters.productSalesPriceMin,
        productSalesPriceMax: filters.productSalesPriceMax,
    })
    const productFilterOptionsQuery = useProductFilterOptionsQuery(true)
    const { exportMeta, handleExport } = useMasterDataListExport()
    const [previewId, setPreviewId] = React.useState<string | null>(null)

    // DataTable 需要可变数组：整份结果直接交给表格做排序与分页
    const rows = React.useMemo(
        () => [...(listQuery.data?.rows ?? [])],
        [listQuery.data?.rows],
    )
    const previewRow = React.useMemo(
        () => rows.find((row) => row.stableId === previewId) ?? null,
        [previewId, rows],
    )
    const directory = productFilterDirectoryState(productFilterOptionsQuery)
    const selectedCategoryLabel = directorySelectionLabel(
        filters.productCategoryId,
        directory.categories,
        (option) => option.categoryId,
        (option) => option.categoryName,
        directory,
    )
    const selectedBrandLabel = directorySelectionLabel(
        filters.productBrandId,
        directory.brands,
        (option) => option.value,
        (option) => option.label,
        directory,
    )
    /** 所有已生效条件均可从 chip 单独撤销。 */
    const appliedChips = React.useMemo<readonly SellableAppliedChip[]>(() => {
        const chips: SellableAppliedChip[] = []
        if (filters.q.trim()) {
            chips.push({ key: "q", label: `搜索：${filters.q.trim()}` })
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
                label: `${PRICE_FILTER_LABELS[filters.productPriceKind]}：${minimum} 至 ${maximum}`,
            })
        }
        return chips
    }, [
        filters.productBrandId,
        filters.productCategoryId,
        filters.productKind,
        filters.productSalesPriceMax,
        filters.productPriceKind,
        filters.productSalesPriceMin,
        filters.q,
        filters.supplyRegion,
        selectedBrandLabel,
        selectedCategoryLabel,
    ])

    const filterSnapshotLabel = React.useMemo(
        () =>
            buildSellableFilterSnapshotLabel({
                q: filters.q,
                productKind: filters.productKind,
                productPriceKind: filters.productPriceKind,
                productSalesPriceMin: filters.productSalesPriceMin,
                productSalesPriceMax: filters.productSalesPriceMax,
                supplyRegion: filters.supplyRegion,
                selectedCategoryLabel,
                selectedBrandLabel,
            }),
        [
            filters.productKind,
            filters.productSalesPriceMax,
            filters.productPriceKind,
            filters.productSalesPriceMin,
            filters.q,
            filters.supplyRegion,
            selectedBrandLabel,
            selectedCategoryLabel,
        ],
    )
    const sellableTableDescription = React.useMemo(
        () =>
            buildSellableTableDescription({
                q: filters.q,
                productKind: filters.productKind,
                productPriceKind: filters.productPriceKind,
                productSalesPriceMin: filters.productSalesPriceMin,
                productSalesPriceMax: filters.productSalesPriceMax,
                supplyRegion: filters.supplyRegion,
                selectedCategoryLabel,
                selectedBrandLabel,
                rowCount: rows.length,
            }),
        [
            filters.productKind,
            filters.productSalesPriceMax,
            filters.productPriceKind,
            filters.productSalesPriceMin,
            filters.q,
            filters.supplyRegion,
            rows.length,
            selectedBrandLabel,
            selectedCategoryLabel,
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
                supplyRegion: filters.supplyRegion,
                productPriceKind: filters.productPriceKind,
                productSalesPriceMin: filters.productSalesPriceMin,
                productSalesPriceMax: filters.productSalesPriceMax,
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
        previewRow,
        appliedChips,
        filterSnapshotLabel,
        sellableTableDescription,
        onExport,
    }
}
