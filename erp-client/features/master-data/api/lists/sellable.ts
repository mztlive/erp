/** 公司商品池（销售资格投影）列表适配。 */

import type { SellableSkuDto } from "@/features/master-data/api/contracts"
import { mapSkuAsSellable } from "@/features/master-data/api/list-mappers"
import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type {
    MasterDataListItem,
    MasterDataListQuery,
} from "@/features/master-data/types"
import { fetchAllPages } from "./fetch-all"

function sellableListParams(
    query: MasterDataListQuery & { maxSupplierCount?: number },
) {
    return {
        q: query.q || undefined,
        product_kind: query.productKind,
        category_id: query.productCategoryId,
        brand_id: query.productBrandId,
        supplier_id: query.productSupplierId,
        supply_region: query.supplyRegion,
        sales_price_min: query.productSalesPriceMin,
        sales_price_max: query.productSalesPriceMax,
        eligibility_as_of: query.eligibilityAsOf,
        max_supplier_count: query.maxSupplierCount,
    }
}

export async function listSellableItems(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    // 公司商品池是资格投影，仅含当前可销售 SKU；启停/上架/供给覆盖不适用。
    const rows = await fetchAllPages<SellableSkuDto>(
        "/admin/sellable-skus",
        sellableListParams(query),
    )
    return rows.map(mapSkuAsSellable)
}

/** 选品 Dialog 使用服务端分页，避免把几万 SKU 一次拉进浏览器。 */
export async function listSellableItemsPage(
    query: MasterDataListQuery & {
        page: number
        pageSize: number
        maxSupplierCount?: number
    },
): Promise<{ rows: MasterDataListItem[]; total: number }> {
    const page = await apiGet<Page<SellableSkuDto>>("/admin/sellable-skus", {
        ...sellableListParams(query),
        page: query.page,
        page_size: query.pageSize,
    })
    return {
        rows: page.items.map(mapSkuAsSellable),
        total: page.total,
    }
}
