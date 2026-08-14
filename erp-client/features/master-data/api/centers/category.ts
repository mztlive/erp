/** 商品分类对象中心：全量拉取 + 上级分类名称解析。 */

import type { ProductCategoryDto } from "@/features/master-data/api/contracts"
import { mapCategoryRow } from "@/features/master-data/api/list-mappers"
import { fetchAllPages } from "@/features/master-data/api/lists"
import { productKindLabel, tsToIso } from "@/features/master-data/api/presentation"
import type { MasterDataCenterView } from "@/features/master-data/types"
import { baseCenter } from "./base"

export async function centerCategory(
    stableId: string,
): Promise<MasterDataCenterView | null> {
    const items = await fetchAllPages<ProductCategoryDto>(
        "/admin/product-categories",
        {},
    )
    const dto = items.find((c) => c.id === stableId)
    if (!dto) return null
    const byId = new Map(items.map((c) => [c.id, c]))
    const row = mapCategoryRow(dto)
    const parentName = dto.parent_category_id
        ? (byId.get(dto.parent_category_id)?.name ?? "（未知上级）")
        : "（根分类）"
    const facts = [
        { label: "分类代码", value: dto.category_code },
        { label: "上级分类", value: parentName },
        { label: "适用商品类型", value: productKindLabel(dto.product_kind) },
    ]
    return baseCenter(
        "categories",
        { ...row, keyFacts: facts },
        {
            resourceFacts: facts,
            currentRevision: {
                revisionId: dto.id,
                revisionNo: dto.version,
                name: dto.name,
                effectiveFrom: tsToIso(dto.created_at).slice(0, 10),
                changeReason: "—",
                actor: "—",
                fields: facts,
            },
        },
    )
}
