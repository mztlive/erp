import type { PaginationState } from "@tanstack/react-table"

import type { SellableSupplyPresetSelection } from "@/features/master-data/hooks/use-sellable-list-filters"
import type { ProductKind } from "@/features/master-data/types"

export type SellablePickerAppliedFilters = Readonly<{
    q: string
    supplyPreset?: SellableSupplyPresetSelection
    productKind?: ProductKind
    productCategoryId?: string
    productBrandId?: string
    productSupplierId?: string
    supplyRegion?: string
    productSalesPriceMin?: string
    productSalesPriceMax?: string
}>

export type SellableSkuPickerListQuery = Readonly<{
    q?: string
    productKind?: ProductKind
    productCategoryId?: string
    productBrandId?: string
    productSupplierId?: string
    supplyRegion?: string
    productSalesPriceMin?: string
    productSalesPriceMax?: string
    maxSupplierCount?: number
    page: number
    pageSize: number
}>

/**
 * 把公司商品池筛选（含供应快捷视图）映射为分页查询。
 * 「全国可供」在未另选区域时落到可供区域=全国；「单一供应商」落到供应商数量上限 1。
 */
export function toSellablePickerListQuery(
    filters: SellablePickerAppliedFilters,
    pagination: PaginationState,
): SellableSkuPickerListQuery {
    const typedRegion = filters.supplyRegion?.trim() || undefined
    const supplyRegion =
        typedRegion ??
        (filters.supplyPreset === "nationwide" ? "全国" : undefined)
    return {
        q: filters.q.trim() || undefined,
        productKind: filters.productKind,
        productCategoryId: filters.productCategoryId,
        productBrandId: filters.productBrandId,
        productSupplierId: filters.productSupplierId,
        supplyRegion,
        productSalesPriceMin: filters.productSalesPriceMin,
        productSalesPriceMax: filters.productSalesPriceMax,
        maxSupplierCount:
            filters.supplyPreset === "single-supplier" ? 1 : undefined,
        page: pagination.pageIndex + 1,
        pageSize: pagination.pageSize,
    }
}
