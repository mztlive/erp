/** 公司商品池对象中心：按 SKU 稳定身份匹配销售资格投影。 */

import type { SellableSkuDto } from "@/features/master-data/api/contracts"
import { mapSkuAsSellable } from "@/features/master-data/api/list-mappers"
import { fetchAllPages } from "@/features/master-data/api/lists"
import type { MasterDataCenterView } from "@/features/master-data/types"
import { baseCenter } from "./base"

export async function centerSellable(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const items = await fetchAllPages<SellableSkuDto>(
        "/admin/sellable-skus",
        {},
    )
    const item = items.find((candidate) => candidate.sku_id === stableId)
    if (!item) return null
    const row = mapSkuAsSellable(item)
    return baseCenter("sellable-items", row)
}
