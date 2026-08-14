/** 分类 / 品牌 / 计量单位三类字典的列表查询适配。 */

import type {
    ProductBrandDto,
    ProductCategoryDto,
    UnitOfMeasureDto,
} from "@/features/master-data/api/contracts"
import {
    mapBrandRow,
    mapCategoryRow,
    mapUnitOfMeasureRow,
} from "@/features/master-data/api/list-mappers"
import { productKindLabel } from "@/features/master-data/api/presentation"
import type {
    MasterDataListItem,
    MasterDataListQuery,
} from "@/features/master-data/types"
import { fetchAllPages } from "./fetch-all"

export async function listCategories(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    const items = await fetchAllPages<ProductCategoryDto>(
        "/admin/product-categories",
        {
            status,
            name: query.q || undefined,
        },
    )
    // Resolve parent names for keyFacts
    const byId = new Map(items.map((c) => [c.id, c]))
    return items.map((dto) => {
        const row = mapCategoryRow(dto)
        if (dto.parent_category_id) {
            const parent = byId.get(dto.parent_category_id)
            return {
                ...row,
                keyFacts: [
                    { label: "分类代码", value: dto.category_code },
                    {
                        label: "上级分类",
                        value: parent?.name ?? "（未知上级）",
                    },
                    {
                        label: "适用商品类型",
                        value: productKindLabel(dto.product_kind),
                    },
                ],
            }
        }
        return row
    })
}

export async function listBrands(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    const items = await fetchAllPages<ProductBrandDto>(
        "/admin/product-brands",
        {
            status,
            name: query.q || undefined,
        },
    )
    return items.map(mapBrandRow)
}

export async function listUnitOfMeasures(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    // 仅按 status 拉全量（字典体量小）；代码/名称/符号在本地模糊匹配
    const items = await fetchAllPages<UnitOfMeasureDto>(
        "/admin/unit-of-measures",
        { status },
    )
    const rows = items.map(mapUnitOfMeasureRow)
    const q = query.q?.trim().toLowerCase()
    if (!q) return rows
    return rows.filter((row) => {
        const hay = [
            row.stableNo,
            row.name,
            ...row.keyFacts.map((f) => f.value),
        ]
            .join(" ")
            .toLowerCase()
        return hay.includes(q)
    })
}
