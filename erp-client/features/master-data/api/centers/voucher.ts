/** 卡券类目对象中心：兼容旧链接按 profile.id 匹配，取最新扩展修订。 */

import type {
    SkuDto,
    VoucherCategoryProfileDto,
} from "@/features/master-data/api/contracts"
import { mapVoucherRow } from "@/features/master-data/api/list-mappers"
import { fetchAllPages } from "@/features/master-data/api/lists"
import { asLifecycle, tsToIso } from "@/features/master-data/api/presentation"
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
    const skuId = profiles.find((p) => p.id === stableId)?.sku_id ?? stableId
    const matched = profiles.filter((p) => p.sku_id === skuId)
    if (matched.length === 0) return null
    const profile = matched.reduce((best, cur) =>
        cur.revision_no > best.revision_no ? cur : best,
    )
    const skus = await fetchAllPages<SkuDto>("/admin/skus", {})
    const sku = skus.find((s) => s.id === profile.sku_id)
    const row = mapVoucherRow(profile, sku)
    return baseCenter("voucher-categories", row, {
        revisionTimeline: [...matched]
            .sort((a, b) => b.revision_no - a.revision_no)
            .map((revision) => {
                const isCurrent = revision.id === profile.id
                return {
                    id: revision.id,
                    revisionNo: revision.revision_no,
                    revisionTiming: isCurrent ? "CURRENT" : "HISTORICAL",
                    timingLabel: isCurrent ? "当前版本" : "历史版本",
                    // 接口 name 来自当前商品修订，不是历史名称快照。
                    nameSnapshot: isCurrent ? row.name : "",
                    descriptionSnapshot: revision.description,
                    actor: "—",
                    effectiveFrom: tsToIso(revision.created_at),
                    changeReason: "—",
                    isCurrent,
                    lifecycleAtRevision: asLifecycle(revision.status),
                }
            }),
    })
}
