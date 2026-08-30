"use client"

import { keepPreviousData, useQuery } from "@tanstack/react-query"

import { listSellableItemsPage } from "@/features/master-data/api"
import { fetchFileAsset } from "@/features/master-data/api/media-assets"
import type { SellableSkuPickerListQuery } from "@/features/sales-orders/lib/sellable-sku-picker-query"

const STALE_TIME = 5 * 60 * 1000

export function useSellableSkuPickerQuery(
    input: SellableSkuPickerListQuery,
    enabled: boolean,
) {
    return useQuery({
        queryKey: ["sales-orders", "sellable-sku-picker", input],
        queryFn: () =>
            listSellableItemsPage({
                resource: "sellable-items",
                q: input.q,
                productKind: input.productKind,
                productCategoryId: input.productCategoryId,
                productBrandId: input.productBrandId,
                productSupplierId: input.productSupplierId,
                supplyRegion: input.supplyRegion,
                productSalesPriceMin: input.productSalesPriceMin,
                productSalesPriceMax: input.productSalesPriceMax,
                maxSupplierCount: input.maxSupplierCount,
                page: input.page,
                pageSize: input.pageSize,
            }),
        enabled,
        placeholderData: keepPreviousData,
        staleTime: STALE_TIME,
    })
}

export function useFileAssetQuery(assetId: string | undefined) {
    return useQuery({
        queryKey: [
            "sales-orders",
            "sellable-sku-picker",
            "file",
            assetId ?? "",
        ],
        queryFn: () => fetchFileAsset(assetId ?? ""),
        enabled: Boolean(assetId?.trim()),
        staleTime: STALE_TIME,
    })
}
