/** 卡券类目对象中心：兼容旧链接按 profile.id 匹配，取最新扩展修订。 */

import type {
    SkuDto,
    VoucherCategoryProfileDto,
} from "@/features/master-data/api/contracts"
import { mapVoucherRow } from "@/features/master-data/api/list-mappers"
import { fetchAllPages } from "@/features/master-data/api/lists"
import type { MasterDataCenterView } from "@/features/master-data/types"
import { baseCenter } from "./base"

export async function centerVoucher(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const profiles = await fetchAllPages<VoucherCategoryProfileDto>(
        "/admin/voucher-category-profiles",
        {},
    )
    // stableId 为 SKU 身份；兼容旧链接仍按 profile.id 查找。
    const matched = profiles.filter(
        (p) => p.sku_id === stableId || p.id === stableId,
    )
    if (matched.length === 0) return null
    const profile = matched.reduce((best, cur) =>
        cur.revision_no > best.revision_no ? cur : best,
    )
    const skus = await fetchAllPages<SkuDto>("/admin/skus", {})
    const sku = skus.find((s) => s.id === profile.sku_id)
    const row = mapVoucherRow(profile, sku)
    return baseCenter("voucher-categories", row)
}
