import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type { SourceSku } from "../types"

/** 候选复用公司商品池资格与权限，不使用后台 SKU 管理列表。 */
export const fetchSourceSkus = (q: string, page: number) =>
    apiGet<Page<SourceSku>>("/admin/sellable-skus", {
        q: q.trim() || undefined,
        page,
        page_size: 10,
    })
