import type { CustomerScope } from "@/features/customers/types"

export type DirectoryStatus = "active" | "disabled" | "all"

/** 表头排序列到服务端目录排序键。 */
export const SORT_COLUMN_TO_FIELD: Record<string, string> = {
    business: "updated_at",
}

export function parsePage(value: string | null): number {
    const page = Number.parseInt(value ?? "", 10)
    return Number.isFinite(page) && page > 0 ? page : 1
}

export function writeDirectoryUrl(
    pathname: string,
    params: {
        scope: CustomerScope
        status: DirectoryStatus
        q: string
        sort: string
        dir: "asc" | "desc"
        page: number
    },
): string {
    const sp = new URLSearchParams()
    if (params.scope !== "mine") sp.set("scope", params.scope)
    if (params.status !== "active") sp.set("status", params.status)
    if (params.q.trim()) sp.set("q", params.q.trim())
    if (params.sort && params.sort !== "business") sp.set("sort", params.sort)
    if (params.dir === "asc") sp.set("dir", "asc")
    if (params.page > 1) sp.set("page", String(params.page))
    const qs = sp.toString()
    return qs ? `${pathname}?${qs}` : pathname
}
