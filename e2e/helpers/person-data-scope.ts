import { API_BASE, apiGet } from "./api"

export type PersonScopeTerm = {
    scope_type: "company" | "self_owned" | "organization" | "collaborative" | "team"
    target_dimension: string
    target_mode: "explicit" | "own_org" | null
    include_descendants: boolean | null
    scope_targets: string[]
}

export type PersonScopeView = {
    policy_version: number
    businesses: Array<{ resource: string; configurable_actions: string[]; dimensions: string[] }>
    items: Array<{
        resource: string
        action: string
        expression: {
            additive: boolean
            history_read: boolean
            alternatives: PersonScopeTerm[][]
            condition: PersonScopeTerm[] | null
        }
    }>
}

export type PersonScopeSave = {
    resource: string
    actions: string[]
    grants: Array<{ actions: string[]; terms: PersonScopeTerm[] }>
    replace_legacy: true
    expected_policy_version: number
}

function coversWarehouse(expression: PersonScopeView["items"][number]["expression"], warehouseId: string): boolean {
    if (expression.condition !== null) return false
    return expression.alternatives.some((terms) =>
        terms.length > 0 && terms.every((term) => term.target_dimension === "warehouse") &&
        terms.some((term) => term.scope_type === "company" || (
            term.scope_type === "organization" && term.target_mode === "explicit" &&
            term.scope_targets.includes(warehouseId)
        )),
    )
}

/** 按当前可配置动作追加仓库，保留被替换动作的每个既有分支，其余动作不参与替换。 */
export function appendWarehouseScope(
    view: PersonScopeView,
    resource: string,
    requestedActions: readonly string[],
    warehouseId: string,
): PersonScopeSave | null {
    const business = view.businesses.find((row) => row.resource === resource)
    if (!business?.dimensions.includes("warehouse")) {
        throw new Error(`人员没有可配置的仓库业务范围：${resource}`)
    }
    const eligible = [...new Set(requestedActions)].filter((action) => business.configurable_actions.includes(action))
    if (!eligible.length) throw new Error(`人员没有库存业务操作资格：${resource}`)
    const existing = view.items.filter((row) => row.resource === resource)
    const actions = eligible.filter((action) => !existing.some((row) =>
        row.action === action && coversWarehouse(row.expression, warehouseId),
    ))
    if (!actions.length) return null
    const grants: PersonScopeSave["grants"] = []
    for (const row of existing.filter((item) => actions.includes(item.action))) {
        if (row.expression.history_read || row.expression.condition !== null) {
            throw new Error(`无法无损追加旧库存范围：${resource}:${row.action} 包含历史读取或交集条件`)
        }
        for (const terms of row.expression.alternatives) {
            if (terms.length) grants.push({ actions: [row.action], terms })
        }
    }
    grants.push({
        actions,
        terms: [{
            scope_type: "organization",
            target_dimension: "warehouse",
            target_mode: "explicit",
            include_descendants: null,
            scope_targets: [warehouseId],
        }],
    })
    return { resource, actions, grants, replace_legacy: true, expected_policy_version: view.policy_version }
}

/** 保存空成功结果；范围写入接口允许标准信封 data 为 null。 */
async function savePersonScope(token: string, userId: string, body: PersonScopeSave): Promise<void> {
    const path = `/admin/person-data-scopes/${encodeURIComponent(userId)}`
    const response = await fetch(`${API_BASE}${path}`, {
        method: "PUT",
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(15_000),
    })
    const text = await response.text()
    const result = text ? JSON.parse(text) as { success?: boolean; errorMessage?: string } : null
    if (!response.ok || result?.success === false) {
        throw new Error(`API PUT ${path} 失败（HTTP ${response.status}）: ${result?.errorMessage ?? text.slice(0, 300)}`)
    }
}

/** 每次保存使用完整最新读取的策略版本；保存完成重读，不使用旧版并发写入。 */
export async function ensurePersonWarehouseScope(
    token: string,
    userId: string,
    grants: ReadonlyArray<{ resource: string; actions: readonly string[] }>,
    warehouseId: string,
): Promise<void> {
    const path = `/admin/person-data-scopes/${encodeURIComponent(userId)}`
    let view = await apiGet<PersonScopeView>(token, path)
    for (const grant of grants) {
        const body = appendWarehouseScope(view, grant.resource, grant.actions, warehouseId)
        if (!body) continue
        await savePersonScope(token, userId, body)
        view = await apiGet<PersonScopeView>(token, path)
    }
}
