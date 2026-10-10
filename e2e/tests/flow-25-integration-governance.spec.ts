/**
 * flow-25: API 供应商连接身份、配置前置与结算来源完整性。
 * 使用真实管理页面及 API；引用注册表按运行配置装配，绑定仅接受服务端短时票据。
 * 未绑定技术引用、未启用连接与缺失结算来源均须失败关闭，拒绝命令不得修改连接。
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
type ReferenceOption = { reference_id: string; alias: string; version: string; expires_at: number }

async function uiResult<T>(response: Response): Promise<T> {
    const result = (await response.json()) as Envelope<T>
    expect(response.ok() && result.success !== false, result.errorMessage).toBe(true)
    return result.data
}

async function post(token: string, endpoint: string, body: unknown) {
    return fetch(`${API_BASE}${endpoint}`, {
        method: "POST",
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
}

async function rejected(token: string, endpoint: string, body: unknown, reason: RegExp | string) {
    const response = await post(token, endpoint, body)
    const result = (await response.json()) as Envelope<unknown>
    expect(response.ok && result.success !== false, endpoint).toBe(false)
    expect([400, 409, 422], result.errorMessage).toContain(response.status)
    if (typeof reason === "string") {
        expect(response.status, endpoint).toBe(422)
        expect(result.code, endpoint).toBe(reason)
    } else {
        expect(result.errorMessage).toMatch(reason)
    }
    return result
}

test("flow-25 配置绑定校验票据，技术未就绪禁止执行、未启用禁止下单、缺来源禁止结算", async ({ page }) => {
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
    for (const [action, blocker] of [
        ["ENABLE", "TECHNICAL_REFERENCES_MISSING"],
        ["RUN_HEALTH_CHECK", "TECHNICAL_REFERENCES_MISSING"],
        ["START_CATALOG_SYNC", "CONNECTION_NOT_ENABLED"],
    ]) {
        expect(detail.allowed_actions, action).not.toContain(action)
        expect(detail.action_blockers, action).toContainEqual(expect.objectContaining({ action, code: blocker }))
    }
    const registryAvailable = detail.allowed_actions.includes("BIND_ENDPOINT_REFERENCE")
    expect(detail.allowed_actions.includes("BIND_CREDENTIAL_REFERENCE")).toBe(registryAvailable)
    for (const action of ["BIND_ENDPOINT_REFERENCE", "BIND_CREDENTIAL_REFERENCE"]) {
        expect(detail.action_blockers.some((item) => item.action === action
            && item.code === "REFERENCE_REGISTRY_UNAVAILABLE")).toBe(!registryAvailable)
    }
    expect(detail).not.toHaveProperty("credential_reference")
    expect(detail).not.toHaveProperty("endpoint_reference")

    await page.locator("#supplier-api-connections-center-tab-security").click()
    for (const kind of ["endpoint", "credential"] as const) {
        const bindButton = page.locator(`#supplier-api-connections-security-bind-${kind}`)
        if (!registryAvailable) {
            await expect(bindButton).toBeDisabled()
            const options = await apiGet<ReferenceOption[]>(token,
                `/admin/supplier-api-connections/${identity.id}/reference-options`, { kind })
            expect(options).toEqual([])
            continue
        }
        await expect(bindButton).toBeEnabled()
        const optionsResponse = page.waitForResponse((response) => response.request().method() === "GET"
            && new URL(response.url()).pathname === `/admin/supplier-api-connections/${identity.id}/reference-options`
            && new URL(response.url()).searchParams.get("kind") === kind)
        await bindButton.click()
        const response = await optionsResponse
        expect(response.headers()["cache-control"]).toBe("no-store")
        const options = await uiResult<ReferenceOption[]>(response)
        expect(options.length, `${kind} 注册表必须提供当前测试环境配置`).toBeGreaterThan(0)
        for (const option of options) {
            expect(Object.keys(option).sort()).toEqual(["alias", "expires_at", "reference_id", "version"])
            expect(option.alias).not.toBe("")
            expect(option.version).not.toBe("")
            expect(option.reference_id).not.toBe("")
            expect(option.reference_id).not.toContain("config://")
            expect(option.expires_at).toBeGreaterThan(Date.now() / 1000)
            expect(option.expires_at).toBeLessThanOrEqual(Date.now() / 1000 + 300)
        }
        const binding = page.getByRole("dialog", { name: kind === "endpoint" ? "绑定地址配置" : "绑定密钥配置" })
        await expect(binding.locator(`#supplier-api-connections-reference-bind-${kind}-confirm`)).toBeDisabled()
        await binding.locator(`#supplier-api-connections-reference-bind-${kind}-cancel`).click()
        await expect(binding).toBeHidden()
    }

    const createBody = { supplier_id: supplier!.id, connection_code: code, environment: "testing", status: "disabled", capabilities: [] }
    await rejected(token, "/admin/supplier-api-connections", createBody, /重复|已存在|唯一/)
    await rejected(token, "/admin/supplier-api-connections", { ...createBody, connection_code: `${code}-ACTIVE`, status: "active" }, /停用/)
    await rejected(token, "/admin/supplier-api-connections", { ...createBody, connection_code: `${code}-REF`, credential_reference: "unregistered-reference" }, /引用/)

    const endpoint = `/admin/supplier-api-connections/${identity.id}/commands`
    await rejected(token, endpoint, { action: "BIND_ENDPOINT_REFERENCE", expected_version: detail.version,
        payload_reference: "unregistered-reference", idempotency_key: `${code}-bind` }, "BUSINESS_RULE_BLOCKED")
    await rejected(token, endpoint, { action: "BIND_CREDENTIAL_REFERENCE", expected_version: detail.version,
        payload_reference: "unregistered-reference", idempotency_key: `${code}-bind-credential` }, "BUSINESS_RULE_BLOCKED")
    if (registryAvailable) {
        const options = await apiGet<ReferenceOption[]>(token,
            `/admin/supplier-api-connections/${identity.id}/reference-options`, { kind: "endpoint" })
        const ticket = options[0].reference_id
        const wrongKind = await rejected(token, endpoint, {
            action: "BIND_CREDENTIAL_REFERENCE", expected_version: detail.version,
            payload_reference: ticket, idempotency_key: `${code}-wrong-kind`,
        }, "BUSINESS_RULE_BLOCKED")
        expect(wrongKind.errorMessage).toBe("当前业务条件不允许继续操作，请核对相关资料后重试")
        const otherResponse = await post(token, "/admin/supplier-api-connections", {
            ...createBody, connection_code: `${code}-OTHER`,
        })
        const otherResult = await otherResponse.json() as Envelope<Connection>
        expect(otherResponse.ok && otherResult.success !== false, otherResult.errorMessage).toBe(true)
        const other = otherResult.data
        const wrongConnection = await rejected(token, `/admin/supplier-api-connections/${other.id}/commands`, {
            action: "BIND_ENDPOINT_REFERENCE", expected_version: other.version,
            payload_reference: ticket, idempotency_key: `${code}-wrong-connection`,
        }, "BUSINESS_RULE_BLOCKED")
        expect(wrongConnection.errorMessage).toBe("当前业务条件不允许继续操作，请核对相关资料后重试")
        const otherAfter = await apiGet<Connection>(token, `/admin/supplier-api-connections/${other.id}`)
        expect(otherAfter.version).toBe(other.version)
        expect(otherAfter.safe_references.endpoint.state).toBe("MISSING")
        expect(otherAfter.safe_references.credential.state).toBe("MISSING")
        expect(otherAfter.health_records).toEqual([])
    }
    const enabled = await rejected(token, endpoint, { action: "ENABLE", expected_version: detail.version,
        idempotency_key: `${code}-enable` }, "BUSINESS_RULE_BLOCKED")
    expect(enabled.errorMessage).toContain("地址与密钥引用")
    const health = await rejected(token, endpoint, { action: "RUN_HEALTH_CHECK", check_type: "CONNECTIVITY", expected_version: detail.version,
        idempotency_key: `${code}-health` }, "BUSINESS_RULE_BLOCKED")
    expect(health.errorMessage).toContain("地址与密钥引用")
    const catalog = await rejected(token, endpoint, { action: "START_CATALOG_SYNC", expected_version: detail.version,
        idempotency_key: `${code}-sync` }, "BUSINESS_RULE_BLOCKED")
    expect(catalog.errorMessage).toContain("连接启用后")
    const blocked = await apiGet<Connection>(token, `/admin/supplier-api-connections/${identity.id}`)
    expect(blocked).toEqual(detail)

    const ordersBefore = await apiGet<ApiPage<{ id: string }>>(token, "/admin/supplier-fulfillment-orders", { page: 1, page_size: 100 })
    const order = await rejected(token, "/admin/supplier-fulfillment-orders", {
        fulfillment_order_no: `${code}-ORDER`, supplier_id: supplier!.id, connection_id: identity.id, split_no: 1,
        address_snapshot_encrypted: "opaque-e2e-address", address_snapshot_fingerprint: "e2e-address-fingerprint",
        items: [{ supplier_offering_revision_id: "unavailable-revision", quantity: "1", unit_cost_snapshot_gross: "1", input_tax_rate: "0" }],
    }, "BUSINESS_RULE_BLOCKED")
    expect(order.errorMessage).toContain("供应商连接未启用")
    const ordersAfter = await apiGet<ApiPage<{ id: string }>>(token, "/admin/supplier-fulfillment-orders", { page: 1, page_size: 100 })
    expect(ordersAfter.total).toBe(ordersBefore.total)

    const settlementsBefore = await apiGet<ApiPage<{ id: string }>>(token, "/admin/supplier-settlement-statements", { page: 1, page_size: 100 })
    const settlement = await rejected(token, "/admin/supplier-settlement-statements", {
        action: "CREATE", supplier_id: supplier!.id, period_start: "2026-01-01", period_end: "2026-01-31",
        request_id: `${code}-settlement`, idempotency_key: `${code}-settlement`,
    }, "BUSINESS_RULE_BLOCKED")
    expect(settlement.errorMessage).toBe("当前业务条件不允许继续操作，请核对相关资料后重试")
    const settlementsAfter = await apiGet<ApiPage<{ id: string }>>(token, "/admin/supplier-settlement-statements", { page: 1, page_size: 100 })
    expect(settlementsAfter.total).toBe(settlementsBefore.total)
    const unchanged = await apiGet<Connection>(token, `/admin/supplier-api-connections/${identity.id}`)
    expect(unchanged.version).toBe(detail.version)
    expect(unchanged.status).toBe("disabled")
    expect(unchanged.health_records).toEqual([])
    const listed = await apiGet<ApiPage<Connection>>(token, "/admin/supplier-api-connections", { connection_code: code, page: 1, page_size: 100 })
    expect(listed.items.filter((item) => item.connection_code === code)).toHaveLength(1)
})
