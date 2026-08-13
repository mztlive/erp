"use client"

import * as React from "react"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { useCreatePermission } from "@/features/master-data/hooks/use-create-permission"
import { useClientPagedRows } from "@/features/master-data/hooks/use-client-paged-rows"
import { useProductListFilters } from "@/features/master-data/hooks/use-product-list-filters"
import {
    useMasterDataListQuery,
    useProductFilterOptionsQuery,
    useProductListSkusQuery,
    useProductListingMutation,
} from "@/features/master-data/hooks/queries"
import { useMasterDataListExport } from "@/features/master-data/hooks/use-master-data-list-export"
import { buildProductFilterSnapshotLabel } from "@/features/master-data/lib/master-data-list-summaries"
import { syncListMetrics } from "@/features/master-data/lib/master-data-list-summaries"
import { resourceLabel } from "@/features/master-data/lib/data"
import type {
    MasterDataListItem,
    ProductListSkuSummary,
} from "@/features/master-data/types"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/queries"
import type { FixedSku } from "@/features/supplier-offerings/types"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"

export function useProductListState(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const { canCreate, createBlockedReason } =
        useCreatePermission("product:create")
    const accountQuery = useAccountProfileQuery()
    const filters = useProductListFilters(searchInputRef)
    const listQuery = useMasterDataListQuery({
        resource: "products",
        q: filters.q.trim() || undefined,
        lifecycleStatus: filters.lifecycleStatus,
        revisionTiming: filters.revisionTiming,
        productKind: filters.productKind,
        productCategoryId: filters.productCategoryId,
        productBrandId: filters.productBrandId,
        productSupplierId: filters.productSupplierId,
        productListingStatus: filters.productListingStatus,
        productSupplyCoverage: filters.productSupplyCoverage,
        productSalesPriceMin: filters.productSalesPriceMin,
        productSalesPriceMax: filters.productSalesPriceMax,
    })
    const productFilterOptionsQuery = useProductFilterOptionsQuery(true)
    const { exportMeta, handleExport } = useMasterDataListExport()
    const productListingMutation = useProductListingMutation()
    const canUpdateProductListing = hasPermission(
        accountQuery.data?.permissions,
        "product:update",
    )
    const [disableTarget, setDisableTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [supplyProduct, setSupplyProduct] =
        React.useState<MasterDataListItem | null>(null)
    const [supplyDialogSku, setSupplyDialogSku] =
        React.useState<FixedSku | null>(null)
    const [listingError, setListingError] = React.useState<string | null>(null)

    const rows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data?.rows],
    )
    const pageRows = useClientPagedRows(rows, filters.pagination)
    const productPageIds = React.useMemo(
        () => pageRows.map((row) => row.stableId),
        [pageRows],
    )
    const productSkusQuery = useProductListSkusQuery(productPageIds)
    const productSkusByProduct = React.useMemo(() => {
        const grouped = new Map<string, ProductListSkuSummary[]>()
        for (const sku of productSkusQuery.data ?? []) {
            const skus = grouped.get(sku.productId) ?? []
            skus.push(sku)
            grouped.set(sku.productId, skus)
        }
        return grouped
    }, [productSkusQuery.data])
    const productPageSkuIds = React.useMemo(
        () => (productSkusQuery.data ?? []).map((sku) => sku.skuId),
        [productSkusQuery.data],
    )
    const supplierOfferingsQuery =
        useSupplierOfferingsForSkusQuery(productPageSkuIds)
    const currentSupplySkuIds = React.useMemo(
        () =>
            new Set(
                (supplierOfferingsQuery.data ?? [])
                    .filter(
                        (offering) =>
                            offering.status === "ACTIVE" &&
                            Boolean(offering.current_revision_id),
                    )
                    .map((offering) => offering.sku_id),
            ),
        [supplierOfferingsQuery.data],
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
    const syncedMetrics = React.useMemo(() => {
        const base = listQuery.data?.metrics ?? []
        if (rows.length === 0 || listQuery.data == null) return base
        return syncListMetrics(base, rows)
    }, [listQuery.data, rows])
    const filterSnapshotLabel = React.useMemo(
        () =>
            buildProductFilterSnapshotLabel({
                q: filters.q,
                lifecycleStatus: filters.lifecycleStatus,
                revisionTiming: filters.revisionTiming,
                productKind: filters.productKind,
                productListingStatus: filters.productListingStatus,
                productSupplyCoverage: filters.productSupplyCoverage,
                productSalesPriceMin: filters.productSalesPriceMin,
                productSalesPriceMax: filters.productSalesPriceMax,
                selectedCategoryLabel,
                selectedBrandLabel,
                selectedSupplierLabel,
            }),
        [
            filters.lifecycleStatus,
            filters.productKind,
            filters.productListingStatus,
            filters.productSalesPriceMax,
            filters.productSalesPriceMin,
            filters.productSupplyCoverage,
            filters.q,
            filters.revisionTiming,
            selectedBrandLabel,
            selectedCategoryLabel,
            selectedSupplierLabel,
        ],
    )

    const onExport = React.useCallback(() => {
        if (!listQuery.data || rows.length === 0) return
        void handleExport(
            {
                resource: "products",
                q: filters.q.trim() || undefined,
                lifecycleStatus: filters.lifecycleStatus,
                revisionTiming: filters.revisionTiming,
                productKind: filters.productKind,
                productCategoryId: filters.productCategoryId,
                productBrandId: filters.productBrandId,
                productSupplierId: filters.productSupplierId,
                productListingStatus: filters.productListingStatus,
                productSupplyCoverage: filters.productSupplyCoverage,
                productSalesPriceMin: filters.productSalesPriceMin,
                productSalesPriceMax: filters.productSalesPriceMax,
            },
            filterSnapshotLabel,
            resourceLabel("products"),
        )
    }, [
        filterSnapshotLabel,
        filters,
        handleExport,
        listQuery.data,
        rows.length,
    ])

    const updateProductListing = React.useCallback(
        async (item: MasterDataListItem, listed: boolean) => {
            if (
                !listed &&
                !window.confirm(
                    `下架后，商品「${item.name}」下的全部 SKU 都会退出公司商品池。确定继续？`,
                )
            ) {
                return
            }
            setListingError(null)
            try {
                await productListingMutation.mutateAsync({
                    productId: item.stableId,
                    listingStatus: listed ? "LISTED" : "UNLISTED",
                })
            } catch (error) {
                setListingError(
                    getErrorMessage(error, "上架状态更新失败，请刷新后重试。"),
                )
            }
        },
        [productListingMutation],
    )

    return {
        filters,
        listQuery,
        productFilterOptionsQuery,
        exportMeta,
        productListingMutation,
        canCreate,
        createBlockedReason,
        canUpdateProductListing,
        disableTarget,
        setDisableTarget,
        supplyProduct,
        setSupplyProduct,
        supplyDialogSku,
        setSupplyDialogSku,
        listingError,
        rows,
        pageRows,
        productPageSkuIds,
        productSkusByProduct,
        productSkusQuery,
        supplierOfferingsQuery,
        currentSupplySkuIds,
        syncedMetrics,
        onExport,
        updateProductListing,
    }
}
