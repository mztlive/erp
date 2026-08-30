import type { MasterDataListItem } from "@/features/master-data/types"

/** 销售单选品确认后的 SKU 身份与建单所需带出字段。 */
export type SellableSkuPick = Readonly<{
    skuId: string
    skuRevisionId: string
    skuNo: string
    name: string
    specificationLabel: string
    baseUnit: string
    salesVisiblePriceGross: string
    mainImageAssetId?: string
    productKind?: string
}>

export function sellableItemToPick(row: MasterDataListItem): SellableSkuPick {
    const item = row.sellableItem
    return {
        skuId: row.stableId,
        skuRevisionId: row.currentRevisionId,
        skuNo: row.stableNo,
        name: row.name,
        specificationLabel: item?.specificationLabel ?? "",
        baseUnit: item?.baseUnit ?? "",
        salesVisiblePriceGross: item?.salesVisiblePriceGross ?? "",
        mainImageAssetId: item?.mainImageAssetId,
        productKind: row.productKind,
    }
}
