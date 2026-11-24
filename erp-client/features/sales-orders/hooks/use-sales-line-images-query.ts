"use client"

import { sellableItemToPick } from "../lib/sellable-sku-pick"
import { mapSkuAsSellable } from "@/features/master-data/api/list-mappers"
import {
    indexProductUnitNames,
    useProductUnitDirectoryQuery,
    useSkuPresentationQueries,
} from "@/hooks/use-product-unit-names"

/** 按明细 SKU 批量读取主图、参考价和计量单位名称。 */
export function useSalesLineImagesQuery(skuIds: readonly string[]) {
    const { rows: skuRows, error, refetch } = useSkuPresentationQueries(skuIds)
    const directory = useProductUnitDirectoryQuery(skuIds.length > 0)
    const { names: unitNames } = indexProductUnitNames(skuRows, directory)

    return {
        unitNames,
        references: new Map(
            skuRows.map(
                (row) =>
                    [
                        row.sku_revision_id,
                        sellableItemToPick(mapSkuAsSellable(row)),
                    ] as const,
            ),
        ),
        images: new Map(
            skuRows.map(
                (row) =>
                    [row.sku_id, row.main_image_asset_id ?? undefined] as const,
            ),
        ),
        error,
        refetch,
    }
}

export type SalesLineSkuDetails = ReturnType<typeof useSalesLineImagesQuery>
