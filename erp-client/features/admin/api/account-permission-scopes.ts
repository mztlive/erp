import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

export type AccountPermissionScope = {
    id: string
    subject_type: "user" | "role"
    subject_id: string
    scope_type: string
    scope_targets: string[]
}

/** 分别读取账号与角色的数据范围配置，保留来源，不把多份配置合并成鉴权结果。 */
export async function fetchAccountPermissionScopes(
    accountId: string,
    roleIds: readonly string[],
) {
    const subjects = [
        { type: "user", id: accountId },
        ...[...new Set(roleIds)].map((id) => ({ type: "role", id })),
    ]
    const results = await Promise.all(
        subjects.map(async (subject) => {
            const rows: AccountPermissionScope[] = []
            let page = 1
            while (true) {
                const result = await apiGet<Page<AccountPermissionScope>>(
                    "/admin/data-scopes",
                    {
                        page,
                        page_size: 50,
                        subject_type: subject.type,
                        subject_id: subject.id,
                    },
                )
                rows.push(...result.items)
                if (rows.length >= result.total) return rows
                if (result.items.length === 0)
                    throw new Error("数据范围未完整返回，请重试。")
                page += 1
            }
        }),
    )
    return results.flat()
}
