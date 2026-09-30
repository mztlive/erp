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
export type PersonDataScope = {
    created_at: number
    id: string
    version: number
    user_id: string
    resource: string
    action: string
    expression: {
        history_read: boolean
        alternatives: PersonScopeTerm[][]
        condition: PersonScopeTerm[] | null
    }
}
export type PersonScopeList = {
    items: PersonDataScope[]
    businesses: {
        resource: string
        actions: string[]
        dimensions: ScopeDimension[]
    }[]
    policy_version: number
}
export type PersonScopeInput = {
    confirmed: boolean
    resource: string
    actions: string[]
    dimension: ScopeDimension
    mode: "self" | "own_org" | "explicit" | "company"
    org_ids: string[]
    warehouse_ids: string[]
    settlement_ids: string[]
    include_descendants: boolean
}
const path = (userId: string) =>
    `/admin/person-data-scopes/${encodeURIComponent(userId)}`
export const fetchPersonScopes = (userId: string) =>
    apiGet<PersonScopeList>(path(userId))
export const savePersonScope = (
    userId: string,
    resource: string,
    actions: string[],
    terms: PersonScopeTerm[],
    version: number,
) =>
    apiPut<void>(path(userId), {
        resource,
        actions,
        terms,
        expected_policy_version: version,
    })
export function personScopeDescription(
    scope: PersonDataScope | undefined,
    labels: Map<string, string> = new Map(),
) {
    if (!scope) return "待设置"
    const expression = scope.expression
    if (expression.condition?.length === 0) return "无数据访问范围"
    if (expression.history_read) return "已迁移组合范围（含历史参与读取）"
    if (expression.alternatives.length === 1 && !expression.condition?.length) {
        if (expression.alternatives[0].length > 1) return "组合范围（多个条件）"
        return (
            expression.alternatives[0]
                .map((term) =>
                    term.scope_type === "company"
                        ? "公司范围"
                        : term.scope_type === "self_owned"
                          ? "本人负责"
                          : term.scope_type === "collaborative"
                            ? "协作参与"
                            : term.target_mode === "own_org"
                              ? `所属部门${term.include_descendants ? "及下级" : ""}`
                              : `${term.target_dimension === "warehouse" ? "指定仓库" : term.target_dimension === "settlement_party" ? "指定结算主体" : "指定部门"}：${term.scope_targets.map((id) => labels.get(id) ?? "已选目标").join("、")}${term.include_descendants ? "（含下级）" : ""}`,
                )
                .join("，且") || "无数据访问范围"
        )
    }
    return "已迁移的组合范围（保留原条件）"
}

/** 已保存的简单一致范围预填；不同/复杂范围必须重新明确选择。 */
export function personScopeDefaults(
    data: PersonScopeList,
    resource: string,
): PersonScopeInput {
    const business = data.businesses.find((b) => b.resource === resource)
    const rows =
        business?.actions.map((action) =>
            data.items.find(
                (s) => s.resource === resource && s.action === action,
            ),
        ) ?? []
    const base: PersonScopeInput = {
        resource,
        actions: business?.actions ?? [],
        dimension: business?.dimensions[0] ?? "internal_org",
        mode: business?.dimensions.includes("internal_org")
            ? "self"
            : "explicit",
        org_ids: [],
        warehouse_ids: [],
        settlement_ids: [],
        include_descendants: false,
        confirmed: rows.every((row) => !row),
    }
    const expressions = rows.map((row) => JSON.stringify(row?.expression))
    if (!rows.length || new Set(expressions).size !== 1 || !rows[0]) return base
    const expression = rows[0].expression
    if (
        expression.history_read ||
        expression.condition ||
        expression.alternatives.length !== 1
    )
        return base
    const terms = expression.alternatives[0]
    if (
        !terms.length ||
        terms.some((term) =>
            ["collaborative", "team"].includes(term.scope_type),
        )
    )
        return base
    if (terms.length !== 1) return base
    const term =
        terms.find((t) => t.target_dimension === "internal_org") ?? terms[0]
    const mode =
        term.scope_type === "company"
            ? "company"
            : term.scope_type === "self_owned"
              ? "self"
              : term.target_mode === "own_org"
                ? "own_org"
                : "explicit"
    return {
        ...base,
        confirmed: true,
        mode,
        dimension: term.target_dimension,
        include_descendants: term.include_descendants ?? false,
        org_ids: terms
            .filter((t) => t.target_dimension === "internal_org")
            .flatMap((t) => t.scope_targets),
        warehouse_ids: terms
            .filter((t) => t.target_dimension === "warehouse")
            .flatMap((t) => t.scope_targets),
        settlement_ids: terms
            .filter((t) => t.target_dimension === "settlement_party")
            .flatMap((t) => t.scope_targets),
    }
}
