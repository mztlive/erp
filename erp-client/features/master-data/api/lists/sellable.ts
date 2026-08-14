/** 公司商品池（销售资格投影）列表适配。 */

import type { SellableSkuDto } from "@/features/master-data/api/contracts"
import { mapSkuAsSellable } from "@/features/master-data/api/list-mappers"
import type {
    MasterDataListItem,
    MasterDataListQuery,
} from "@/features/master-data/types"
import { fetchAllPages } from "./fetch-all"

export async function listSellableItems(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    // 公司商品池是资格投影，仅含当前可销售 SKU；启停/上架/供给覆盖不适用。
    const rows = await fetchAllPages<SellableSkuDto>("/admin/sellable-skus", {
        q: query.q || undefined,
        product_kind: query.productKind,
        category_id: query.productCategoryId,
        brand_id: query.productBrandId,
        supplier_id: query.productSupplierId,
        supply_region: query.supplyRegion,
        sales_price_min: query.productSalesPriceMin,
        sales_price_max: query.productSalesPriceMax,
        eligibility_as_of: query.eligibilityAsOf,
    })
    return rows.map(mapSkuAsSellable)
}
