/** 卡券类目列表适配：每 SKU 只保留最新扩展修订，避免多行重复。 */

import type {
    SkuDto,
    VoucherCategoryProfileDto,
} from "@/features/master-data/api/contracts"
import { mapVoucherRow } from "@/features/master-data/api/list-mappers"
import type {
    MasterDataListItem,
    MasterDataListQuery,
} from "@/features/master-data/types"
import { fetchAllPages } from "./fetch-all"

export async function listVoucherCategories(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    // 先按 SKU 取最新版本，再按当前状态筛选；历史启用版本不能使已停用类目复活。
    const profiles = await fetchAllPages<VoucherCategoryProfileDto>(
        "/admin/voucher-category-profiles",
        {},
    )
    if (profiles.length === 0) return []
    // 每个 SKU 只保留最新扩展修订，避免更新后列表出现多行。
    const latestBySku = new Map<string, VoucherCategoryProfileDto>()
    for (const profile of profiles) {
        const prev = latestBySku.get(profile.sku_id)
        if (!prev || profile.revision_no > prev.revision_no) {
            latestBySku.set(profile.sku_id, profile)
        }
    }
    const skus = await fetchAllPages<SkuDto>("/admin/skus", {}).catch(
        () => [] as SkuDto[],
    )
    const skuById = new Map(skus.map((s) => [s.id, s]))
    let rows = Array.from(latestBySku.values()).map((p) =>
        mapVoucherRow(p, skuById.get(p.sku_id)),
    )
    if (query.lifecycleStatus === "enabled") {
        rows = rows.filter((row) => row.lifecycleStatus === "ENABLED")
    } else if (query.lifecycleStatus === "disabled") {
        rows = rows.filter((row) => row.lifecycleStatus === "DISABLED")
    }
    if (query.q?.trim()) {
        const q = query.q.trim().toLowerCase()
        rows = rows.filter(
            (r) =>
                r.name.toLowerCase().includes(q) ||
                r.stableNo.toLowerCase().includes(q),
        )
    }
    return rows
}
