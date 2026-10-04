/**
 * [flow-32] 业务审计的真实命令回放、中文安全投影及两类审计权限隔离。
 * 合同：docs/audit-business-dependency-contract.md V01、V02、V07、V10。
 * 每项验收自行建立业务或账号；不从审计记录恢复业务事实，不模拟写入结果。
 */
import { randomUUID } from "node:crypto"

import { test, expect, type Page } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { openLoggedInWorkspace } from "../helpers/login"

type Envelope<T> = {
    success?: boolean
    errorMessage?: string
    requestId?: string
    data: T
}
type ApiPage<T> = { items: T[]; total: number }
type Identity = {
    id: string
    category_code?: string
    brand_code?: string
    unit_code?: string
    supplier_no?: string
}
type Account = { id: string; account: string; name: string; role_ids: string[] }
type OfferingResult = {
    offering_id: string
    revision_id: string
    revision_no: number
}
type Offering = {
    id: string
    version: number
    current_revision_id: string
    current_revision_no: number
    availability_version: number
}
type Handover = {
    offering_id: string
    maintainer_user_id: string
    business_org_unit_id: string
    version: number
}
type AuditRow = {
    id: string
    actor_id: string
    actor_account: string
    actor_type: string
    action: string
    resource_type: string
    resource_id: string
    success: boolean
    message: string
    structured_event: {
        schema_version: number
        event_sequence: number
        action_code: string
        action_version: number
        action_label: string
        actor_id: string
        actor_account: string
        actor_type: string
        actor_name_snapshot: string
        resource_type: string
        resource_id: string
        resource_number_snapshot?: string
        result: string
        request_id: string
        command_id?: string
        field_changes: unknown[]
        facts: unknown[]
        occurred_at: number
    }
}

async function call<T>(
    token: string,
    method: string,
    endpoint: string,
    body?: unknown,
    traceId?: string,
) {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method,
        headers: {
            Authorization: `Bearer ${token}`,
            ...(body === undefined
                ? {}
                : { "Content-Type": "application/json" }),
            ...(traceId ? { "X-Trace-Id": traceId } : {}),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
    const responseText = await response.text()
    let envelope: Envelope<T>
    try {
        envelope = JSON.parse(responseText) as Envelope<T>
    } catch {
        throw new Error(
            `API ${method} ${endpoint} 返回非 JSON（HTTP ${response.status}，Content-Type ${response.headers.get("content-type") ?? "未提供"}）`,
        )
    }
    return { response, envelope }
}

async function ok<T>(
    token: string,
    method: string,
    endpoint: string,
    body?: unknown,
    traceId?: string,
): Promise<T> {
    const result = await call<T>(token, method, endpoint, body, traceId)
    expect(
        result.response.ok && result.envelope.success !== false,
        `${endpoint}: ${result.envelope.errorMessage ?? result.response.status}`,
    ).toBe(true)
    return result.envelope.data
}

function businessDate() {
    return new Intl.DateTimeFormat("en-CA", {
        timeZone: "Asia/Shanghai",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
    }).format(new Date())
}

async function createOfferingInput(adminToken: string, prefix: string) {
    const [categories, brands, units, suppliers, admins] = await Promise.all([
        apiGet<ApiPage<Identity>>(adminToken, "/admin/product-categories", {
            category_code: "TEA",
            page_size: 100,
        }),
        apiGet<ApiPage<Identity>>(adminToken, "/admin/product-brands", {
            brand_code: "SF",
            page_size: 100,
        }),
        apiGet<ApiPage<Identity>>(adminToken, "/admin/unit-of-measures", {
            unit_code: "HE",
            page_size: 100,
        }),
        apiGet<ApiPage<Identity>>(adminToken, "/admin/suppliers", {
            page_size: 100,
        }),
        apiGet<Account[]>(adminToken, "/admin/admins"),
    ])
    const category = categories.items.find((row) => row.category_code === "TEA")
    const brand = brands.items.find((row) => row.brand_code === "SF")
    const unit = units.items.find((row) => row.unit_code === "HE")
    const supplier = suppliers.items.find(
        (row) => row.supplier_no === "SUP-HZSF",
    )
    const actor = admins.find((row) => row.account === "caigou")
    const adminActor = admins.find((row) => row.account === "admin")
    expect(
        category && brand && unit && supplier && actor && adminActor,
        "业务审计验收需要固定目录和采购岗位种子",
    ).toBeTruthy()
    const product = await ok<{ id: string }>(
        adminToken,
        "POST",
        "/admin/products",
        {
            change_reason: "E2E 业务审计独立商品",
            product_no: prefix,
            product_kind: "PHYSICAL",
            maintainer_user_id: actor!.id,
            name: `E2E 业务审计 ${prefix}`,
            category_id: category!.id,
            brand_id: brand!.id,
            status: "active",
            effective_from: businessDate(),
            carousel_media: [],
            detail_media: [],
            skus: [
                {
                    sku_no: prefix,
                    name: prefix,
                    base_unit_id: unit!.id,
                    sales_visible_price_gross: "99.00",
                    spec_entries: [],
                },
            ],
        },
    )
    const skus = await apiGet<ApiPage<{ id: string }>>(
        adminToken,
        `/admin/products/${product.id}/skus`,
        { page_size: 100 },
    )
    expect(skus.items).toHaveLength(1)
    return {
        actor: actor!,
        adminActor: adminActor!,
        body: {
            sku_id: skus.items[0]!.id,
            supplier_id: supplier!.id,
            supplier_sku_code: prefix,
            source_type: "MANUAL",
            terms: {
                dropship_supply_price_gross: "21.00",
                bulk_supply_price_gross: "19.00",
                input_tax_rate: "0.09",
                bulk_minimum_order_quantity: "5",
                supply_region: ["全国"],
                product_capabilities: [],
                valid_from: businessDate(),
            },
            availability_status: "AVAILABLE",
            available_quantity: "10",
            change_reason: `仅请求正文含此标记-${prefix}`,
            idempotency_key: randomUUID(),
        },
    }
}

async function offeringAudits(token: string) {
    return apiGet<ApiPage<AuditRow>>(token, "/admin/audit-logs", {
        actor_account: "caigou",
        action: "supplier_offering.create",
        event_result: "succeeded",
        page: 1,
        page_size: 100,
    })
}

test("业务命令回放保持一份供给和成功审计；页面展示真实中文、人物快照及请求关联", async ({
    browser,
}) => {
    const prefix = `E2E-AUDIT-${Date.now().toString(36).toUpperCase()}`
    const adminToken = await apiToken("admin")
    const actorToken = await apiToken("caigou")
    const fixture = await createOfferingInput(adminToken, prefix)
    const traceId = `audit-${randomUUID()}`
    const createdReply = await call<OfferingResult>(
        actorToken,
        "POST",
        "/admin/supplier-offerings",
        fixture.body,
        traceId,
    )
    expect(
        createdReply.response.ok && createdReply.envelope.success !== false,
        createdReply.envelope.errorMessage,
    ).toBe(true)
    expect(createdReply.response.headers.get("x-trace-id")).toBe(traceId)
    const created = createdReply.envelope.data
    const originalFact = await apiGet<Offering>(
        actorToken,
        `/admin/supplier-offerings/${created.offering_id}`,
    )
    const before = await offeringAudits(adminToken)
    const rows = before.items.filter(
        (row) => row.resource_id === created.offering_id,
    )
    expect(rows).toHaveLength(1)
    const audit = rows[0]!
    expect(audit).toMatchObject({
        actor_id: fixture.actor.id,
        actor_account: fixture.actor.account,
        actor_type: "admin",
        action: "supplier_offering.create",
        resource_type: "supplier_offering",
        resource_id: created.offering_id,
        success: true,
        structured_event: {
            schema_version: 1,
            event_sequence: 1,
            action_version: 1,
            action_code: "supplier_offering.create",
            action_label: "创建供应商供给",
            actor_id: fixture.actor.id,
            actor_account: fixture.actor.account,
            actor_type: "admin",
            actor_name_snapshot: fixture.actor.name,
            resource_type: "supplier_offering",
            resource_id: created.offering_id,
            result: "succeeded",
            request_id: traceId,
            field_changes: [],
            facts: [],
        },
    })
    expect(audit.structured_event.occurred_at).toBeGreaterThan(0)
    expect(audit.message).toContain("创建供应商供给")
    expect(JSON.stringify(audit)).not.toContain(fixture.body.change_reason)
    expect(JSON.stringify(audit)).not.toContain(fixture.body.idempotency_key)

    await test.step("同键同载荷返回原结果，版本和成功审计均不新增；同键异载荷拒绝", async () => {
        expect(
            await ok<OfferingResult>(
                actorToken,
                "POST",
                "/admin/supplier-offerings",
                fixture.body,
                `replay-${randomUUID()}`,
            ),
        ).toEqual(created)
        expect(
            await apiGet<Offering>(
                actorToken,
                `/admin/supplier-offerings/${created.offering_id}`,
            ),
        ).toEqual(originalFact)
        expect(await offeringAudits(adminToken)).toEqual(before)
        const conflict = await call(
            actorToken,
            "POST",
            "/admin/supplier-offerings",
            {
                ...fixture.body,
                supplier_sku_code: `${prefix}-CHANGED`,
            },
        )
        expect(conflict.response.status).toBe(409)
        expect(conflict.envelope.success).toBe(false)
        expect(await offeringAudits(adminToken)).toEqual(before)
        const all = await apiGet<ApiPage<Offering>>(
            actorToken,
            "/admin/supplier-offerings",
            { q: prefix, page_size: 100 },
        )
        expect(all.items.map((row) => row.id)).toEqual([created.offering_id])
    })

    let handoverAudit: AuditRow
    const handoverTraceId = `handover-${randomUUID()}`
    await test.step("供给交接通过独立领域回执回放，保存命令关联和原业务编号", async () => {
        const org = await apiGet<{
            people: Array<{
                id: string
                account: string
                active: boolean
                own_org_unit_id?: string
            }>
        }>(adminToken, "/admin/org-units")
        const target = org.people.find(
            (person) => person.account === "admin" && person.active,
        )
        expect(
            target?.own_org_unit_id,
            "建档管理员必须有已启用主属组织",
        ).toBeTruthy()
        const handoverBody = {
            target_user_id: target!.id,
            target_org_unit_id: target!.own_org_unit_id,
            reason: `交接请求正文-${prefix}`,
            expected_version: originalFact.version,
            idempotency_key: randomUUID(),
        }
        const endpoint = `/admin/supplier-offerings/${created.offering_id}/handover`
        const handed = await ok<Handover>(
            adminToken,
            "POST",
            endpoint,
            handoverBody,
            handoverTraceId,
        )
        expect(handed).toMatchObject({
            offering_id: created.offering_id,
            maintainer_user_id: target!.id,
            business_org_unit_id: target!.own_org_unit_id,
        })
        expect(handed.version).toBeGreaterThan(originalFact.version)
        const query = {
            action: "supplier_offering.handover",
            actor_account: "admin",
            resource_number: prefix,
            page_size: 100,
        }
        const events = await apiGet<ApiPage<AuditRow>>(
            adminToken,
            "/admin/audit-logs",
            query,
        )
        expect(events.total).toBe(1)
        expect(events.items).toHaveLength(1)
        handoverAudit = events.items[0]!
        expect(handoverAudit.structured_event).toMatchObject({
            event_sequence: 1,
            action_code: "supplier_offering.handover",
            action_label: "供给交接",
            actor_id: fixture.adminActor.id,
            actor_account: "admin",
            actor_name_snapshot: fixture.adminActor.name,
            resource_id: created.offering_id,
            resource_number_snapshot: prefix,
            result: "succeeded",
            request_id: handoverTraceId,
        })
        expect(handoverAudit.structured_event.command_id).toMatch(
            /^offering-handover-/,
        )
        expect(handoverAudit.structured_event.command_id).not.toBe(
            handoverAudit.id,
        )
        expect(handoverAudit.structured_event.facts).toContainEqual({
            field: "handover_result",
            field_label: "交接结果",
            value: { kind: "code", code: "completed", label: "已交接" },
        })
        expect(handoverAudit.structured_event.facts).toContainEqual({
            field: "responsibility",
            field_label: "责任人",
            value: { kind: "changed" },
        })
        expect(JSON.stringify(handoverAudit)).not.toContain(handoverBody.reason)
        expect(JSON.stringify(handoverAudit)).not.toContain(
            handoverBody.idempotency_key,
        )
        expect(
            await ok<Handover>(
                adminToken,
                "POST",
                endpoint,
                handoverBody,
                `retry-${randomUUID()}`,
            ),
        ).toEqual(handed)
        expect(
            await apiGet<ApiPage<AuditRow>>(
                adminToken,
                "/admin/audit-logs",
                query,
            ),
        ).toEqual(events)
        const changed = await call(adminToken, "POST", endpoint, {
            ...handoverBody,
            reason: "不同交接原因",
        })
        expect(changed.response.status).toBe(409)
        expect(changed.envelope.success).toBe(false)
        expect(
            await apiGet<ApiPage<AuditRow>>(
                adminToken,
                "/admin/audit-logs",
                query,
            ),
        ).toEqual(events)
    })

    await test.step("业务审计列表及详情使用保存的安全事件，不展示原始请求正文", async () => {
        const { page } = await openLoggedInWorkspace(browser, "admin")
        await page.goto(
            `/system/audit?source=business&business_number=${prefix}`,
        )
        await expect(
            page.locator("#operations-audit-source-business"),
        ).toBeVisible()
        const row = page.locator(`[data-row-id="${handoverAudit!.id}"]`)
        await expect(row).toBeVisible()
        await expect(row).toContainText(fixture.adminActor.name)
        await expect(row).toContainText("供给交接")
        await expect(row).toContainText(prefix)
        await expect(row).toContainText("执行成功")
        await expect(row).toContainText("无字段变化")
        await row.click()
        const details = page.getByRole("dialog")
        await expect(details).toContainText(fixture.adminActor.name)
        await expect(details).toContainText("交接结果")
        await expect(details).toContainText("已交接")
        await expect(details).toContainText("责任人")
        await expect(details).toContainText("已变更")
        await details
            .locator("#business-audit-details-technical-trigger")
            .click()
        await expect(details).toContainText(handoverTraceId)
        await expect(details).toContainText(
            handoverAudit!.structured_event.command_id!,
        )
        await expect(details).toContainText(created.offering_id)
        await expect(details).not.toContainText(fixture.body.change_reason)
        await expect(details).not.toContainText(fixture.body.idempotency_key)
    })
})

async function auditOnlyAccount(
    adminToken: string,
    permission: string,
    suffix: string,
) {
    const roleId = await ok<string>(adminToken, "POST", "/admin/roles", {
        name: `E2E ${permission} ${suffix}`,
        permissions: [permission],
    })
    expect(typeof roleId).toBe("string")
    expect(roleId).not.toBe("")
    const roles = await apiGet<Array<{ id: string; permissions: string[] }>>(
        adminToken,
        "/admin/roles",
    )
    expect(roles.find((role) => role.id === roleId)?.permissions).toEqual([
        permission,
    ])
    const account = `e32_${permission === "audit_log:list" ? "b" : "i"}_${suffix}`
    await ok(adminToken, "POST", "/admin/admins", {
        account,
        name: `E2E 审计查询 ${suffix}`,
        password: "123456",
        role_ids: [roleId],
    })
    const saved = (await apiGet<Account[]>(adminToken, "/admin/admins")).find(
        (row) => row.account === account,
    )
    expect(saved).toBeTruthy()
    expect(saved?.role_ids).toEqual([roleId])
    return { account, password: "123456", id: saved!.id }
}

async function loginAuditPage(
    page: Page,
    identity: { account: string; password: string },
    source: string,
) {
    await page.goto(
        `/login?returnTo=${encodeURIComponent(`/system/audit?source=${source}`)}`,
    )
    await page.locator("#governance-auth-login-account").fill(identity.account)
    await page
        .locator("#governance-auth-login-password")
        .fill(identity.password)
    await page.locator("#governance-auth-login-submit").click()
    await page.waitForURL((url) => url.pathname === "/system/audit")
    await expect(
        page.getByRole("heading", { name: "审计查询", exact: true }),
    ).toBeVisible()
}

test("只有业务审计或权限审计权限的账号均可独立查询，不能读取另一类事件或关联业务", async ({
    browser,
}) => {
    const suffix = Date.now().toString(36)
    const adminToken = await apiToken("admin")
    const business = await auditOnlyAccount(
        adminToken,
        "audit_log:list",
        suffix,
    )
    const identity = await auditOnlyAccount(
        adminToken,
        "audit_event:list",
        suffix,
    )
    for (const item of [
        {
            login: identity,
            source: "identity",
            allowed: "/admin/audit-events",
            denied: "/admin/audit-logs",
            hiddenTab: "business",
            table: "operations-audit-events-table",
        },
        {
            login: business,
            source: "business",
            allowed: "/admin/audit-logs",
            denied: "/admin/audit-events",
            hiddenTab: "identity",
            table: "business-audit-table",
        },
    ]) {
        await test.step(`${item.login.account} 仅取得其独立查询权限`, async () => {
            const token = await apiToken(item.login)
            expect(
                (await call(token, "GET", item.allowed)).response.status,
            ).toBe(200)
            for (const endpoint of [
                item.denied,
                "/admin/sales-orders",
                "/admin/roles",
                "/admin/admins",
            ]) {
                expect(
                    (await call(token, "GET", endpoint)).response.status,
                    `${item.login.account} 不应取得 ${endpoint} 权限`,
                ).toBe(403)
            }
            const context = await browser.newContext()
            try {
                const page = await context.newPage()
                const forbiddenReads: string[] = []
                const catalogReads: string[] = []
                page.on("request", (request) => {
                    const path = new URL(request.url()).pathname
                    if (
                        request.method() === "GET" &&
                        ["/admin/roles", "/admin/admins"].includes(path)
                    ) {
                        catalogReads.push(path)
                    }
                })
                page.on("response", (response) => {
                    if (
                        response.request().method() === "GET" &&
                        new URL(response.url()).pathname.startsWith(
                            "/admin/",
                        ) &&
                        response.status() === 403
                    ) {
                        forbiddenReads.push(new URL(response.url()).pathname)
                    }
                })
                await loginAuditPage(page, item.login, item.source)
                expect(new URL(page.url()).searchParams.get("source")).toBe(
                    item.source,
                )
                await expect(
                    page.locator(`#operations-audit-source-${item.hiddenTab}`),
                ).toHaveCount(0)
                await expect(page.locator(`#${item.table}`)).toBeVisible()
                await expect(
                    page.getByText("查询失败", { exact: true }),
                ).toHaveCount(0)
                expect(
                    forbiddenReads,
                    "独立审计页面不得依赖未授予的人员、角色或另一类审计读取权限",
                ).toEqual([])

                const loaded = page.waitForResponse(
                    (response) =>
                        response.request().method() === "GET" &&
                        new URL(response.url()).pathname === item.allowed,
                )
                await page.goto(
                    `/system/audit?source=${item.source}&subjectType=USER&subjectId=${encodeURIComponent(item.login.id)}`,
                )
                const reply = await loaded
                expect(reply.status()).toBe(200)
                expect((await reply.json()).success).not.toBe(false)
                await expect(page.locator(`#${item.table}`)).toBeVisible()
                await expect(page.getByRole("dialog")).toHaveCount(0)
                expect(
                    catalogReads,
                    "审计URL中的真实subjectId不得触发人员或角色目录读取",
                ).toEqual([])
                expect(
                    forbiddenReads,
                    "审计URL不得打开需要额外授权的对象权限解释",
                ).toEqual([])
            } finally {
                await context.close()
            }
        })
    }
})
