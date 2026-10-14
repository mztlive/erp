import type { PaginationState } from "@tanstack/react-table"

import type { PriceFilterKind } from "@/lib/price-filter"
import type { ProductKind } from "@/features/master-data/types"

export type SellablePickerAppliedFilters = Readonly<{
    q: string
    productKind?: ProductKind
    productCategoryId?: string
    productBrandId?: string
    productPriceKind?: PriceFilterKind
    productSalesPriceMin?: string
    productSalesPriceMax?: string
}>

export type SellableSkuPickerListQuery = Readonly<{
    q?: string
    productKind?: ProductKind
    productCategoryId?: string
    productBrandId?: string
    productPriceKind?: PriceFilterKind
    productSalesPriceMin?: string
    productSalesPriceMax?: string
    page: number
    pageSize: number
}>

/** 将与公司商品池一致的已生效条件映射为弹窗分页查询。 */
export function toSellablePickerListQuery(
    filters: SellablePickerAppliedFilters,
    pagination: PaginationState,
): SellableSkuPickerListQuery {
    return {
        q: filters.q.trim() || undefined,
        productKind: filters.productKind,
        productCategoryId: filters.productCategoryId,
        productBrandId: filters.productBrandId,
        productPriceKind: filters.productPriceKind,
        productSalesPriceMin: filters.productSalesPriceMin,
        productSalesPriceMax: filters.productSalesPriceMax,
        page: pagination.pageIndex + 1,
        pageSize: pagination.pageSize,
    }
}
