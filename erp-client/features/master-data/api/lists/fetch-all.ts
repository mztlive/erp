import { fetchCompleteList } from "@/lib/collect-pages"

/** 拉取服务端匹配的全部页，失败时不展示被截断的目录。 */
export async function fetchAllPages<T>(
    path: string,
    query: Record<string, unknown> = {},
): Promise<T[]> {
    return (
        await fetchCompleteList<T>(path, query, (item) => JSON.stringify(item))
    ).items
}
