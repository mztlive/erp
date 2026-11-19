/**
 * flow-25: API 供应商连接身份、配置前置与结算来源完整性。
 * 平台设置使用部署配置；引用票据只通过独立 API 核验，不触发外部供应商操作。
 * 未绑定技术引用、未启用连接与缺失结算来源均须失败关闭，拒绝命令不得修改连接。
 * 不将身份创建、负向校验视为真实供应商健康检查、目录同步或履约成功证据。
 */
import { workPage } from "../helpers/workspace-page"
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
    connector_type: string
    status: string
    version: number
    safe_references: { endpoint: { state: string }; credential: { state: string } }
    allowed_actions: string[]
    action_blockers: Array<{ action: string; code: string; message: string }>
    deployment_ready: boolean
    declared_capabilities: string[]
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

async function rejected(token: string, endpoint: string, body: unknown, status: 400 | 409 | 422) {
    const response = await post(token, endpoint, body)
    const result = (await response.json()) as Envelope<unknown>
    expect(response.status, `${endpoint}: ${result.errorMessage}`).toBe(status)
    expect(result.success).toBe(false)
    expect(result.code).toBe({ 400: "INVALID_REQUEST", 409: "CONFLICT", 422: "BUSINESS_RULE_BLOCKED" }[status])
    return result
}

const blockedMessage = "当前业务条件不允许继续操作，请核对相关资料后重试"

test("flow-25 配置绑定校验票据，技术未就绪禁止执行、未启用禁止下单、缺来源禁止结算", async ({ page }) => {
    await loginViaUi(page, "admin")
    const token = await apiToken("admin")
    const suppliers = await apiGet<ApiPage<{ id: string; legal_name: string | null; short_name: string | null }>>(
        token, "/admin/suppliers", { page: 1, page_size: 100 },
    )
    const supplier = suppliers.items.find((item) => item.legal_name?.includes("杭州狮峰"))
        ?? suppliers.items.find((item) => item.legal_name)
    expect(supplier, "隔离种子必须有可选择的供应商").toBeTruthy()
    const supplierName = supplier!.legal_name!.trim()
    const settingsTitle = supplier!.short_name?.trim() || supplierName
    const code = `E2E-CONN-${Date.now()}`
    const connectorType = "dangaoshushu"

    await page.goto("/supplier-api/connections")
    await workPage(page).locator("#supplier-api-connections-list-create").click()
    const dialog = workPage(page).getByRole("dialog", { name: "新建连接" })
    await dialog.locator("#supplier-api-connections-create-connection-code").fill(code)
    await chooseOption(page, dialog.locator("#supplier-api-connections-create-supplier"), supplierName)
    await chooseOption(page, dialog.locator("#supplier-api-connections-create-connector-type"), "蛋糕叔叔 API")
    const created = page.waitForResponse((response) => response.request().method() === "POST"
        && new URL(response.url()).pathname === "/admin/supplier-api-connections")
    await dialog.locator("#supplier-api-connections-create-submit").click()
    const identity = await uiResult<Connection>(await created)
    expect(identity.connection_code).toBe(code)
    expect(identity.supplier_id).toBe(supplier!.id)
    expect(identity.connector_type).toBe(connectorType)
    expect(identity).not.toHaveProperty("environment")
    expect(identity.status).toBe("disabled")
    await expect(dialog).toBeHidden()

    await workPage(page).locator("#supplier-api-connections-create-success-open").click()
    await expect(page).toHaveURL((url) => url.pathname === `/supplier-api/connections/${identity.id}`)
    await expect(workPage(page).getByRole("heading", { name: settingsTitle, level: 1, exact: true })).toBeVisible()
    await expect(workPage(page).getByText(`连接代码：${code}`, { exact: true })).toBeVisible()

    const connectionPath = `/admin/supplier-api-connections/${identity.id}`
    const detail = await apiGet<Connection>(token, connectionPath)
    expect(detail.health_records).toEqual([])
    expect(detail).not.toHaveProperty("credential_reference")
    expect(detail).not.toHaveProperty("endpoint_reference")
    for (const kind of ["endpoint", "credential"] as const) {
        expect(detail.safe_references[kind]).toMatchObject({
            state: detail.deployment_ready ? "BOUND" : "MISSING",
            alias: null,
            version: null,
        })
        expect(detail.allowed_actions).not.toContain(`BIND_${kind.toUpperCase()}_REFERENCE`)
    }

    const createBody = { supplier_id: supplier!.id, connection_code: code, connector_type: connectorType }
    await rejected(token, "/admin/supplier-api-connections", createBody, 409)
    await rejected(token, "/admin/supplier-api-connections", { ...createBody, connection_code: `${code}-ACTIVE`, status: "active" }, 400)
    await rejected(token, "/admin/supplier-api-connections", { ...createBody, connection_code: `${code}-REF`, credential_reference: "unregistered-reference" }, 400)

    const endpoint = `${connectionPath}/commands`
    for (const kind of ["endpoint", "credential"] as const) {
        const optionsResponse = await fetch(`${API_BASE}${connectionPath}/reference-options?kind=${kind}`, {
            headers: { Authorization: `Bearer ${token}` },
            signal: AbortSignal.timeout(20_000),
        })
        const options = await optionsResponse.json() as Envelope<ReferenceOption[]>
        if (detail.deployment_ready) {
            expect(optionsResponse.status, options.errorMessage).toBe(200)
            expect(optionsResponse.headers.get("cache-control")).toBe("no-store")
            expect(options.data).not.toHaveLength(0)
            for (const option of options.data) {
                expect(Object.keys(option).sort()).toEqual(["alias", "expires_at", "reference_id", "version"])
                expect(option.alias).not.toBe("")
                expect(option.version).not.toBe("")
                expect(option.reference_id).not.toBe("")
                expect(option.reference_id).not.toContain("config://")
                expect(option.expires_at).toBeGreaterThan(Date.now() / 1000)
                expect(option.expires_at).toBeLessThanOrEqual(Date.now() / 1000 + 300)
            }
            if (kind === "endpoint") {
                const ticket = options.data[0].reference_id
                const wrongKind = await rejected(token, endpoint, {
                    action: "BIND_CREDENTIAL_REFERENCE", expected_version: detail.version,
                    payload_reference: ticket, idempotency_key: `${code}-wrong-kind`,
                }, 422)
                expect(wrongKind.errorMessage).toBe(blockedMessage)
                const otherResponse = await post(token, "/admin/supplier-api-connections", {
                    ...createBody, connection_code: `${code}-OTHER`,
                })
                const otherResult = await otherResponse.json() as Envelope<Connection>
                expect(otherResponse.ok && otherResult.success !== false, otherResult.errorMessage).toBe(true)
                const other = otherResult.data
                const otherPath = `/admin/supplier-api-connections/${other.id}`
                const before = await apiGet<Connection>(token, otherPath)
                const wrongConnection = await rejected(token, `${otherPath}/commands`, {
                    action: "BIND_ENDPOINT_REFERENCE", expected_version: other.version,
                    payload_reference: ticket, idempotency_key: `${code}-wrong-connection`,
                }, 422)
                expect(wrongConnection.errorMessage).toBe(blockedMessage)
                expect(await apiGet<Connection>(token, otherPath)).toEqual(before)
            }
        } else {
            expect(optionsResponse.status, options.errorMessage).toBe(422)
            expect(options.code).toBe("BUSINESS_RULE_BLOCKED")
        }
        const malformed = await rejected(token, endpoint, {
            action: `BIND_${kind.toUpperCase()}_REFERENCE`, expected_version: detail.version,
            payload_reference: "unregistered-reference", idempotency_key: `${code}-bind-${kind}`,
        }, 422)
        expect(malformed.errorMessage).toBe(blockedMessage)
    }

    if (!detail.deployment_ready) {
        await expect(workPage(page).getByText("供应商接口尚未就绪", { exact: true })).toBeVisible()
        await expect(workPage(page).getByRole("button", { name: "允许自动下单", exact: true })).toBeDisabled()
        await expect(workPage(page).getByRole("button", { name: "测试连接", exact: true })).toBeDisabled()
        expect(detail.declared_capabilities).toEqual([])
        for (const [action, blocker] of [
            ["ENABLE", "ORDER_UNAVAILABLE"],
            ["RUN_HEALTH_CHECK", "SUPPLIER_CONNECTOR_UNAVAILABLE"],
            ["START_CATALOG_SYNC", "SUPPLIER_CONNECTOR_UNAVAILABLE"],
        ]) {
            expect(detail.allowed_actions).not.toContain(action)
            expect(detail.action_blockers).toContainEqual(expect.objectContaining({ action, code: blocker }))
        }
        for (const action of ["ENABLE", "RUN_HEALTH_CHECK", "START_CATALOG_SYNC"]) {
            const blocked = await rejected(token, endpoint, {
                action, expected_version: detail.version, idempotency_key: `${code}-${action}`,
                ...(action === "RUN_HEALTH_CHECK" ? { check_type: "AUTHENTICATION" } : {}),
            }, 422)
            expect(blocked.errorMessage).toBe(blockedMessage)
        }
    }
    expect(await apiGet<Connection>(token, connectionPath)).toEqual(detail)

    const ordersBefore = await apiGet<ApiPage<{ id: string }>>(token, "/admin/supplier-fulfillment-orders", { page: 1, page_size: 100 })
    const order = await rejected(token, "/admin/supplier-fulfillment-orders", {
        fulfillment_order_no: `${code}-ORDER`, supplier_id: supplier!.id, connection_id: identity.id, split_no: 1,
        address_snapshot_encrypted: "opaque-e2e-address", address_snapshot_fingerprint: "e2e-address-fingerprint",
        items: [{ supplier_offering_revision_id: "unavailable-revision", quantity: "1", unit_cost_snapshot_gross: "1", input_tax_rate: "0" }],
    }, 422)
    expect(order.errorMessage).toContain("供应商连接未启用")
    const ordersAfter = await apiGet<ApiPage<{ id: string }>>(token, "/admin/supplier-fulfillment-orders", { page: 1, page_size: 100 })
    expect(ordersAfter.total).toBe(ordersBefore.total)

    const period = { supplier_id: supplier!.id, period_start: "2026-01-01", period_end: "2026-01-31" }
    const evidence = await fetch(`${API_BASE}/admin/supplier-settlement-source-evidence?${new URLSearchParams(period)}`, {
        headers: { Authorization: `Bearer ${token}` },
        signal: AbortSignal.timeout(20_000),
    })
    expect(evidence.status).toBe(404)
    expect(await evidence.json()).toMatchObject({ success: false, code: "NOT_FOUND" })
    const settlementsBefore = await apiGet<ApiPage<{ id: string }>>(token, "/admin/supplier-settlement-statements", { page: 1, page_size: 100 })
    const financeToken = await apiToken("caiwu")
    const settlement = await rejected(financeToken, "/admin/supplier-settlement-statements", {
        action: "CREATE", ...period,
        request_id: `${code}-settlement`, idempotency_key: `${code}-settlement`,
    }, 422)
    expect(settlement.errorMessage).toBe(blockedMessage)
    const settlementsAfter = await apiGet<ApiPage<{ id: string }>>(token, "/admin/supplier-settlement-statements", { page: 1, page_size: 100 })
    expect(settlementsAfter.total).toBe(settlementsBefore.total)
    const unchanged = await apiGet<Connection>(token, `/admin/supplier-api-connections/${identity.id}`)
    expect(unchanged.version).toBe(detail.version)
    expect(unchanged.status).toBe("disabled")
    expect(unchanged.health_records).toEqual([])
    const listed = await apiGet<ApiPage<Connection>>(token, "/admin/supplier-api-connections", { connection_code: code, page: 1, page_size: 100 })
    expect(listed.items.filter((item) => item.connection_code === code)).toHaveLength(1)
})
