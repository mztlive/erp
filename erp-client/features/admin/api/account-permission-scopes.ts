import { apiGet } from "@/lib/api"
import { createApiError } from "@/lib/api/errors"
import type { Page } from "@/lib/api/paging"

export type { ScopeRule as AccountPermissionScope } from "@/features/organization/lib/scope-description"
import type { ScopeRule as AccountPermissionScope } from "@/features/organization/lib/scope-description"

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
            let version: string | undefined
            let expectedTotal: number | undefined
            const seen = new Set<string>()
            while (true) {
                const result = await apiGet<
                    Page<AccountPermissionScope> & { scope_version?: string }
                >("/admin/data-scopes", {
                    page,
                    page_size: 50,
                    ...(version ? { scope_version: version } : {}),
                    subject_type: subject.type,
                    subject_id: subject.id,
                })
                if (version && result.scope_version !== version)
                    throw createApiError({
                        kind: "Http",
                        status: 409,
                        code: "DATA_SCOPE_CHANGED",
                        message: "数据范围已变化，请重新查询。",
                    })
                version ??= result.scope_version
                expectedTotal ??= result.total
                if (
                    !Number.isSafeInteger(result.total) ||
                    result.total < 0 ||
                    result.total > 10000 ||
                    expectedTotal !== result.total ||
                    result.items.some((row) => seen.has(row.id)) ||
                    new Set(result.items.map((row) => row.id)).size !==
                        result.items.length
                )
                    throw new Error(
                        "数据范围已变化或超过读取上限，请重新查询。",
                    )
                result.items.forEach((row) => seen.add(row.id))
                rows.push(...result.items)
                if (rows.length === result.total) return { rows, version }
                if (rows.length > result.total)
                    throw new Error("数据范围返回数量不一致，请重试。")
                if (result.items.length === 0)
                    throw new Error("数据范围未完整返回，请重试。")
                page += 1
            }
        }),
    )
    const versions = new Set(
        results.map((result) => result.version).filter(Boolean),
    )
    if (versions.size > 1)
        throw createApiError({
            kind: "Http",
            status: 409,
            code: "DATA_SCOPE_CHANGED",
            message: "角色与个人范围版本不一致，请重新查询。",
        })
    return results.flatMap((result) => result.rows)
}
