import { apiGet } from "@/lib/api"
import type { ListEnvelope } from "@/lib/collect-pages"
import { fetchSelectorList } from "@/lib/selector-list"

export type ObjectDirectoryKind = "settlement-parties" | "warehouse-directory"
export type ObjectDirectoryItem = {
    id: string
    code: string
    name: string
    status: string
}

/** 已选目录响应。版本字段保持响应原样，供同类缓存失效。 */
export type ObjectDirectorySelection = ListEnvelope & {
    items: readonly ObjectDirectoryItem[]
    total?: number
}

/** 读取完整匹配目录；后续页携带独立版本，范围或名称变化则整次失败。 */
export async function searchObjectDirectory(
    kind: ObjectDirectoryKind,
    query: string,
) {
    const result = await fetchSelectorList<ObjectDirectoryItem>(
        `/admin/${kind}`,
        { q: query.trim() || undefined },
    )
    return result
}

/** 已选回显仍执行目录资格与当前授权，不能读取旧列表快照。 */
export async function selectedObjectDirectory(
    kind: ObjectDirectoryKind,
    id: string,
): Promise<ObjectDirectorySelection | null> {
    if (!id) return null
    return apiGet<ObjectDirectorySelection>(`/admin/${kind}/selected`, {
        ids: id,
    })
}
