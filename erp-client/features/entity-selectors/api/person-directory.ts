import { apiGet } from "@/lib/api"

/** 路由固定的人员目录。客户端不能改成任意资源。 */
export type PersonDirectoryCategory = "sales" | "procurement" | "business"

export type PersonDirectoryItem = {
    id: string
    name: string
    account: string
    status: "active" | "suspended" | "archived"
    org_label?: string | null
}

export type PersonDirectoryPage = {
    items: PersonDirectoryItem[]
    total: number
    page: number
    page_size: number
    scope_version: string
    policy_version: number
    organization_version: number
    as_of: string
    empty_reason?: string | null
}

const PATHS: Record<PersonDirectoryCategory, string> = {
    business: "/admin/business-people",
    sales: "/admin/salespeople",
    procurement: "/admin/procurement-people",
}

export type PersonDirectorySearch = {
    category: PersonDirectoryCategory
    q?: string
    page: number
    orgUnitIds?: readonly string[]
    includeDescendants?: boolean
    scopeVersion?: string
}

/** 查询一页人员目录。组织条件只在调用方明确传入时发送。 */
export function fetchPersonDirectory(
    input: PersonDirectorySearch,
): Promise<PersonDirectoryPage> {
    const orgUnitIds = input.orgUnitIds?.filter(Boolean) ?? []
    return apiGet<PersonDirectoryPage>(PATHS[input.category], {
        q: input.q?.trim() || undefined,
        page: input.page,
        page_size: 20,
        org_unit_ids: orgUnitIds.length ? orgUnitIds.join(",") : undefined,
        include_descendants:
            orgUnitIds.length > 0 && input.includeDescendants
                ? true
                : undefined,
        scope_version:
            input.page > 1 ? input.scopeVersion || undefined : undefined,
    })
}

/** 回显已选人员。空 ID 不请求。 */
export function fetchSelectedPeople(
    category: PersonDirectoryCategory,
    ids: readonly string[],
): Promise<PersonDirectoryPage> {
    return apiGet<PersonDirectoryPage>(`${PATHS[category]}/selected`, {
        ids: ids.join(","),
    })
}

export function personDirectoryLabel(item: PersonDirectoryItem): string {
    const name = item.name.trim() || "未命名人员"
    const parts = [name, item.account.trim(), item.org_label?.trim()].filter(
        (part): part is string => Boolean(part),
    )
    const label = parts.join(" · ")
    return item.status === "active" ? label : `${label}（已停用）`
}
