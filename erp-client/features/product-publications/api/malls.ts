/**
 * W22 商品发布 · 商城选项（运行时从 source-systems 填充）。
 */

import { apiGet, type Page } from "@/lib/api"

import type { SourceSystem } from "@/features/product-publications/api/wire-types"

/** 商城选项：运行时从 source-systems 填充；初始为空，列表接口内补齐。 */
export let MALLS: Array<{ id: string; name: string }> = []

export async function loadMalls(): Promise<Array<{ id: string; name: string }>> {
    try {
        const page = await apiGet<Page<SourceSystem>>("/admin/source-systems", {
            page: 1,
            page_size: 100,
            system_type: "MALL",
        })
        const list = page.items.map((s) => ({ id: s.id, name: s.name }))
        MALLS = list
        return list
    } catch {
        return MALLS
    }
}

export function mallName(
    malls: Array<{ id: string; name: string }>,
    id: string,
): string {
    return malls.find((m) => m.id === id)?.name ?? id
}
