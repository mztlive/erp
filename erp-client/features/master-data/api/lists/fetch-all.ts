import { fetchCompleteList } from "@/lib/collect-pages"

/** 拉取服务端匹配的全部页，失败时不展示被截断的目录。 */
export async function fetchAllPages<T>(
    path: string,
    query: Record<string, unknown> = {},
): Promise<T[]> {
    const page = await fetchCompleteList<T>(path, query, (item) =>
        JSON.stringify(item),
    )
    // 条目数组仍是返回值。响应里若有 empty_reason，挂在数组上，避免目录筛选丢掉 no_scope。
    if (page.empty_reason !== undefined) {
        Object.defineProperty(page.items, "empty_reason", {
            value: page.empty_reason,
            enumerable: false,
        })
    }
    return page.items
}
