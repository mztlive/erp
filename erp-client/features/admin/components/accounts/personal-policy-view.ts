import {
    AUTHORIZATION_POLICY_LABEL,
    personEffectiveDescription,
    type PersonScopeBusiness,
    type PersonScopeList,
} from "../../api/person-data-scopes"

export type PersonalPolicyView = {
    business: PersonScopeBusiness
    category: string
    summary: string
    groups: { key: string; actions: string[]; description: string }[]
}

/** 保留原始范围条件的差异，不按可能重名的展示文案合并权限。 */
export function personalPolicyView(
    business: PersonScopeBusiness,
    data: PersonScopeList,
    labels: Map<string, string>,
): PersonalPolicyView {
    const groups = new Map<string, PersonalPolicyView["groups"][number]>()
    for (const action of business.configurable_actions) {
        const scope = data.items.find(
            (item) =>
                item.resource === business.resource && item.action === action,
        )
        const key = JSON.stringify(scope?.expression) ?? "default"
        let description = personEffectiveDescription(scope, business, labels)
        if (description === "无数据访问范围，请添加授权")
            description =
                business.authorization_policy === "directory"
                    ? "未配置可见范围"
                    : "未配置数据范围"
        const group = groups.get(key) ?? { key, actions: [], description }
        group.actions.push(action)
        groups.set(key, group)
    }
    const rows = [...groups.values()]
    return {
        business,
        category: AUTHORIZATION_POLICY_LABEL[business.authorization_policy],
        summary:
            rows.length > 1
                ? "各操作范围不同"
                : (rows[0]?.description ??
                  AUTHORIZATION_POLICY_LABEL[business.authorization_policy]),
        groups: rows,
    }
}
