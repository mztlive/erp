import { expect, test } from "@playwright/test"

import { rememberApiToken } from "./api"
import { ensureWarehouseStockScope } from "./inventory"
import {
    appendWarehouseScope,
    ensurePersonWarehouseScope,
    type PersonScopeSave,
    type PersonScopeTerm,
    type PersonScopeView,
} from "./person-data-scope"

function warehouseTerm(id: string): PersonScopeTerm {
    return {
        scope_type: "organization",
        target_dimension: "warehouse",
        target_mode: "explicit",
        include_descendants: null,
        scope_targets: [id],
    }
}

function scope(resource: string, action: string, terms: PersonScopeTerm[]): PersonScopeView["items"][number] {
    return { resource, action, expression: { additive: true, history_read: false, alternatives: [terms], condition: null } }
}

function view(items: PersonScopeView["items"] = []): PersonScopeView {
    return {
        policy_version: 11,
        items,
        businesses: [
            { resource: "stock_balance", configurable_actions: ["list", "detail"], dimensions: ["warehouse"] },
            { resource: "stock_movement", configurable_actions: ["list"], dimensions: ["warehouse"] },
            { resource: "stock_reservation", configurable_actions: ["list"], dimensions: ["warehouse"] },
            { resource: "stock_adjustment", configurable_actions: ["list", "detail", "create", "update", "submit"], dimensions: ["warehouse"] },
        ],
    }
}

test("追加仓库保留被替换动作的既有分支，已覆盖动作不参与替换", () => {
    const original = view([
        scope("stock_balance", "list", [warehouseTerm("previous-warehouse")]),
        scope("stock_balance", "detail", [{ ...warehouseTerm(""), scope_type: "company", target_mode: null, scope_targets: [] }]),
    ])
    const untouched = structuredClone(original)
    const body = appendWarehouseScope(original, "stock_balance", ["list", "detail"], "new-warehouse")
    expect(body).toEqual({
        resource: "stock_balance",
        actions: ["list"],
        grants: [
            { actions: ["list"], terms: [warehouseTerm("previous-warehouse")] },
            { actions: ["list"], terms: [warehouseTerm("new-warehouse")] },
        ],
        replace_legacy: true,
        expected_policy_version: 11,
    })
    expect(original).toEqual(untouched)
})

test("只保存服务器返回的可配置动作，已覆盖仓库不重复保存", () => {
    const current = view()
    current.businesses[0].configurable_actions = ["list"]
    expect(appendWarehouseScope(current, "stock_balance", ["list", "detail"], "warehouse")?.actions).toEqual(["list"])
    current.items.push(scope("stock_balance", "list", [warehouseTerm("warehouse")]))
    expect(appendWarehouseScope(current, "stock_balance", ["list", "detail"], "warehouse")).toBeNull()
})

test("不能通过丢弃旧交集或历史读取来追加仓库", () => {
    const current = view([scope("stock_balance", "list", [warehouseTerm("previous")])])
    current.items[0].expression.condition = [warehouseTerm("limit")]
    expect(() => appendWarehouseScope(current, "stock_balance", ["list"], "new")).toThrow("无法无损追加")
    current.items[0].expression.condition = null
    current.items[0].expression.history_read = true
    expect(() => appendWarehouseScope(current, "stock_balance", ["list"], "new")).toThrow("无法无损追加")
})

test("顺序 PUT 每次使用重读后的策略版本，允许 data:null 的成功结果", async () => {
    const originalFetch = globalThis.fetch
    const current = view()
    const calls: string[] = []
    const saved: PersonScopeSave[] = []
    globalThis.fetch = async (url, options) => {
        expect(new URL(String(url)).pathname).toBe("/admin/person-data-scopes/warehouse-person")
        calls.push(options?.method ?? "GET")
        if (options?.method === "PUT") {
            const body = JSON.parse(String(options.body)) as PersonScopeSave
            expect(body.expected_policy_version).toBe(current.policy_version)
            saved.push(body)
            current.policy_version += 1
            for (const action of body.actions) current.items.push(scope(body.resource, action, [warehouseTerm("warehouse")]))
            return Response.json({ success: true, data: null })
        }
        return Response.json({ success: true, data: current })
    }
    try {
        await ensurePersonWarehouseScope("admin-jwt", "warehouse-person", [
            { resource: "stock_balance", actions: ["list"] },
            { resource: "stock_movement", actions: ["list"] },
        ], "warehouse")
        expect(calls).toEqual(["GET", "PUT", "GET", "PUT", "GET"])
        expect(saved.map((body) => body.expected_policy_version)).toEqual([11, 12])
    } finally {
        globalThis.fetch = originalFetch
    }
})

test("库存范围准备使用真实人员 ID，不访问退出运行时的角色范围接口", async () => {
    const originalFetch = globalThis.fetch
    let policyVersion = 20
    const views = new Map([["warehouse-person", view()], ["procurement-person", view()]])
    const writes: Array<{ person: string; resource: string }> = []
    rememberApiToken("admin", "admin-jwt")
    globalThis.fetch = async (url, options) => {
        const path = new URL(String(url)).pathname
        if (path === "/admin/warehouses") return Response.json({ success: true, data: { items: [{ id: "warehouse", warehouse_code: "BJ-TZ-01" }] } })
        if (path === "/admin/admins") return Response.json({ success: true, data: [{ id: "warehouse-person", account: "cangchu" }, { id: "procurement-person", account: "caigou" }] })
        const person = path.replace("/admin/person-data-scopes/", "")
        const current = views.get(person)
        if (!current) throw new Error(`禁止的 fixture 请求：${path}`)
        if (options?.method === "PUT") {
            const body = JSON.parse(String(options.body)) as PersonScopeSave
            expect(body.expected_policy_version).toBe(policyVersion)
            writes.push({ person, resource: body.resource })
            policyVersion += 1
            for (const action of body.actions) current.items.push(scope(body.resource, action, [warehouseTerm("warehouse")]))
            return Response.json({ success: true, data: null })
        }
        return Response.json({ success: true, data: { ...current, policy_version: policyVersion } })
    }
    try {
        await ensureWarehouseStockScope("BJ-TZ-01")
        expect(writes).toEqual([
            { person: "warehouse-person", resource: "stock_balance" },
            { person: "warehouse-person", resource: "stock_movement" },
            { person: "warehouse-person", resource: "stock_reservation" },
            { person: "warehouse-person", resource: "stock_adjustment" },
            { person: "procurement-person", resource: "stock_reservation" },
        ])
    } finally {
        globalThis.fetch = originalFetch
    }
})
