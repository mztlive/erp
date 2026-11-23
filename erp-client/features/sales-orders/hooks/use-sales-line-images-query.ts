"use client"

import { useQueries } from "@tanstack/react-query"

import { sellableItemToPick } from "../lib/sellable-sku-pick"
import { listSellableItemsPage } from "@/features/master-data/api"

/** 按明细 SKU 批量读取主图、参考价和计量单位名称。 */
export function useSalesLineImagesQuery(skuIds: readonly string[]) {
    const ids = [...new Set(skuIds.filter(Boolean))].sort()
    const batches: string[][] = []
    for (let offset = 0; offset < ids.length; offset += 50) {
        batches.push(ids.slice(offset, offset + 50))
    }
    const queries = useQueries({
        queries: batches.map((batch) => ({
            queryKey: ["sales-orders", "line-images", batch],
            queryFn: () =>
                listSellableItemsPage({
                    resource: "sellable-items",
                    skuIds: batch,
                    page: 1,
                    pageSize: 50,
                }),
            staleTime: 5 * 60 * 1000,
        })),
    })
    const skuRows = queries.flatMap((query) => query.data?.rows ?? [])
    const unitNames = new Map<string, string>()
    for (const { sellableItem } of skuRows) {
        if (sellableItem?.baseUnitCode && sellableItem.baseUnitName) {
            unitNames.set(sellableItem.baseUnitCode, sellableItem.baseUnitName)
        }
    }

    return {
        unitNames,
        references: new Map(
            skuRows.map(
                (row) =>
                    [row.currentRevisionId, sellableItemToPick(row)] as const,
            ),
        ),
        images: new Map(
            skuRows.map(
                (row) =>
                    [row.stableId, row.sellableItem?.mainImageAssetId] as const,
            ),
        ),
        error: queries.find((query) => query.isError)?.error,
        refetch: () => Promise.all(queries.map((query) => query.refetch())),
    }
}

export type SalesLineSkuDetails = ReturnType<typeof useSalesLineImagesQuery>
