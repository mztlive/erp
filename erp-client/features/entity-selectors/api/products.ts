import type { ProductComboboxItem } from "@/components/business/entity-comboboxes"
import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

import { OPTION_PAGE_SIZE } from "./shared"
import type { SellableSkuSearch } from "./types"

type SellableSkuDto = Readonly<{
    sku_id: string
    sku_revision_id: string
    sku_no: string
    product_kind: string
    name: string
    specification?: string | null
    base_unit_code?: string | null
    base_unit_name?: string | null
    supplier_count: number
    sales_visible_price_gross?: string | null
}>

function productItem(row: SellableSkuDto): ProductComboboxItem & {
    revisionId: string
    salesVisiblePriceGross?: string
} {
    const baseUnit = row.base_unit_name ?? row.base_unit_code ?? undefined
    const salesVisiblePriceGross = row.sales_visible_price_gross?.trim()
    return {
        productId: row.sku_id,
        revisionId: row.sku_revision_id,
        sku: row.sku_no,
        name: row.name,
        statusLabel: "可销售",
        statusTone: "success",
        baseUnit,
        salesVisiblePriceGross: salesVisiblePriceGross || undefined,
        description: [
            row.specification?.trim(),
            baseUnit ? `单位 ${baseUnit}` : null,
            `有效供应商 ${row.supplier_count}`,
        ]
            .filter(Boolean)
            .join(" · "),
    }
}

export type SellableSkuComboboxItem = ReturnType<typeof productItem>

export async function searchSellableSkus(
    input: SellableSkuSearch,
): Promise<readonly SellableSkuComboboxItem[]> {
    const page = await apiGet<Page<SellableSkuDto>>("/admin/sellable-skus", {
        q: input.query.trim() || undefined,
        product_kind: input.productKind || undefined,
        page: 1,
        page_size: OPTION_PAGE_SIZE,
    })
    return page.items
        .filter(
            (row) =>
                !input.excludeProductKind ||
                row.product_kind.toUpperCase() !==
                    input.excludeProductKind.toUpperCase(),
        )
        .map(productItem)
}
