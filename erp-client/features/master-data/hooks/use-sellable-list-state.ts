"use client"

import * as React from "react"
import type { SortingState } from "@tanstack/react-table"
import { PRICE_FILTER_LABELS } from "@/lib/price-filter"

import { useSellableListFilters } from "@/features/master-data/hooks/use-sellable-list-filters"
import { useProductFilterOptionsQuery } from "@/features/master-data/hooks/queries"
import { useSellableListData } from "@/features/master-data/hooks/use-sellable-list-data"
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
import { sellableDeliveryFilterText } from "@/features/master-data/lib/sellable-delivery-filter"
import { sellableSortFromTable } from "@/features/master-data/lib/sellable-sort"
import { PRODUCT_KIND_LABELS } from "@/features/master-data/types"
import { regionFullName, useRegionIndex } from "@/features/regions"
import type { SellableAppliedChip } from "@/features/master-data/components/list/sellable-list-toolbar"

export function useSellableListState(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const filters = useSellableListFilters(searchInputRef)
    const [sorting, setSorting] = React.useState<SortingState>([])
    const filterQuery = React.useMemo(
        () => ({
            resource: "sellable-items" as const,
            q: filters.q.trim() || undefined,
            productKind: filters.productKind,
            productCategoryId: filters.productCategoryId,
            productBrandId: filters.productBrandId,
            supplyRegion: filters.supplyRegion,
            deliveryRegion: filters.deliveryRegion,
            deliveryExtent: filters.deliveryExtent,
            includeFreightExtra: filters.includeFreightExtra,
            productPriceKind: filters.productPriceKind,
            productSalesPriceMin: filters.productSalesPriceMin,
            productSalesPriceMax: filters.productSalesPriceMax,
        }),
        [
            filters.q,
            filters.productKind,
            filters.productCategoryId,
            filters.productBrandId,
            filters.supplyRegion,
            filters.deliveryRegion,
            filters.deliveryExtent,
            filters.includeFreightExtra,
            filters.productPriceKind,
            filters.productSalesPriceMin,
            filters.productSalesPriceMax,
        ],
    )
    const list = useSellableListData({
        query: { ...filterQuery, ...sellableSortFromTable(sorting) },
        layout: filters.layout,
        pagination: filters.pagination,
    })
    const { changePagination, pagination } = filters
    /** 换排序回到第一页。 */
    const changeSorting = React.useCallback(
        (next: SortingState) => {
            setSorting(next)
            changePagination({ ...pagination, pageIndex: 0 })
        },
        [changePagination, pagination],
    )
    const { data: regionIndex } = useRegionIndex()
    const deliveryLabel = filters.deliveryRegion
        ? sellableDeliveryFilterText(
              regionIndex
                  ? regionFullName(regionIndex, filters.deliveryRegion)
                  : filters.deliveryRegion,
              filters.deliveryExtent,
              filters.includeFreightExtra,
          )
        : undefined
    const productFilterOptionsQuery = useProductFilterOptionsQuery(true)
    const { exportMeta, handleExport } = useMasterDataListExport()
    const [previewId, setPreviewId] = React.useState<string | null>(null)

    const rows = list.rows
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
        if (deliveryLabel) {
            chips.push({
                key: "deliveryRegion",
                label: `配送地区：${deliveryLabel}`,
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
        deliveryLabel,
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
                deliveryLabel,
                selectedCategoryLabel,
                selectedBrandLabel,
            }),
        [
            deliveryLabel,
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
                deliveryLabel,
                selectedCategoryLabel,
                selectedBrandLabel,
                rowCount: list.total,
            }),
        [
            deliveryLabel,
            filters.productKind,
            filters.productSalesPriceMax,
            filters.productPriceKind,
            filters.productSalesPriceMin,
            filters.q,
            filters.supplyRegion,
            list.total,
            selectedBrandLabel,
            selectedCategoryLabel,
        ],
    )

    const onExport = React.useCallback(() => {
        if (!list.loaded || list.total === 0) return
        void handleExport(
            {
                resource: "sellable-items",
                q: filters.q.trim() || undefined,
                productKind: filters.productKind,
                productCategoryId: filters.productCategoryId,
                productBrandId: filters.productBrandId,
                supplyRegion: filters.supplyRegion,
                deliveryRegion: filters.deliveryRegion,
                deliveryExtent: filters.deliveryExtent,
                includeFreightExtra: filters.includeFreightExtra,
                productPriceKind: filters.productPriceKind,
                productSalesPriceMin: filters.productSalesPriceMin,
                productSalesPriceMax: filters.productSalesPriceMax,
            },
            filterSnapshotLabel,
            resourceLabel("sellable-items"),
        )
    }, [filterSnapshotLabel, filters, handleExport, list.loaded, list.total])

    return {
        filters,
        /** 筛选条件标识；变化时清空勾选。 */
        selectionResetKey: JSON.stringify(filterQuery),
        list,
        sorting,
        changeSorting,
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
