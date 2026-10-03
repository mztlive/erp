/**
 * 财务与采购责任配置：页面维护、服务端资格、唯一性、版本和逐行预览。
 * 规则在隔离数据库内修改；不把规则配置等同于正式任务的责任事实。
 */
import { expect, test, type Page, type Response } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { openLoggedInWorkspace } from "../helpers/login"
import { createCustomerViaUi } from "../helpers/customers"
import { chooseOption, dismissToasts } from "../helpers/ui"

type Rule = {
    id: string
    version: number
    status: string
    owner_user_id: string
    scope?: string
    counterparty_id?: string
    sku_id?: string
    rule_type?: string
}
type Owner = {
    user_id: string
    account: string
    supplier_payment_eligible: boolean
    sales_invoice_eligible: boolean
}
type Resolution = {
    lines: Array<{
        line_key: string
        resolved: boolean
        rule_id?: string
        rule_type?: string
        owner_user_id?: string
        error?: string
    }>
}

async function command<T>(
    token: string,
    method: string,
    endpoint: string,
    body: unknown,
): Promise<T> {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method,
        headers: {
            Authorization: `Bearer ${token}`,
            "Content-Type": "application/json",
        },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
    const result = (await response.json()) as {
        success: boolean
        errorMessage?: string
        data: T
    }
    expect(
        response.ok && result.success,
        `${method} ${endpoint}: ${result.errorMessage}`,
    ).toBe(true)
    return result.data
}

async function rejected(
    token: string,
    method: string,
    endpoint: string,
    body: unknown,
    statuses: number[],
): Promise<void> {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method,
        headers: {
            Authorization: `Bearer ${token}`,
            "Content-Type": "application/json",
        },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(20_000),
    })
    const result = (await response.json()) as {
        success: boolean
        errorMessage?: string
    }
    expect(statuses, result.errorMessage).toContain(response.status)
    expect(result.success).toBe(false)
    expect(result.errorMessage).toBeTruthy()
}

async function uiResult<T>(response: Response): Promise<T> {
    const result = (await response.json()) as {
        success: boolean
        errorMessage?: string
        data: T
    }
    expect(response.ok() && result.success, result.errorMessage).toBe(true)
    return result.data
}

async function saveRule(
    page: Page,
    method: string,
    endpoint: string,
    buttonId: string,
): Promise<Rule> {
    const [response] = await Promise.all([
        page.waitForResponse(
            (item) =>
                item.request().method() === method &&
                new URL(item.url()).pathname === endpoint,
        ),
        page.locator(`#${buttonId}`).click(),
    ])
    const rule = await uiResult<Rule>(response)
    await expect(page.locator('[data-slot="dialog-content"]')).toBeHidden()
    await dismissToasts(page)
    return rule
}

test("[flow-31] 财务责任规则页面维护、执行资格和服务端版本保护", async ({
    browser,
}) => {
    const { page } = await openLoggedInWorkspace(browser, "admin")
    const token = await apiToken("admin")
    const [owners, rules, suppliers, accounts] = await Promise.all([
        apiGet<Owner[]>(token, "/admin/finance-responsibility-owner-options"),
        apiGet<Rule[]>(token, "/admin/finance-responsibility-rules"),
        apiGet<{ items: Array<{ id: string; supplier_no: string }> }>(
            token,
            "/admin/suppliers",
            { page_size: 100 },
        ),
        apiGet<Array<{ id: string; account: string }>>(token, "/admin/admins"),
    ])
    const owner = owners.find((item) => item.account === "fukuan")
    const sales = accounts.find((item) => item.account === "xiaoshou")
    const supplier = suppliers.items.find(
        (item) =>
            ["SUP-HZSF", "SUP-DEV-WEEK"].includes(item.supplier_no) &&
            !rules.some((rule) => rule.counterparty_id === item.id),
    )
    expect(owner?.supplier_payment_eligible).toBe(true)
    expect(
        owners.some(
            (item) =>
                item.account === "xiaoshou" && item.supplier_payment_eligible,
        ),
    ).toBe(false)
    expect(sales).toBeTruthy()
    expect(supplier, "种子应提供尚未设置精确付款责任的供应商").toBeTruthy()
    const input = {
        operation: "SUPPLIER_PAYMENT",
        scope: "COUNTERPARTY",
        counterparty_id: supplier!.id,
        owner_user_id: owner!.user_id,
        status: "active",
    }

    await page.goto("/finance/responsibilities")
    await page.locator("#finance-responsibilities-create").click()
    await chooseOption(
        page,
        page.locator("#finance-responsibilities-rule-counterparty"),
        new RegExp(supplier!.supplier_no),
        supplier!.supplier_no,
    )
    await chooseOption(
        page,
        page.locator("#finance-responsibilities-rule-owner"),
        /fukuan/,
    )
    const created = await saveRule(
        page,
        "POST",
        "/admin/finance-responsibility-rules",
        "finance-responsibilities-rule-submit",
    )
    expect(created).toMatchObject({ ...input, version: 1 })

    await test.step("重复规则、无执行资格和越权均被阻断且不会增加记录", async () => {
        await rejected(
            token,
            "POST",
            "/admin/finance-responsibility-rules",
            input,
            [400, 409, 422],
        )
        await rejected(
            token,
            "PUT",
            `/admin/finance-responsibility-rules/${created.id}`,
            { ...input, owner_user_id: sales!.id, version: created.version },
            [400, 409, 422],
        )
        await rejected(
            await apiToken("xiaoshou"),
            "POST",
            "/admin/finance-responsibility-rules",
            input,
            [403],
        )
        const after = await apiGet<Rule[]>(
            token,
            "/admin/finance-responsibility-rules",
        )
        expect(after).toHaveLength(rules.length + 1)
        expect(after.find((item) => item.id === created.id)).toMatchObject({
            owner_user_id: owner!.user_id,
            version: 1,
            status: "active",
        })
    })

    await page
        .getByRole("row")
        .filter({ hasText: supplier!.supplier_no })
        .click()
    await page.locator("#finance-responsibilities-rule-enabled").click()
    const disabled = await saveRule(
        page,
        "PUT",
        `/admin/finance-responsibility-rules/${created.id}`,
        "finance-responsibilities-rule-submit",
    )
    expect(disabled).toMatchObject({
        id: created.id,
        status: "disabled",
        version: 2,
    })
    await rejected(
        token,
        "PUT",
        `/admin/finance-responsibility-rules/${created.id}`,
        { ...input, version: 1 },
        [400, 409],
    )
    expect(
        (
            await apiGet<Rule[]>(token, "/admin/finance-responsibility-rules")
        ).find((item) => item.id === created.id),
    ).toMatchObject({ status: "disabled", version: 2 })

    await test.step("客户精确开票规则可保存，停用客户拒绝新规则且保留历史编号", async () => {
        const name = `E2E 开票责任客户 ${Date.now()}`
        await createCustomerViaUi(page, {
            legalName: name,
            shortName: name,
            paymentTermLabel: "货到 15 天",
        })
        const customers = await apiGet<{
            items: Array<{ id: string; customer_no: string; version: number }>
        }>(token, "/admin/customers", { keyword: name })
        expect(customers.items).toHaveLength(1)
        const customer = customers.items[0]!
        const invoiceOwner = owners.find((item) => item.account === "kaipiao")
        expect(invoiceOwner?.sales_invoice_eligible).toBe(true)
        const invoiceInput = {
            operation: "SALES_INVOICE",
            scope: "COUNTERPARTY",
            counterparty_id: customer.id,
            owner_user_id: invoiceOwner!.user_id,
            status: "active",
        }
        const invoiceRule = await command<Rule>(
            token,
            "POST",
            "/admin/finance-responsibility-rules",
            invoiceInput,
        )
        expect(invoiceRule).toMatchObject({ ...invoiceInput, version: 1 })
        const countBefore = (
            await apiGet<Rule[]>(token, "/admin/finance-responsibility-rules")
        ).length
        const stopped = await command<{ status: string; version: number }>(
            token,
            "PUT",
            `/admin/customers/${customer.id}`,
            { version: customer.version, status: "disabled" },
        )
        expect(stopped).toMatchObject({
            status: "disabled",
            version: customer.version + 1,
        })
        await rejected(
            token,
            "POST",
            "/admin/finance-responsibility-rules",
            invoiceInput,
            [400, 422],
        )
        const after = await apiGet<Array<Rule & { counterparty_no: string }>>(
            token,
            "/admin/finance-responsibility-rules",
        )
        expect(after).toHaveLength(countBefore)
        expect(after.find((item) => item.id === invoiceRule.id)).toMatchObject({
            version: 1,
            counterparty_id: customer.id,
            counterparty_no: customer.customer_no,
        })
    })
})

test("[flow-31] 精确采购责任优先、停用回退、逐行失败和版本拒绝", async ({
    browser,
}) => {
    const { page } = await openLoggedInWorkspace(browser, "admin")
    const token = await apiToken("admin")
    const [skus, rules, accounts] = await Promise.all([
        apiGet<{ items: Array<{ sku_id: string; sku_no: string }> }>(
            token,
            "/admin/sellable-skus",
            { page_size: 100 },
        ),
        apiGet<{ items: Rule[] }>(
            token,
            "/admin/procurement-responsibility-rules",
            { page_size: 200 },
        ),
        apiGet<Array<{ id: string; account: string }>>(token, "/admin/admins"),
    ])
    const sku = skus.items.find(
        (item) => !rules.items.some((rule) => rule.sku_id === item.sku_id),
    )
    const owner = accounts.find((item) => item.account === "caigou")
    expect(sku, "种子须提供未配置精确采购责任的在售 SKU").toBeTruthy()
    expect(owner).toBeTruthy()
    const line = { line_key: "valid-line", sku_id: sku!.sku_id }
    const baseline = await command<Resolution>(
        token,
        "POST",
        "/admin/procurement-responsibility/resolve",
        { lines: [line] },
    )
    expect(baseline.lines[0]).toMatchObject({
        line_key: line.line_key,
        resolved: true,
    })

    await page.goto("/master-data/procurement-responsibilities")
    await page.locator("#procurement-responsibility-rules-create").click()
    await chooseOption(
        page,
        page.locator("#procurement-responsibility-rules-dialog-sku"),
        new RegExp(sku!.sku_no),
        sku!.sku_no,
    )
    await chooseOption(
        page,
        page.locator("#procurement-responsibility-rules-dialog-owner"),
        /caigou/,
    )
    const created = await saveRule(
        page,
        "POST",
        "/admin/procurement-responsibility-rules",
        "procurement-responsibility-rules-dialog-save",
    )
    expect(created).toMatchObject({
        rule_type: "SKU",
        sku_id: sku!.sku_id,
        owner_user_id: owner!.id,
        status: "active",
        version: 1,
    })
    const resolved = await command<Resolution>(
        token,
        "POST",
        "/admin/procurement-responsibility/resolve",
        {
            lines: [
                line,
                { line_key: "missing-line", sku_id: "sku-e2e-missing" },
            ],
        },
    )
    expect(resolved.lines).toHaveLength(2)
    expect(resolved.lines[0]).toMatchObject({
        resolved: true,
        rule_id: created.id,
        rule_type: "SKU",
        owner_user_id: owner!.id,
    })
    expect(resolved.lines[1]).toMatchObject({
        line_key: "missing-line",
        resolved: false,
    })
    expect(resolved.lines[1]!.error).toBeTruthy()
    await rejected(
        token,
        "POST",
        "/admin/procurement-responsibility/resolve",
        { lines: [line, line] },
        [400, 422],
    )

    await page.getByRole("row").filter({ hasText: sku!.sku_no }).click()
    await page
        .locator("#procurement-responsibility-rules-dialog-enabled")
        .click()
    const disabled = await saveRule(
        page,
        "PUT",
        `/admin/procurement-responsibility-rules/${created.id}`,
        "procurement-responsibility-rules-dialog-save",
    )
    expect(disabled).toMatchObject({ status: "disabled", version: 2 })
    const input = {
        rule_type: "SKU",
        sku_id: sku!.sku_id,
        owner_user_id: owner!.id,
        status: "active",
        version: 1,
    }
    await rejected(
        token,
        "PUT",
        `/admin/procurement-responsibility-rules/${created.id}`,
        input,
        [400, 409],
    )
    await rejected(
        await apiToken("xiaoshou"),
        "POST",
        "/admin/procurement-responsibility-rules",
        input,
        [403],
    )
    const fallback = await command<Resolution>(
        token,
        "POST",
        "/admin/procurement-responsibility/resolve",
        { lines: [line] },
    )
    expect(fallback.lines[0]).toMatchObject({
        resolved: true,
        rule_id: baseline.lines[0]!.rule_id,
        owner_user_id: baseline.lines[0]!.owner_user_id,
    })
})
