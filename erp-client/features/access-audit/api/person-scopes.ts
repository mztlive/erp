import {
    fetchPersonScopes,
    personScopeDescription,
    type PersonDataScope,
    type PersonScopeList,
} from "@/features/admin/api/person-data-scopes"
import type { BackendDataScope } from "./backend-types"
/** 只读展示单份人员配置，组合表达式不拆成额外授权来源。 */
export function scopeRow(
    scope: PersonDataScope,
    defaultSelf = false,
): BackendDataScope {
    return {
        id: scope.id,
        version: scope.version,
        subject_type: "user",
        subject_id: scope.user_id,
        resource: scope.resource,
        actions: [scope.action],
        scope_type: "expression",
        scope_targets: [],
        enabled: true,
        created_at: scope.created_at,
        summary: personScopeDescription(scope, new Map(), defaultSelf),
    }
}

/** 已存配置与服务端登记的本人基础范围一起展示，不写入虚构配置。 */
export function scopeViewRows(
    userId: string,
    view: PersonScopeList,
): BackendDataScope[] {
    const rows = view.items.map((scope) => {
        const business = view.businesses.find(
            (item) => item.resource === scope.resource,
        )
        const active = business?.actions.includes(scope.action) ?? false
        const row = scopeRow(scope, business?.default_self ?? false)
        return {
            ...row,
            enabled: active,
            summary: active
                ? row.summary
                : `当前无操作权限；已存范围：${row.summary}`,
        }
    })
    for (const business of view.businesses) {
        if (!business.default_self) continue
        for (const action of business.actions) {
            if (
                view.items.some(
                    (scope) =>
                        scope.resource === business.resource &&
                        scope.action === action,
                )
            )
                continue
            rows.push({
                id: `baseline:${userId}:${business.resource}:${action}`,
                version: view.policy_version,
                subject_type: "user",
                subject_id: userId,
                resource: business.resource,
                actions: [action],
                scope_type: "self_owned",
                scope_targets: [],
                enabled: true,
                created_at: 0,
                summary: "本人负责的数据（基础范围）",
            })
        }
    }
    return rows
}
export async function personScopeRows(ids: string[]) {
    const views = await Promise.all(ids.map(fetchPersonScopes))
    const versions = new Set(views.map((v) => v.policy_version))
    if (versions.size > 1) throw new Error("范围配置已变化，请刷新后重试")
    return views.flatMap((view, index) => scopeViewRows(ids[index], view))
}
