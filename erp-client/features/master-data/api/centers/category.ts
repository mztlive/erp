/** 商品分类对象中心：全量拉取 + 上级路径与直接子分类解析。 */

import type { ProductCategoryDto } from "@/features/master-data/api/contracts"
import { mapCategoryRow } from "@/features/master-data/api/list-mappers"
import { fetchAllPages } from "@/features/master-data/api/lists"
import {
    asLifecycle,
    lifecycleLabel,
    lifecycleTone,
    productKindLabel,
    tsToIso,
} from "@/features/master-data/api/presentation"
import type { MasterDataCenterView } from "@/features/master-data/types"
import { baseCenter } from "./base"

function countDescendants(
    byParent: ReadonlyMap<string, readonly ProductCategoryDto[]>,
    rootId: string,
): number {
    const visited = new Set<string>([rootId])
    const queue: string[] = [rootId]
    let total = 0
    while (queue.length > 0) {
        const current = queue.shift()!
        const children = byParent.get(current) ?? []
        for (const child of children) {
            if (visited.has(child.id)) continue
            visited.add(child.id)
            total += 1
            queue.push(child.id)
        }
    }
    return total
}

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
    const byParent = new Map<string, ProductCategoryDto[]>()
    for (const item of items) {
        if (!item.parent_category_id) continue
        const bucket = byParent.get(item.parent_category_id) ?? []
        bucket.push(item)
        byParent.set(item.parent_category_id, bucket)
    }

    const ancestors: Array<{ stableId: string; name: string; code: string }> =
        []
    const seen = new Set<string>([dto.id])
    let cursor = dto.parent_category_id
    while (cursor && !seen.has(cursor)) {
        seen.add(cursor)
        const ancestor = byId.get(cursor)
        if (!ancestor) break
        ancestors.push({
            stableId: ancestor.id,
            name: ancestor.name,
            code: ancestor.category_code,
        })
        cursor = ancestor.parent_category_id
    }
    ancestors.reverse()

    const directChildren = (byParent.get(dto.id) ?? [])
        .toSorted((left, right) =>
            left.category_code.localeCompare(right.category_code, "zh-CN"),
        )
        .map((child) => {
            const lifecycle = asLifecycle(child.status)
            return {
                stableId: child.id,
                name: child.name,
                code: child.category_code,
                lifecycleStatusLabel: lifecycleLabel(lifecycle),
                lifecycleTone: lifecycleTone(lifecycle),
            }
        })
    const descendantCount = countDescendants(byParent, dto.id)

    const row = mapCategoryRow(dto)
    const parentName = dto.parent_category_id
        ? (byId.get(dto.parent_category_id)?.name ?? "（未知上级）")
        : "（根分类）"
    const facts = [
        { label: "上级分类", value: parentName },
        { label: "适用商品类型", value: productKindLabel(dto.product_kind) },
    ]
    return baseCenter(
        "categories",
        { ...row, keyFacts: facts },
        {
            resourceFacts: facts,
            categoryHierarchy: {
                ancestors,
                directChildren,
                descendantCount,
            },
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
