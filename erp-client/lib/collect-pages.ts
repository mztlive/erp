import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import { createApiError } from "@/lib/api/errors"

/** 读取完整匹配集合；重复页、总数变化或空的中间页使整次查询失败。 */
export async function fetchCompleteList<T>(
    path: string,
    query: Record<string, unknown> = {},
    keyOf: (item: T) => string = (item) => (item as { id: string }).id,
): Promise<{ items: T[]; total: number }> {
    const items = new Map<string, T>()
    let expectedTotal: number | undefined
    for (let page = 1; ; page += 1) {
        const result = await apiGet<Page<T>>(path, {
            ...query,
            page,
            page_size: 100,
        })
        expectedTotal ??= result.total
        const before = items.size
        for (const item of result.items) {
            const key = keyOf(item)
            if (!key)
                throw createApiError({
                    kind: "Parse",
                    message: "列表缺少记录标识，请重新查询。",
                })
            items.set(key, item)
        }
        if (
            !Number.isSafeInteger(result.total) ||
            result.total < 0 ||
            result.total !== expectedTotal ||
            items.size > result.total ||
            items.size - before !== result.items.length ||
            (items.size === before && items.size < result.total)
        ) {
            throw createApiError({
                kind: "Parse",
                message: "列表数据已变化，请重新查询。",
            })
        }
        if (items.size === result.total)
            return { items: [...items.values()], total: result.total }
    }
}
