import {
    fetchPersonScopes,
    personScopeDescription,
    type PersonDataScope,
} from "@/features/admin/api/person-data-scopes"
import type { BackendDataScope } from "./backend-types"
/** 只读展示单份人员配置，组合表达式不拆成额外授权来源。 */
export function scopeRow(scope: PersonDataScope): BackendDataScope {
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
        summary: personScopeDescription(scope),
    }
}
export async function personScopeRows(ids: string[]) {
    const views = await Promise.all(ids.map(fetchPersonScopes))
    const versions = new Set(views.map((v) => v.policy_version))
    if (versions.size > 1) throw new Error("范围配置已变化，请刷新后重试")
    return views.flatMap((view) => view.items.map(scopeRow))
}
