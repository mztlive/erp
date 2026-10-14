/** 公司商品池服务端排序：表格列与接口排序字段的对应。 */

import type { SortingState } from "@tanstack/react-table"

export type SellableSortField =
    | "sku_no"
    | "dropship_price"
    | "bulk_price"
    | "market_price"
    | "bulk_min_quantity"
    | "supplier_count"

const COLUMN_SORT_FIELDS: Readonly<Record<string, SellableSortField>> = {
    price: "dropship_price",
    bulkPriceGross: "bulk_price",
    marketPrice: "market_price",
    bulkMinQuantity: "bulk_min_quantity",
    supplierCount: "supplier_count",
}

/** 表格排序转为接口排序；未排序或不支持的列按 SKU 编号升序（返回空）。 */
export function sellableSortFromTable(sorting: SortingState): {
    sellableSortBy?: SellableSortField
    sellableSortDesc?: boolean
} {
    const first = sorting[0]
    const field = first ? COLUMN_SORT_FIELDS[first.id] : undefined
    return field ? { sellableSortBy: field, sellableSortDesc: first.desc } : {}
}
