/**
 * flow-25: API 供应商连接身份、配置前置与结算来源完整性。
 * 使用真实管理页面及 API；当前组合根未注入供应商连接器，因此验收失败关闭。
 * 不将身份创建、负向校验视为真实供应商健康检查、目录同步或履约成功证据。
 */
import { test, expect, type Response } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { loginViaUi } from "../helpers/login"
import { chooseOption } from "../helpers/ui"

type Envelope<T> = { success?: boolean; errorMessage?: string; code?: string; data: T }
type ApiPage<T> = { items: T[]; total: number }
type Connection = {
    id: string
    connection_code: string
    supplier_id: string
    environment: string
    status: string
    version: number
    safe_references: { endpoint: { state: string }; credential: { state: string } }
    allowed_actions: string[]
    action_blockers: Array<{ action: string; code: string; message: string }>
    capabilities: unknown[]
    health_records: unknown[]
}

async function uiResult<T>(response: Response): Promise<T> {
    const result = (await response.json()) as Envelope<T>
    expect(response.ok() && result.success !== false, result.errorMessage).toBe(true)
    return result.data
}

async function rejected(token: string, endpoint: string, body: unknown, reason: RegExp | string) {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method: "POST",
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
    const result = (await response.json()) as Envelope<unknown>
    expect(response.ok && result.success !== false, endpoint).toBe(false)
    expect([400, 409, 422], result.errorMessage).toContain(response.status)
    if (typeof reason === "string") {
        expect(response.status, endpoint).toBe(422)
        expect(result.code, endpoint).toBe(reason)
    } else {
        expect(result.errorMessage).toMatch(reason)
    }
}

test("flow-25 连接创建后配置失败关闭，未启用禁止下单、缺来源禁止结算", async ({ page }) => {
    await loginViaUi(page, "admin")
    const token = await apiToken("admin")
    const suppliers = await apiGet<ApiPage<{ id: string; legal_name: string | null }>>(
        token, "/admin/suppliers", { page: 1, page_size: 100 },
    )
    const supplier = suppliers.items.find((item) => item.legal_name?.includes("杭州狮峰"))
        ?? suppliers.items.find((item) => item.legal_name)
    expect(supplier, "隔离种子必须有可选择的供应商").toBeTruthy()
    const code = `E2E-CONN-${Date.now()}`

    await page.goto("/supplier-api/connections")
    await page.locator("#supplier-api-connections-list-create").click()
    const dialog = page.getByRole("dialog", { name: "新建连接" })
    await dialog.locator("#supplier-api-connections-create-connection-code").fill(code)
    await chooseOption(page, dialog.locator("#supplier-api-connections-create-supplier"), supplier!.legal_name!)
    await chooseOption(page, dialog.locator("#supplier-api-connections-create-environment"), "测试")
    const created = page.waitForResponse((response) => response.request().method() === "POST"
        && new URL(response.url()).pathname === "/admin/supplier-api-connections")
    await dialog.locator("#supplier-api-connections-create-submit").click()
    const identity = await uiResult<Connection>(await created)
    expect(identity.connection_code).toBe(code)
    expect(identity.environment).toBe("testing")
    expect(identity.status).toBe("disabled")
    await expect(dialog).toBeHidden()

    await page.locator("#supplier-api-connections-create-success-open").click()
    await expect(page).toHaveURL((url) => url.pathname === "/supplier-api/connections"
        && url.searchParams.get("connectionId") === identity.id)
    await expect(page.getByRole("heading", { name: new RegExp(code) })).toBeVisible()
    await expect(page.locator("#supplier-api-connections-center-enable")).toBeDisabled()
    await expect(page.locator("#supplier-api-connections-center-run-health")).toBeDisabled()
    const detail = await apiGet<Connection>(token, `/admin/supplier-api-connections/${identity.id}`)
    expect(detail.safe_references.endpoint.state).toBe("MISSING")
    expect(detail.safe_references.credential.state).toBe("MISSING")
    expect(detail.capabilities).toEqual([])
    expect(detail.health_records).toEqual([])
    expect(detail.allowed_actions).not.toContain("ENABLE")
    expect(detail.action_blockers.some((item) => item.code === "REFERENCE_REGISTRY_UNAVAILABLE")).toBe(true)
    expect(detail).not.toHaveProperty("credential_reference")
    expect(detail).not.toHaveProperty("endpoint_reference")

    const createBody = { supplier_id: supplier!.id, connection_code: code, environment: "testing", status: "disabled", capabilities: [] }
    await rejected(token, "/admin/supplier-api-connections", createBody, /重复|已存在|唯一/)
    await rejected(token, "/admin/supplier-api-connections", { ...createBody, connection_code: `${code}-ACTIVE`, status: "active" }, /停用/)
    await rejected(token, "/admin/supplier-api-connections", { ...createBody, connection_code: `${code}-REF`, credential_reference: "unregistered-reference" }, /引用/)

    const endpoint = `/admin/supplier-api-connections/${identity.id}/commands`
    await rejected(token, endpoint, { action: "BIND_ENDPOINT_REFERENCE", expected_version: detail.version,
        payload_reference: "unregistered-reference", idempotency_key: `${code}-bind` }, "BUSINESS_RULE_BLOCKED")
    await rejected(token, endpoint, { action: "ENABLE", expected_version: detail.version, idempotency_key: `${code}-enable` }, "BUSINESS_RULE_BLOCKED")
    await rejected(token, endpoint, { action: "RUN_HEALTH_CHECK", check_type: "CONNECTIVITY", expected_version: detail.version,
        idempotency_key: `${code}-health` }, "BUSINESS_RULE_BLOCKED")
    await rejected(token, endpoint, { action: "START_CATALOG_SYNC", expected_version: detail.version,
        idempotency_key: `${code}-sync` }, "BUSINESS_RULE_BLOCKED")
    const blocked = await apiGet<Connection>(token, `/admin/supplier-api-connections/${identity.id}`)
    expect(blocked).toEqual(detail)

    const ordersBefore = await apiGet<ApiPage<{ id: string }>>(token, "/admin/supplier-fulfillment-orders", { page: 1, page_size: 100 })
    await rejected(token, "/admin/supplier-fulfillment-orders", {
        fulfillment_order_no: `${code}-ORDER`, supplier_id: supplier!.id, connection_id: identity.id, split_no: 1,
        address_snapshot_encrypted: "opaque-e2e-address", address_snapshot_fingerprint: "e2e-address-fingerprint",
        items: [{ supplier_offering_revision_id: "unavailable-revision", quantity: "1", unit_cost_snapshot_gross: "1", input_tax_rate: "0" }],
    }, "BUSINESS_RULE_BLOCKED")
    const ordersAfter = await apiGet<ApiPage<{ id: string }>>(token, "/admin/supplier-fulfillment-orders", { page: 1, page_size: 100 })
    expect(ordersAfter.total).toBe(ordersBefore.total)

    const settlementsBefore = await apiGet<ApiPage<{ id: string }>>(token, "/admin/supplier-settlement-statements", { page: 1, page_size: 100 })
    await rejected(token, "/admin/supplier-settlement-statements", {
        action: "CREATE", supplier_id: supplier!.id, period_start: "2026-01-01", period_end: "2026-01-31",
        request_id: `${code}-settlement`, idempotency_key: `${code}-settlement`,
    }, "BUSINESS_RULE_BLOCKED")
    const settlementsAfter = await apiGet<ApiPage<{ id: string }>>(token, "/admin/supplier-settlement-statements", { page: 1, page_size: 100 })
    expect(settlementsAfter.total).toBe(settlementsBefore.total)
    const unchanged = await apiGet<Connection>(token, `/admin/supplier-api-connections/${identity.id}`)
    expect(unchanged.version).toBe(detail.version)
    expect(unchanged.status).toBe("disabled")
    expect(unchanged.health_records).toEqual([])
    const listed = await apiGet<ApiPage<Connection>>(token, "/admin/supplier-api-connections", { connection_code: code, page: 1, page_size: 100 })
    expect(listed.items.filter((item) => item.connection_code === code)).toHaveLength(1)
})
