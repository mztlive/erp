import type { ProductComboboxItem } from "@/components/business/entity-comboboxes"
import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

import { OPTION_PAGE_SIZE } from "./shared"
import type { EntitySearch } from "./types"

type CompanySkuDto = Readonly<{
    id: string
    sku_no: string
    specification_signature: string
    /** 当前 SKU 修订名称（公司审核后的 SKU 名称）。 */
    name?: string | null
    status: string
}>

export type CompanySkuComboboxItem = ProductComboboxItem

export async function searchCompanySkus(
    input: EntitySearch,
): Promise<readonly CompanySkuComboboxItem[]> {
    const page = await apiGet<Page<CompanySkuDto>>("/admin/skus", {
        q: input.query.trim() || undefined,
        status: "active",
        page: 1,
        page_size: OPTION_PAGE_SIZE,
        sort_by: "sku_no",
        sort_dir: "asc",
    })
    return page.items.map((row) => ({
        productId: row.id,
        sku: row.sku_no,
        name: row.name?.trim() || row.specification_signature || row.sku_no,
        statusLabel: "启用",
        statusTone: "success",
        description: row.specification_signature || undefined,
    }))
}
