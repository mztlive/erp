import { fetchCompleteList } from "@/lib/collect-pages"
import type { ListEnvelope } from "@/lib/collect-pages"

export type SelectorPage<T> = ListEnvelope & { items: readonly T[]; total: number }

/** 对象自身目录完整查询，最多10000项；跨页版本变化或超限整体失败。 */
export function fetchSelectorList<T extends { id: string }>(
    path: string,
    query: Record<string, unknown> = {},
) {
    return fetchCompleteList<T>(path, query, (item) => item.id, 10_000)
}
