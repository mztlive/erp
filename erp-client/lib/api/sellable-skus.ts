import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type { SellableSkuDto } from "@/features/master-data/api/contracts"

export function fetchSellableSkuPage(params: Record<string, unknown>) {
    return apiGet<Page<SellableSkuDto>>("/admin/sellable-skus", params)
}
