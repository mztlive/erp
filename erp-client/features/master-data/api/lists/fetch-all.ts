/** 分页全量拉取：翻页直到取完（上限 50 页），供字典与列表适配使用。 */

import { apiGet } from "@/lib/api"
import type { BackendPage } from "@/features/master-data/api/contracts"
import { LIST_PAGE_SIZE } from "@/features/master-data/api/presentation"

export async function fetchAllPages<T>(
    path: string,
    query: Record<string, unknown> = {},
): Promise<T[]> {
    const items: T[] = []
    let page = 1
    let total = Number.POSITIVE_INFINITY
    while (items.length < total) {
        const result = await apiGet<BackendPage<T>>(path, {
            ...query,
            page,
            page_size: LIST_PAGE_SIZE,
        })
        items.push(...result.items)
        total = result.total
        if (result.items.length === 0) break
        page += 1
        if (page > 50) break
    }
    return items
}
