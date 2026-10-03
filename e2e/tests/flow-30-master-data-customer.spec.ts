/**
 * 流程: [flow-30] 公共字典、公司主体、供应商修订、仓库 API 与客户归属及敏感资料。
 * 合同: docs/erp-phase-1.md §5、§9.4、§11；组织与权限合同。
 * 所有请求使用隔离运行传入的真实 API，页面写入独立测试资料，不修改固定主数据。
 * 客户经营质量验证当前归属、真实筛选及服务端 CSV；零订单客户不证明财务计算。
 */
import fs from "node:fs/promises"

import { test, expect, type Page, type Response } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { openLoggedInWorkspace } from "../helpers/login"
import { chooseOption, dismissToasts, pickCalendarDay } from "../helpers/ui"

const VISIBLE = { timeout: 20_000 }
type Envelope<T> = { success?: boolean; errorMessage?: string; data: T }
type ApiPage<T> = { items: T[]; total: number }
type Dictionary = {
    id: string
    name: string
    version: number
    status: "active" | "disabled"
    parent_category_id?: string | null
    unit_code?: string
    symbol?: string
    quantity_scale?: number
}
type Company = {
    id: string
    party_no: string
    version: number
    legal_name: string
    short_name: string | null
    aliases: string[]
    status: "active" | "disabled"
}
type CustomerProfile = {
    id: string
    customer_no: string
    version: number
    party_version: number
    owner_user_id: string
    current_revision: { id: string; revision_no: number; legal_name: string }
    assignments: Array<{
        id: string
        user_id: string
        assignment_role: string
        valid_from: string
        valid_to: string | null
        version: number
    }>
    sensitive_fields: Array<{
        kind: string
        record_id: string
        masked_value: string
        reveal_token: string
    }>
}
type Warehouse = {
    id: string
    warehouse_code: string
    status: string
    version: number
    inbound_handler_user_id: string
    outbound_handler_user_id: string
}
type QualityView = {
    scopeVersion: string
    ownershipBasis: string
    totals: { objectCount: number; orderCount: number; grossTotal: string }
    rows: {
        total: number
        items: Array<{
            customerId: string
            customerName: string
            ownerUserId: string
            orderCount: number
            grossTotal: string
        }>
    }
}

function businessDate(offsetDays = 0): string {
    return new Intl.DateTimeFormat("en-CA", {
        timeZone: "Asia/Shanghai",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
    }).format(new Date(Date.now() + offsetDays * 86_400_000))
}

async function command<T>(
    token: string,
    method: "POST" | "PUT" | "DELETE",
    endpoint: string,
    body?: unknown,
): Promise<T> {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method,
        headers: {
            Authorization: `Bearer ${token}`,
            "Content-Type": "application/json",
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
    const result = (await response.json()) as Envelope<T>
    expect(
        response.ok && result.success !== false,
        `${method} ${endpoint}: ${result.errorMessage ?? response.status}`,
    ).toBe(true)
    return result.data
}

async function rejected(
    token: string,
    method: "POST" | "PUT" | "DELETE",
    endpoint: string,
    body: unknown,
    status: number[],
    reason: RegExp,
): Promise<void> {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method,
        headers: {
            Authorization: `Bearer ${token}`,
            "Content-Type": "application/json",
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
    const result = (await response.json()) as Envelope<unknown>
    expect(
        response.ok && result.success !== false,
        `${method} ${endpoint} 应拒绝`,
    ).toBe(false)
    expect(status, result.errorMessage).toContain(response.status)
    expect(result.errorMessage).toMatch(reason)
}

function uiResponse(
    page: Page,
    method: string,
    endpoint: string,
): Promise<Response> {
    return page.waitForResponse(
        (response) =>
            response.request().method() === method &&
            new URL(response.url()).pathname === endpoint,
    )
}

async function resultOf<T>(response: Response): Promise<T> {
    const result = (await response.json()) as Envelope<T>
    expect(response.ok() && result.success !== false, result.errorMessage).toBe(
        true,
    )
    return result.data
}

async function readDictionary(
    token: string,
    endpoint: string,
    id: string,
): Promise<Dictionary> {
    const listed = await apiGet<ApiPage<Dictionary>>(token, endpoint, {
        page_size: 100,
    })
    const item = listed.items.find((row) => row.id === id)
    expect(item, `列表须保留字典 ${id}`).toBeTruthy()
    return item!
}

async function createCategory(
    page: Page,
    name: string,
    code: string,
    child = false,
): Promise<Dictionary> {
    await page
        .locator(
            child
                ? "#master-data-category-workspace-create-child"
                : "#master-data-category-tree-create-root",
        )
        .click()
    const prefix = "#master-data-category-create-dialog"
    await page.locator(`${prefix}-name`).fill(name)
    await page.locator(`${prefix}-code`).fill(code)
    await chooseOption(page, page.locator(`${prefix}-product-kind`), "实物")
    await page.locator(`${prefix}-change-reason`).fill("E2E 分类树维护")
    const saved = uiResponse(page, "POST", "/admin/product-categories")
    await page.locator(`${prefix}-submit`).click()
    const category = await resultOf<Dictionary>(await saved)
    await expect(page.locator(`${prefix}-name`)).toBeHidden(VISIBLE)
    await dismissToasts(page)
    return category
}

test.describe.serial("[flow-30] 基础资料及客户治理", () => {
    const suffix = `${Date.now().toString(36)}-${process.pid}`
    let adminToken: string
    let company: Company
    let customer: CustomerProfile
    let salesUsers: Array<{ id: string; account: string; name: string }>
    const phone = "13800138068"
    const address = `上海市测试路 30 号 ${suffix}`
    const bankAccount = "6222020200003068"
    const customerName = `E2E 客户治理 ${suffix}`

    test.beforeAll(async () => {
        adminToken = await apiToken("admin")
        salesUsers = await apiGet<typeof salesUsers>(
            adminToken,
            "/admin/admins",
        )
        expect(salesUsers.some((user) => user.account === "xiaoshou")).toBe(
            true,
        )
        expect(salesUsers.some((user) => user.account === "lisiyong")).toBe(
            true,
        )
    })

    test("品牌和计量单位：页面建档、更新、停用、唯一代码和过期版本拒绝", async ({
        browser,
    }) => {
        const { context, page } = await openLoggedInWorkspace(browser, "admin")
        try {
            for (const resource of ["brands", "unit-of-measures"] as const) {
                await test.step(
                    resource === "brands" ? "品牌" : "计量单位",
                    async () => {
                        const endpoint =
                            resource === "brands"
                                ? "/admin/product-brands"
                                : "/admin/unit-of-measures"
                        const base =
                            resource === "brands"
                                ? "master-data-brand"
                                : "master-data-unit-of-measures-list"
                        const code = `E2E-${resource === "brands" ? "BR" : "UOM"}-${suffix}`
                        const name = `E2E ${resource === "brands" ? "品牌" : "单位"} ${suffix}`
                        await page.goto(`/master-data/${resource}`)
                        await page
                            .locator(`#master-data-${resource}-list-create`)
                            .click()
                        const createPrefix = `#${base}-create-dialog`
                        await page.locator(`${createPrefix}-name`).fill(name)
                        await page.locator(`${createPrefix}-code`).fill(code)
                        if (resource === "unit-of-measures") {
                            await page
                                .locator(`${createPrefix}-symbol`)
                                .fill("组")
                            await chooseOption(
                                page,
                                page.locator(`${createPrefix}-quantity-scale`),
                                "2",
                            )
                        }
                        await page
                            .locator(`${createPrefix}-change-reason`)
                            .fill("E2E 独立字典资料")
                        const creating = uiResponse(page, "POST", endpoint)
                        await page.locator(`${createPrefix}-submit`).click()
                        const created = await resultOf<Dictionary>(
                            await creating,
                        )
                        expect(created).toMatchObject({
                            name,
                            status: "active",
                        })
                        if (resource === "unit-of-measures")
                            expect(created).toMatchObject({
                                symbol: "组",
                                quantity_scale: 2,
                            })
                        await expect(
                            page.locator(`${createPrefix}-name`),
                        ).toBeHidden(VISIBLE)

                        await rejected(
                            adminToken,
                            "POST",
                            endpoint,
                            {
                                ...(resource === "brands"
                                    ? { brand_code: code }
                                    : {
                                          unit_code: code,
                                          symbol: "组",
                                          quantity_scale: 2,
                                      }),
                                name: `${name} 重复`,
                                status: "active",
                            },
                            [400, 409, 422],
                            /已存在|重复|duplicate|唯一/i,
                        )
                        await dismissToasts(page)
                        await page
                            .locator(`button[data-row-id="${created.id}"]`)
                            .click()
                        await page
                            .locator(`#master-data-${resource}-preview-revise`)
                            .click()
                        const revisePrefix = `#${base}-revise-dialog`
                        await page
                            .locator(`${revisePrefix}-name`)
                            .fill(`${name} 更新`)
                        if (resource === "unit-of-measures") {
                            await expect(
                                page.locator(`${revisePrefix}-code`),
                            ).toBeDisabled()
                            await expect(
                                page.locator(`${revisePrefix}-code`),
                            ).toHaveValue(code)
                            await page
                                .locator(`${revisePrefix}-symbol`)
                                .fill("套")
                        }
                        await page
                            .locator(`${revisePrefix}-change-reason`)
                            .fill("E2E 更新字典名称")
                        const updating = uiResponse(
                            page,
                            "PUT",
                            `${endpoint}/${created.id}`,
                        )
                        await page.locator(`${revisePrefix}-submit`).click()
                        const updated = await resultOf<Dictionary>(
                            await updating,
                        )
                        expect(updated).toMatchObject({
                            id: created.id,
                            name: `${name} 更新`,
                            status: "active",
                        })
                        expect(updated.version).toBeGreaterThan(created.version)
                        await rejected(
                            adminToken,
                            "PUT",
                            `${endpoint}/${created.id}`,
                            {
                                version: created.version,
                                name: "过期写入不得覆盖",
                            },
                            [409],
                            /版本|冲突|数据已被其他请求修改|version|conflict/i,
                        )
                        expect(
                            (
                                await readDictionary(
                                    adminToken,
                                    endpoint,
                                    created.id,
                                )
                            ).name,
                        ).toBe(`${name} 更新`)

                        await page.goto(
                            `/master-data/${resource}?q=${encodeURIComponent(code)}`,
                        )
                        await page
                            .locator(`button[data-row-id="${created.id}"]`)
                            .click()
                        await page
                            .locator(`#master-data-${resource}-preview-disable`)
                            .click()
                        const dialog = page.getByRole("dialog").last()
                        await dialog
                            .locator("#master-data-shared-disable-reason")
                            .fill("E2E 停用独立字典")
                        const disabling = uiResponse(
                            page,
                            "PUT",
                            `${endpoint}/${created.id}`,
                        )
                        await dialog
                            .getByRole("button", {
                                name: "确认停用",
                                exact: true,
                            })
                            .click()
                        const disabled = await resultOf<Dictionary>(
                            await disabling,
                        )
                        expect(disabled.status).toBe("disabled")
                        await page.goto(
                            `/master-data/${resource}?q=${encodeURIComponent(code)}&lifecycleStatus=disabled`,
                        )
                        await expect(
                            page.locator(`button[data-row-id="${created.id}"]`),
                        ).toContainText("停用")
                        await command(
                            adminToken,
                            "DELETE",
                            `${endpoint}/${created.id}`,
                        )
                        const deleted = await apiGet<ApiPage<Dictionary>>(
                            adminToken,
                            endpoint,
                            {
                                q: code,
                                page_size: 100,
                            },
                        )
                        expect(
                            deleted.items.some((row) => row.id === created.id),
                        ).toBe(false)
                    },
                )
            }
        } finally {
            await context.close()
        }
    })

    test("商品分类：页面父子建档、原子名称及父级更新、防环和子树删除保护", async ({
        browser,
    }) => {
        const { context, page } = await openLoggedInWorkspace(browser, "admin")
        try {
            await page.goto("/master-data/categories")
            const first = await createCategory(
                page,
                `E2E 一级分类 ${suffix}`,
                `E2E-C1-${suffix}`,
            )
            await expect(page).toHaveURL(new RegExp(`category=${first.id}`))
            const child = await createCategory(
                page,
                `E2E 子分类 ${suffix}`,
                `E2E-CC-${suffix}`,
                true,
            )
            expect(child.parent_category_id).toBe(first.id)
            await rejected(
                adminToken,
                "DELETE",
                `/admin/product-categories/${first.id}`,
                undefined,
                [400, 409, 422],
                /子分类|不能删除/,
            )
            const secondName = `E2E 二级根 ${suffix}`
            const second = await createCategory(
                page,
                secondName,
                `E2E-C2-${suffix}`,
            )

            await page.goto(`/master-data/categories?category=${child.id}`)
            await page.locator("#master-data-category-workspace-edit").click()
            const prefix = "#master-data-category-revise-dialog"
            await expect(page.locator(`${prefix}-code`)).toHaveAttribute(
                "readonly",
                "",
            )
            await page
                .locator(`${prefix}-name`)
                .fill(`E2E 子分类更新 ${suffix}`)
            await chooseOption(
                page,
                page.locator(`${prefix}-parent`),
                secondName,
            )
            await page
                .locator(`${prefix}-change-reason`)
                .fill("E2E 同时更新名称与上级")
            const updating = uiResponse(
                page,
                "PUT",
                `/admin/product-categories/${child.id}`,
            )
            await page.locator(`${prefix}-submit`).click()
            const updated = await resultOf<Dictionary>(await updating)
            expect(updated).toMatchObject({
                id: child.id,
                parent_category_id: second.id,
                name: `E2E 子分类更新 ${suffix}`,
            })
            await expect(
                page.locator("#category-workspace-scroll"),
            ).toContainText(secondName)
            await rejected(
                adminToken,
                "PUT",
                `/admin/product-categories/${second.id}`,
                {
                    version: second.version,
                    parent_change: { parent_category_id: child.id },
                },
                [400, 409, 422],
                /环|自身|下级/,
            )
            expect(
                (
                    await readDictionary(
                        adminToken,
                        "/admin/product-categories",
                        second.id,
                    )
                ).parent_category_id,
            ).toBeNull()
            await command(
                adminToken,
                "DELETE",
                `/admin/product-categories/${first.id}`,
            )
            await rejected(
                adminToken,
                "DELETE",
                `/admin/product-categories/${second.id}`,
                undefined,
                [400, 409, 422],
                /子分类|不能删除/,
            )
            await command(
                adminToken,
                "DELETE",
                `/admin/product-categories/${child.id}`,
            )
            await command(
                adminToken,
                "DELETE",
                `/admin/product-categories/${second.id}`,
            )
        } finally {
            await context.close()
        }
    })

    test("公司主体：页面建档、别名编辑、状态切换和版本冲突不覆盖", async ({
        browser,
    }) => {
        const { context, page } = await openLoggedInWorkspace(browser, "admin")
        try {
            await page.goto("/master-data/companies")
            await page.locator("#companies-create").click()
            await page
                .locator("#company-form-legal-name")
                .fill(`E2E 公司主体 ${suffix}`)
            await page
                .locator("#company-form-short-name")
                .fill(`E2E 公司 ${suffix}`)
            await page
                .locator("#company-form-aliases")
                .fill("测试公司甲、测试公司乙")
            const creating = uiResponse(page, "POST", "/admin/companies")
            await page.locator("#company-form-submit").click()
            company = await resultOf<Company>(await creating)
            expect([...company.aliases].sort()).toEqual(
                ["测试公司甲", "测试公司乙"].sort(),
            )
            const row = page
                .getByRole("row")
                .filter({ hasText: company.legal_name })
            await row.locator('[id^="company-edit-"]').click()
            await page
                .locator("#company-form-short-name")
                .fill(`E2E 公司修订 ${suffix}`)
            await page
                .locator("#company-form-aliases")
                .fill("测试公司乙、测试公司丙")
            const updating = uiResponse(
                page,
                "PUT",
                `/admin/companies/${company.id}`,
            )
            await page.locator("#company-form-submit").click()
            const updated = await resultOf<Company>(await updating)
            expect([...updated.aliases].sort()).toEqual(
                ["测试公司乙", "测试公司丙"].sort(),
            )
            expect(updated.version).toBeGreaterThan(company.version)
            const { id: companyId, ...originalCompany } = company
            await rejected(
                adminToken,
                "PUT",
                `/admin/companies/${companyId}`,
                { ...originalCompany, legal_name: "过期公司资料" },
                [409],
                /版本|冲突|数据已被其他请求修改|version|conflict/i,
            )
            const disabling = uiResponse(
                page,
                "PUT",
                `/admin/companies/${company.id}`,
            )
            await row.locator('[id^="company-status-"]').click()
            expect((await resultOf<Company>(await disabling)).status).toBe(
                "disabled",
            )
            await expect(row).toContainText("停用")
            const enabling = uiResponse(
                page,
                "PUT",
                `/admin/companies/${company.id}`,
            )
            await row.locator('[id^="company-status-"]').click()
            company = await resultOf<Company>(await enabling)
            expect(company.status).toBe("active")
            expect(company.short_name).toBe(`E2E 公司修订 ${suffix}`)
        } finally {
            await context.close()
        }
    })

    test("供应商资料：页面修订保留商业历史、公司主体和既有敏感资料", async ({
        browser,
    }) => {
        const maintainer = salesUsers.find((user) => user.account === "caigou")!
        expect(maintainer).toBeTruthy()
        const commandBody = {
            idempotency_key: `e2e-supplier-${suffix}`,
            party_no: `E2E-PTY-${suffix}`,
            supplier_no: `E2E-SUP-${suffix}`,
            legal_name: `E2E 供应商主体 ${suffix}`,
            short_name: `E2E 供应商 ${suffix}`,
            contact: { contact_name: "供货联系人", mobile: phone },
            address: { address, contact_name: "供货联系人" },
            bank_account: {
                bank_name: "测试开户行",
                account_number: bankAccount,
            },
            settlement_mode: "prepayment",
            reconciliation_cycle: "none",
            payment_term_snapshot: "PREPAY_50",
            invoice_type: "vat_special",
            invoice_tax_rates: ["0.130000"],
            signing_entity_party_id: company.id,
            payment_entity_party_id: company.id,
            maintainer_user_id: maintainer.id,
            capability_codes: [],
            capability_owners: [],
            qualifications: [],
            effective_from: businessDate(),
            change_reason: "E2E 供应商根级建档",
        }
        const created = await command<{
            supplier_id: string
            revision_id: string
            revision_no: number
        }>(adminToken, "POST", "/admin/supplier-profiles", commandBody)
        type SupplierDetail = {
            id: string
            short_name: string
            party_version: number
            version: number
            contacts: Array<{ id: string }>
            addresses: Array<{ id: string }>
            bank_accounts: Array<{ id: string }>
            current_profile: {
                id: string
                revision_no: number
                payment_term_snapshot: string
                signing_entity_party_id: string
                payment_entity_party_id: string
            }
            commercial_profiles: Array<{
                id: string
                revision_no: number
                payment_term_snapshot: string
            }>
        }
        const before = await apiGet<SupplierDetail>(
            adminToken,
            `/admin/suppliers/${created.supplier_id}`,
        )
        const { context, page } = await openLoggedInWorkspace(browser, "admin")
        try {
            await page.goto(`/master-data/suppliers/${created.supplier_id}`)
            await expect(
                page.locator("#master-data-supplier-basic-name"),
            ).toHaveValue(commandBody.short_name)
            await page
                .locator("#master-data-supplier-basic-name")
                .fill(`${commandBody.short_name} 更新`)
            await page
                .locator("#master-data-supplier-editor-tab-commercial")
                .click()
            await chooseOption(
                page,
                page.locator(
                    "#master-data-supplier-commercial-payment-term-combobox",
                ),
                "先款 30%",
            )
            await page
                .locator("#master-data-supplier-document-header-submit")
                .click()
            await page
                .locator("#supplier-save-reason")
                .fill("E2E 供应商商务条款修订")
            const updating = uiResponse(
                page,
                "PUT",
                `/admin/supplier-profiles/${created.supplier_id}`,
            )
            await page
                .locator(
                    "#master-data-supplier-supplier-save-reason-dialog-button-2",
                )
                .click()
            const saved = await resultOf<{
                revision_id: string
                revision_no: number
            }>(await updating)
            expect(saved.revision_id).not.toBe(created.revision_id)
            expect(saved.revision_no).toBeGreaterThan(created.revision_no)
            const after = await apiGet<SupplierDetail>(
                adminToken,
                `/admin/suppliers/${created.supplier_id}`,
            )
            expect(after.short_name).toBe(`${commandBody.short_name} 更新`)
            expect(after.current_profile).toMatchObject({
                id: saved.revision_id,
                payment_term_snapshot: "PREPAY_30",
                signing_entity_party_id: company.id,
                payment_entity_party_id: company.id,
            })
            expect(
                after.commercial_profiles.find(
                    (revision) => revision.id === created.revision_id,
                )?.payment_term_snapshot,
            ).toBe("PREPAY_50")
            for (const field of [
                "contacts",
                "addresses",
                "bank_accounts",
            ] as const) {
                expect(after[field].map((item) => item.id)).toEqual(
                    before[field].map((item) => item.id),
                )
            }
            await rejected(
                adminToken,
                "PUT",
                `/admin/supplier-profiles/${created.supplier_id}`,
                {
                    ...commandBody,
                    party_no: null,
                    supplier_no: null,
                    idempotency_key: `e2e-supplier-stale-${suffix}`,
                    expected_party_version: before.party_version,
                    expected_supplier_version: before.version,
                },
                [409],
                /版本|冲突|数据已被其他请求修改|version|conflict/i,
            )
            await page.reload()
            await expect(
                page.locator("#master-data-supplier-basic-name"),
            ).toHaveValue(`${commandBody.short_name} 更新`)
            await expect(
                page.locator("#master-data-supplier-basic-company"),
            ).toHaveValue(commandBody.legal_name)
        } finally {
            await context.close()
        }
    })

    test("仓库 API：建档、修订、收发责任资格和过期版本拒绝", async () => {
        const options = await apiGet<
            Array<{
                user_id: string
                account: string
                inbound_eligible: boolean
                outbound_eligible: boolean
            }>
        >(adminToken, "/admin/warehouse-fulfillment-handler-options")
        const handler = options.find(
            (item) =>
                item.account === "cangchu" &&
                item.inbound_eligible &&
                item.outbound_eligible,
        )
        expect(handler, "固定仓储账号须具备收发完整执行权限").toBeTruthy()
        const input = {
            warehouse_code: `E2E-WH-${suffix}`,
            name: `E2E 仓库 ${suffix}`,
            address,
            contact: "仓库经办人",
            effective_from: businessDate(-1),
            change_reason: "E2E 独立仓库",
            status: "active",
            inbound_handler_user_id: handler!.user_id,
            outbound_handler_user_id: handler!.user_id,
        }
        const created = await command<Warehouse>(
            adminToken,
            "POST",
            "/admin/warehouses",
            input,
        )
        const revisions = await apiGet<
            ApiPage<{ id: string; name: string; revision_no: number }>
        >(adminToken, "/admin/warehouse-revisions", {
            warehouse_id: created.id,
        })
        expect(revisions.items).toHaveLength(1)
        expect(revisions.items[0].name).toBe(input.name)
        const updated = await command<Warehouse>(
            adminToken,
            "PUT",
            `/admin/warehouses/${created.id}`,
            {
                ...input,
                version: created.version,
                name: `${input.name} 更新`,
                effective_from: businessDate(),
                change_reason: "E2E 仓库资料修订",
                status: "disabled",
            },
        )
        expect(updated).toMatchObject({
            id: created.id,
            status: "disabled",
            inbound_handler_user_id: handler!.user_id,
        })
        const after = await apiGet<
            ApiPage<{ id: string; name: string; revision_no: number }>
        >(adminToken, "/admin/warehouse-revisions", {
            warehouse_id: created.id,
        })
        expect(after.items).toHaveLength(2)
        expect(
            after.items.find(
                (revision) => revision.id === revisions.items[0].id,
            )?.name,
        ).toBe(input.name)
        expect(
            after.items.some(
                (revision) => revision.name === `${input.name} 更新`,
            ),
        ).toBe(true)
        await rejected(
            adminToken,
            "PUT",
            `/admin/warehouses/${created.id}`,
            { ...input, version: created.version },
            [409],
            /版本|冲突|数据已被其他请求修改|version|conflict/i,
        )
        const sales = salesUsers.find((user) => user.account === "xiaoshou")!
        await rejected(
            adminToken,
            "PUT",
            `/admin/warehouses/${created.id}/fulfillment-handlers`,
            {
                version: updated.version,
                inbound_handler_user_id: sales.id,
                outbound_handler_user_id: sales.id,
            },
            [400, 409, 422],
            /权限|资格|经办|入库|发货/,
        )
        const list = await apiGet<ApiPage<Warehouse>>(
            adminToken,
            "/admin/warehouses",
            {
                warehouse_code: input.warehouse_code,
            },
        )
        expect(list.items.find((item) => item.id === created.id)).toMatchObject(
            {
                version: updated.version,
                inbound_handler_user_id: handler!.user_id,
            },
        )
    })

    test("客户资料：真实揭示、无权限账号令牌拒绝、页面换任及历史归属保留", async ({
        browser,
    }) => {
        const salesToken = await apiToken("xiaoshou")
        const created = await command<{ customer_id: string }>(
            salesToken,
            "POST",
            "/admin/customer-profiles",
            {
                idempotency_key: `e2e-customer-governance-${suffix}`,
                legal_name: customerName,
                default_payment_term_id: "POSTPAY_NET15",
                status: "active",
                effective_from: businessDate(-1),
                change_reason: "E2E 客户治理独立资料",
                contacts: [
                    {
                        contact_name: "测试联系人",
                        mobile: phone,
                        is_default: true,
                    },
                ],
                addresses: [
                    { address_type: "operating", address, is_default: true },
                ],
            },
        )
        customer = await apiGet<CustomerProfile>(
            adminToken,
            `/admin/customer-profiles/${created.customer_id}`,
        )
        const oldOwner = salesUsers.find((user) => user.account === "xiaoshou")!
        const nextOwner = salesUsers.find(
            (user) => user.account === "lisiyong",
        )!
        expect(customer.owner_user_id).toBe(oldOwner.id)
        const ownerAssignment = customer.assignments.find(
            (assignment) =>
                assignment.assignment_role === "OWNER" &&
                assignment.valid_to == null,
        )!
        expect(ownerAssignment).toBeTruthy()
        const sensitivePhone = customer.sensitive_fields.find(
            (field) => field.kind === "contact_mobile",
        )!
        const sensitiveAddress = customer.sensitive_fields.find(
            (field) => field.kind === "address",
        )!
        expect(sensitivePhone?.masked_value).not.toBe(phone)
        expect(sensitivePhone?.reveal_token).toBeTruthy()
        expect(sensitiveAddress?.reveal_token).toBeTruthy()
        await rejected(
            await apiToken("cangchu"),
            "POST",
            "/admin/customer-sensitive-fields/reveal",
            { reveal_token: sensitivePhone.reveal_token },
            [400, 403, 422],
            /令牌|权限|账号|用户|token/i,
        )
        await rejected(
            adminToken,
            "POST",
            `/admin/customers/${customer.id}/assignments`,
            {
                action: "end",
                assignment_id: ownerAssignment.id,
                valid_to: businessDate(),
                change_reason: "不允许单独结束 OWNER",
                version: ownerAssignment.version,
            },
            [400, 409, 422],
            /负责人|OWNER|换任|替换|结束/,
        )

        const { context, page } = await openLoggedInWorkspace(browser, "admin")
        try {
            await page.goto(`/sales/customers/${customer.id}`)
            const phoneValue = page
                .locator('[data-slot="sensitive-value"]')
                .filter({ hasText: "测试联系人手机：" })
            await expect(phoneValue.locator("code")).not.toHaveText(phone)
            const revealing = uiResponse(
                page,
                "POST",
                "/admin/customer-sensitive-fields/reveal",
            )
            await phoneValue.getByRole("button").click()
            expect(
                (await resultOf<{ value: string }>(await revealing)).value,
            ).toBe(phone)
            await expect(phoneValue.locator("code")).toHaveText(phone)
            await phoneValue
                .getByRole("button", {
                    name: "隐藏测试联系人手机",
                    exact: true,
                })
                .click()
            await expect(phoneValue.locator("code")).not.toHaveText(phone)
            const addressValue = page
                .locator('[data-slot="sensitive-value"]')
                .filter({ hasText: "经营地址：" })
            await addressValue.getByRole("button").click()
            await expect(addressValue.locator("code")).toHaveText(address)

            await page.locator("#customers-detail-tab-audit").click()
            await page
                .locator("#customers-detail-audit-manage-assignments")
                .click()
            await chooseOption(
                page,
                page.locator("#customers-assignment-dialog-owner"),
                "李思勇",
            )
            await page.locator("#customer-assignment-from").fill(businessDate())
            await page
                .locator("#customers-assignment-dialog-reason")
                .fill("E2E 调整客户负责销售")
            const assigning = uiResponse(
                page,
                "POST",
                `/admin/customers/${customer.id}/assignments`,
            )
            await page.locator("#customers-assignment-dialog-submit").click()
            await resultOf(await assigning)
            await expect(
                page.locator("#customers-assignment-dialog-submit"),
            ).toBeHidden(VISIBLE)
            customer = await apiGet<CustomerProfile>(
                adminToken,
                `/admin/customer-profiles/${customer.id}`,
            )
            expect(customer.owner_user_id).toBe(nextOwner.id)
            const activeOwners = customer.assignments.filter(
                (assignment) =>
                    assignment.assignment_role === "OWNER" &&
                    assignment.valid_to == null,
            )
            expect(activeOwners).toHaveLength(1)
            expect(activeOwners[0].user_id).toBe(nextOwner.id)
            expect(
                customer.assignments.find(
                    (assignment) => assignment.id === ownerAssignment.id,
                )?.valid_to,
            ).toBe(businessDate())
            await expect(
                page.getByText("李思勇", { exact: true }).first(),
            ).toBeVisible()
        } finally {
            await context.close()
        }
    })

    test("客户经营质量：真实客户筛选、现任负责人、无订单指标和完整服务端 CSV", async ({
        browser,
    }) => {
        const { context, page } = await openLoggedInWorkspace(browser, "admin")
        try {
            const from = `${businessDate().slice(0, 4)}-01-01`
            const to = businessDate()
            const opening = page.waitForResponse((response) => {
                const url = new URL(response.url())
                return (
                    url.pathname === "/admin/customer-quality/current" &&
                    url.searchParams.get("customer_id") === customer.id &&
                    response.ok()
                )
            })
            await page.goto(
                `/analytics/customer-quality?from=${from}&to=${to}&dualCustomerId=${customer.id}`,
            )
            const view = await resultOf<QualityView>(await opening)
            expect(view.rows.total).toBe(1)
            expect(view.rows.items[0]).toMatchObject({
                customerId: customer.id,
                customerName,
                ownerUserId: customer.owner_user_id,
                orderCount: 0,
                grossTotal: "0.00",
            })
            expect(view.totals).toMatchObject({
                objectCount: 1,
                orderCount: 0,
                grossTotal: "0.00",
            })
            const qualitySection = page.getByRole("region", {
                name: "客户经营质量双口径",
            })
            await expect(
                qualitySection
                    .getByRole("row")
                    .filter({ hasText: customerName }),
            ).toContainText("李思勇")
            const download = page.waitForEvent("download")
            const exporting = uiResponse(
                page,
                "POST",
                "/admin/customer-quality/current/exports",
            )
            await page.locator("#customers-quality-dual-current-export").click()
            const file = await resultOf<{
                rowCount: number
                csvContent: string
                fileName: string
            }>(await exporting)
            expect(file.rowCount).toBe(1)
            expect(file.csvContent).toContain("现任负责人")
            expect(file.csvContent).toContain(customerName)
            expect(file.csvContent).toContain("李思勇")
            expect(file.csvContent).toContain("0.00")
            const downloaded = await download
            expect(downloaded.suggestedFilename()).toBe(file.fileName)
            const downloadedPath = await downloaded.path()
            expect(downloadedPath).toBeTruthy()
            expect(await fs.readFile(downloadedPath!, "utf8")).toBe(
                `\uFEFF${file.csvContent}`,
            )

            const noResultQuery = `无匹配客户-${suffix}`
            await page
                .locator("#customers-quality-dual-search")
                .fill(noResultQuery)
            const searching = page.waitForResponse((response) => {
                const url = new URL(response.url())
                return (
                    url.pathname === "/admin/customer-quality/current" &&
                    url.searchParams.get("q") === noResultQuery
                )
            })
            await page.locator("#customers-quality-dual-apply").click()
            expect(
                (await resultOf<QualityView>(await searching)).rows.total,
            ).toBe(0)
            await expect(
                qualitySection.getByText("当前筛选无客户结果", { exact: true }),
            ).toBeVisible()
            await expect(
                page.locator("#customers-quality-dual-current-export"),
            ).toBeDisabled()
            await expect(page).toHaveURL(
                new RegExp(`dualQ=${encodeURIComponent(noResultQuery)}`),
            )
            await page.locator("#customers-quality-dual-clear").click()
            await expect(
                page.locator("#customers-quality-dual-search"),
            ).toHaveValue("")

            await page.goto(
                `/analytics/customer-quality?dualCustomerId=${customer.id}`,
            )
            await expect(
                page.getByText("期间配置加载失败", { exact: true }),
            ).toBeVisible()
            await expect(qualitySection).toBeHidden()
            await expect(
                page.locator("#customers-quality-blocker-apply"),
            ).toBeDisabled()
            await pickCalendarDay(
                page,
                page.locator("#customers-quality-blocker-from"),
                to,
            )
            await pickCalendarDay(
                page,
                page.locator("#customers-quality-blocker-to"),
                to,
            )
            const explicitPeriodQuery = page.waitForResponse((response) => {
                const url = new URL(response.url())
                return (
                    url.pathname === "/admin/customer-quality/current" &&
                    url.searchParams.get("from") === to &&
                    url.searchParams.get("to") === to &&
                    url.searchParams.get("customer_id") === customer.id
                )
            })
            await page.locator("#customers-quality-blocker-apply").click()
            expect(
                (await resultOf<QualityView>(await explicitPeriodQuery)).rows
                    .items[0],
            ).toMatchObject({
                customerId: customer.id,
                ownerUserId: customer.owner_user_id,
            })
            await expect(page).toHaveURL(new RegExp(`from=${to}`))
            await expect(page).toHaveURL(new RegExp(`to=${to}`))
            await expect(
                page.getByText("期间配置加载失败", { exact: true }),
            ).toBeVisible()
            await expect(
                qualitySection
                    .getByRole("row")
                    .filter({ hasText: customerName }),
            ).toContainText("李思勇")

            const deniedToken = await apiToken("cangchu")
            await rejected(
                deniedToken,
                "POST",
                "/admin/customer-quality/current/exports",
                {
                    from,
                    to,
                    dimension: "customer",
                    sort: "orderCount:desc",
                    page: 1,
                    page_size: 20,
                    customer_id: customer.id,
                },
                [403],
                /权限|禁止|forbidden/i,
            )
        } finally {
            await context.close()
        }
    })
})
