"use client"

import * as React from "react"

import { useClientPagedRows } from "@/features/master-data/hooks/use-client-paged-rows"
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

    const rows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data?.rows],
    )
    const pageRows = useClientPagedRows(rows, filters.pagination)
    const previewRow = React.useMemo(
        () => rows.find((row) => row.stableId === previewId) ?? null,
        [previewId, rows],
    )
    const selectedCategoryLabel = React.useMemo(
        () =>
            productFilterOptionsQuery.data?.categories.find(
                (option) => option.categoryId === filters.productCategoryId,
            )?.categoryName ?? filters.productCategoryId,
        [
            filters.productCategoryId,
            productFilterOptionsQuery.data?.categories,
        ],
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
            }),
        [
            filters.productKind,
            filters.productSalesPriceMax,
            filters.productSalesPriceMin,
            filters.q,
            filters.supplyRegion,
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
                rowCount: rows.length,
            }),
        [
            filters.productKind,
            filters.productSalesPriceMax,
            filters.productSalesPriceMin,
            filters.q,
            filters.supplyRegion,
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
        pageRows,
        previewRow,
        sellableTableDescription,
        onExport,
    }
}
