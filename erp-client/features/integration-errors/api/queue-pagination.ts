import type { Page } from "@/lib/api"
import { createApiError } from "@/lib/api/errors"

/** 读取全部匹配页；中途失败或分页不再前进时明确失败，禁止展示截断结果。 */
export async function collectQueuePages<T extends { id: string }>(
    load: (page: number) => Promise<Page<T>>,
): Promise<T[]> {
    const items = new Map<string, T>()
    let expectedTotal: number | undefined
    for (let page = 1; ; page += 1) {
        const result = await load(page)
        const before = items.size
        expectedTotal ??= result.total
        for (const item of result.items) items.set(item.id, item)
        if (
            result.total !== expectedTotal ||
            items.size - before !== result.items.length ||
            (items.size === before && items.size < result.total)
        ) {
            throw createApiError({
                kind: "Parse",
                message: "队列数据已变化，请重新查询。",
            })
        }
        if (items.size >= result.total) return [...items.values()]
    }
}
