import { actionLabel } from "@/lib/permission-catalog"
import { apiGet, apiPut } from "@/lib/api"
import type { ScopeDimension } from "@/features/organization/types"

export type PersonScopeTerm = {
    scope_type:
        | "company"
        | "self_owned"
        | "organization"
        | "collaborative"
        | "team"
    target_dimension: ScopeDimension
    target_mode: "explicit" | "own_org" | null
    include_descendants: boolean | null
    scope_targets: string[]
}
export type PersonScopeExpression = {
    alternatives: PersonScopeTerm[][]
}
export type PersonDataScope = {
    created_at: number
    id: string
    version: number
    user_id: string
    resource: string
    action: string
    expression: PersonScopeExpression
}
export type PersonScopeBusiness = {
    configurable_actions: string[]
    policy_description: string
    resource: string
    actions: string[]
    dimensions: ScopeDimension[]
    default_self: boolean
    default_self_actions: string[]
}
export type PersonScopeList = {
    items: PersonDataScope[]
    retired_items: { resource: string; action: string; reason: string }[]
    businesses: PersonScopeBusiness[]
    policy_version: number
}
export type PersonScopeGrant = {
    /** 未完成的行内字段随草稿保留，提交时不发送。 */
    editor?: PersonGrantEditorInput
    key: string
    actions: string[]
    terms: PersonScopeTerm[]
}
export type PersonGrantEditorInput = {
    key: string | null
    actions: string[]
    dimension: ScopeDimension
    mode: "self" | "own_org" | "explicit" | "company"
    org_ids: string[]
    warehouse_ids: string[]
    settlement_ids: string[]
    include_descendants: boolean
}
export type PersonScopeInput = {
    resource: string
    actions: string[]
    grants: PersonScopeGrant[]
    editor: PersonGrantEditorInput | null
}
const path = (userId: string) =>
    `/admin/person-data-scopes/${encodeURIComponent(userId)}`
export async function fetchPersonScopes(
    userId: string,
): Promise<PersonScopeList> {
    const view = await apiGet<PersonScopeList>(path(userId))
    if (
        !Array.isArray(view.retired_items) ||
        !Array.isArray(view.businesses) ||
        view.businesses.some(
            (business) =>
                !Array.isArray(business.actions) ||
                !Array.isArray(business.configurable_actions) ||
                !Array.isArray(business.default_self_actions) ||
                !Array.isArray(business.dimensions) ||
                business.configurable_actions.some(
                    (action) => !business.actions.includes(action),
                ) ||
                business.default_self_actions.some(
                    (action) => !business.configurable_actions.includes(action),
                ) ||
                typeof business.policy_description !== "string",
        )
    )
        throw new Error("授权规则暂未完整返回，请在服务更新后刷新重试")
    return view
}
export const savePersonScope = (
    userId: string,
    value: PersonScopeInput,
    version: number,
) =>
    apiPut<void>(path(userId), {
        resource: value.resource,
        actions: value.actions,
        grants: value.grants.map(({ actions, terms }) => ({ actions, terms })),
        expected_policy_version: version,
    })

/** 条件顺序和目标顺序不改变授权身份，用于合并跨操作的相同授权。 */
export function personGrantKey(terms: PersonScopeTerm[]) {
    return JSON.stringify(
        terms
            .map((term) => ({
                scope_type: term.scope_type,
                target_dimension: term.target_dimension,
                target_mode: term.target_mode,
                include_descendants: term.include_descendants,
                scope_targets: [...term.scope_targets].sort(),
            }))
            .sort((left, right) =>
                JSON.stringify(left).localeCompare(JSON.stringify(right)),
            ),
    )
}

export function personTermsDescription(
    terms: PersonScopeTerm[],
    labels: Map<string, string> = new Map(),
) {
    if (terms.some((term) => term.scope_type === "company")) return "公司范围"
    const groups = new Map<ScopeDimension, string[]>()
    for (const term of terms) {
        let description: string
        if (term.scope_type === "self_owned") description = "本人负责的数据"
        else if (term.scope_type === "collaborative")
            description = "协作参与的数据"
        else if (term.target_mode === "own_org")
            description = `所属部门${term.include_descendants ? "及下级" : ""}的数据`
        else {
            const dimension =
                term.target_dimension === "warehouse"
                    ? "仓库"
                    : term.target_dimension === "settlement_party"
                      ? "结算主体"
                      : "部门"
            const targets = term.scope_targets
                .map((id) => labels.get(id) ?? `名称待确认的${dimension}`)
                .join("、")
            description = `指定${dimension}：${targets || "无目标"}${term.include_descendants ? "（含下级）" : ""}`
        }
        groups.set(term.target_dimension, [
            ...(groups.get(term.target_dimension) ?? []),
            description,
        ])
    }
    return (
        [...groups.values()]
            .map((descriptions) =>
                descriptions.length > 1
                    ? `（${descriptions.join("，或 ")}）`
                    : descriptions[0],
            )
            .join("，并且同时属于") || "无数据访问范围"
    )
}

export function personScopeDescription(
    scope: PersonDataScope | undefined,
    labels: Map<string, string> = new Map(),
    defaultSelf = false,
): string {
    if (!scope) return defaultSelf ? "本人负责的数据（默认）" : "无数据访问范围"
    return personEffectiveDescription(
        scope,
        {
            resource: scope.resource,
            configurable_actions: [scope.action],
            policy_description: "",
            actions: [scope.action],
            dimensions: [],
            default_self: defaultSelf,
            default_self_actions: defaultSelf ? [scope.action] : [],
        },
        labels,
    )
}

/** 本人基础范围使用服务端逐动作配置，资源汇总不参与单项判断。 */
export function personHasDefaultSelf(
    business: PersonScopeBusiness,
    action: string,
): boolean {
    return business.default_self_actions.includes(action)
}

/** 同一资源可同时包含默认本人读取和必须显式配置的管理动作。 */
export function personDefaultScopeDescription(
    business: PersonScopeBusiness,
): string {
    const defaults = business.configurable_actions.filter((action) =>
        personHasDefaultSelf(business, action),
    )
    if (!defaults.length) return "没有默认本人范围，请添加授权。"
    if (defaults.length === business.configurable_actions.length)
        return "默认包含本人负责的数据；扩大范围需添加授权。"
    return `${defaults.map(actionLabel).join("、")}默认包含本人负责的数据；其余操作需显式添加范围。`
}

export function personEffectiveDescription(
    scope: PersonDataScope | undefined,
    business: PersonScopeBusiness,
    labels: Map<string, string> = new Map(),
    action = scope?.action,
): string {
    if (
        !business.configurable_actions.length ||
        (scope && !business.configurable_actions.includes(scope.action))
    )
        return business.policy_description
    const defaultSelf = action
        ? personHasDefaultSelf(business, action)
        : business.default_self
    const alternatives = scope?.expression.alternatives ?? []
    if (
        alternatives.some((terms) =>
            terms.some((term) => term.scope_type === "company"),
        )
    )
        return defaultSelf
            ? "公司范围（已覆盖本人及其他追加范围）"
            : "公司范围（已覆盖其他追加范围）"
    const descriptions = alternatives.map((terms) =>
        personTermsDescription(terms, labels),
    )
    if (defaultSelf) descriptions.unshift("本人负责的数据")
    return descriptions.join("；或 ") || "无数据访问范围，请添加授权"
}

export function personScopeDefaults(
    data: PersonScopeList,
    resource: string,
): PersonScopeInput {
    const business = data.businesses.find((item) => item.resource === resource)
    const grants = new Map<string, PersonScopeGrant>()
    for (const row of data.items.filter(
        (item) =>
            item.resource === resource &&
            business?.configurable_actions.includes(item.action),
    )) {
        for (const terms of row.expression.alternatives) {
            if (
                !terms.length ||
                (business &&
                    personHasDefaultSelf(business, row.action) &&
                    terms.length === 1 &&
                    terms[0].scope_type === "self_owned")
            )
                continue
            const key = personGrantKey(terms)
            const grant = grants.get(key) ?? { key, terms, actions: [] }
            if (!grant.actions.includes(row.action))
                grant.actions.push(row.action)
            grants.set(key, grant)
        }
    }
    return {
        resource,
        actions: business?.configurable_actions ?? [],
        grants: [...grants.values()],
        editor: null,
    }
}
