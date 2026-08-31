/**
 * 流程: [flow-17] 发票红冲（销项与进项，无审批）
 * 文档: docs/erp-phase-1.md §6.5.4（红票=Invoice，NO_APPROVAL）；
 *       docs/approval-workflow-contract.md §4.3；docs/workbench-workitem-contract.md §3
 * 账号: xiaoshou 建客户/合同/销售单；caigou 采购确认销售单并供给分配；
 *       caiwu 审批采购单（形成应付）；kaipiao 从 W01 登记销项发票，在 W11/W12 按 Invoice
 *       强类型命令登记销项/进项红票。禁止把红票送进审批流。
 *
 * 文档-代码差异（以代码为准）:
 * 1. 文档时序由「业务部门确认依据 → 财务经办登记红票」；代码无单独确认节点，
 *    kaipiao（role-finance）在客户/供应商往来对已登记蓝票点「红票」，一次提交即登记。
 * 2. 文档开票进度完成态写「已完成」；销售单金额摘要 mapInvoicing 为「已开齐」。
 * 3. 文档写「原发票记录保留」；全额红冲后原蓝票仍在列表，销项状态「已作废」、
 *    进项状态「已红冲」，红票是独立记录（种类「红票」）。
 * 4. 销项红票弹窗只填金额+原因，不填红票号码（后端按幂等键生成 HT-…）；
 *    进项红票弹窗红票号码必填，默认 R{原票号}。
 * 5. 销项开票必须从 W01 SALES_INVOICE_EXECUTION 原地登记；W11「登记销项发票」
 *    无开票任务时禁用。红冲后可开票余额回退，任务以 INVOICEABLE_REOPENED_BY_RED_INVOICE 重开。
 * 6. 销售单对象中心冲正/红票关闭（canRequestReverse=false），红票只在财务工作台。
 */
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

import {
    test,
    expect,
    type Browser,
    type BrowserContext,
    type Locator,
    type Page,
} from "@playwright/test"

import { ACCOUNTS } from "../helpers/accounts"
import { apiGet, apiLogin } from "../helpers/api"
import { loginViaUi, newLoggedInContext } from "../helpers/login"
import "../helpers/ui"

test.describe.configure({ mode: "serial" })
test.use({ viewport: { width: 1440, height: 960 } })

const TIMEOUT = 20_000
const LONG = 40_000
const FLOW_TIMEOUT = 12 * 60 * 1000
const SKU_NAME = "狮峰明前龙井礼盒"
const SUPPLIER_SHORT = "狮峰茶叶"
const UNIT_PRICE = "1288.00"

const INVOICE_FORBIDDEN_ACTIONS = [
    "选择流程",
    "更新审批流程版本",
    "提交审批",
    "撤回审批",
    "改派当前审批人",
    "恢复当前审批人",
    "取消受阻审批",
] as const

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const CONTRACT_PDF = path.resolve(REPO_ROOT, "fixtures", "sample-contract.pdf")

test("销项与进项红票按 Invoice 强类型命令登记，不进审批且可开票金额回退", async ({
    page,
    browser,
}) => {
    test.setTimeout(FLOW_TIMEOUT)

    const stamp = Date.now().toString(36).toUpperCase()
    const legalName = `红票客户${stamp}`
    const shortName = `红票${stamp.slice(-6)}`
    const creditCode = (`9117F17${stamp}000000000000`).replace(/[^0-9A-Z]/g, "0").slice(0, 18)
    const contractNo = `HT-F17-${stamp}`
    const salesInvoiceNo = `XS${stamp}`.slice(0, 20)
    const purchaseInvoiceNo = `JX${stamp}`.slice(0, 20)
    const purchaseRedNo = `R${purchaseInvoiceNo}`
    const extra: BrowserContext[] = []

    try {
        // ── 1. 销售：客户 + 合同 + 实物销售单提交 ────────────────────────
        await loginViaUi(page, accountSpec("xiaoshou"))
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: LONG,
        })

        const customerId = await createCustomer(page, {
            legalName,
            shortName,
            creditCode,
        })
        await uploadContract(page, { customerId, legalName, contractNo })
        const order = await createAndSubmitPhysicalSalesOrder(page, {
            customerId,
            contractNo,
            legalName,
        })

        // ── 2. 采购：W01 通过销售单审批（本阶段不分配供给）──────────────
        const caigou = await openRole(browser, extra, "caigou")
        await approveWorkspaceTask(caigou.page, "销售单审批", order.orderNo)
        await caigou.context.close()

        await page.goto(`/sales/orders/${order.id}`)
        await expectEffectiveSalesOrder(page, order.orderNo)
        await expectFulfillmentNotStarted(page)
        await expectNotClosed(page)
        await expectNoChangeOrder(page)
        await page.getByRole("tab", { name: /^采购/ }).click()
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText(
            "采购单 0 笔",
            { timeout: TIMEOUT },
        )
        await expect(page.getByText("本单还没有采购单。")).toBeVisible({ timeout: TIMEOUT })

        // ── 3. 开票人：W01 销项开票任务登记蓝票（Invoice=NO_APPROVAL）──
        const kaipiao = await openRole(browser, extra, "kaipiao")
        const registeredSalesNo = await registerSalesInvoiceFromWorkspace(
            kaipiao.page,
            order.orderNo,
            salesInvoiceNo,
        )
        expect(registeredSalesNo).toBe(salesInvoiceNo)
        await assertNoInvoiceApprovalUi(kaipiao.page)
        await expect(kaipiao.page.getByRole("button", { name: /发票审批/ })).toHaveCount(0)

        await page.goto(`/sales/orders/${order.id}`)
        await expectInvoicing(page, "已开齐")
        await expectInvoicedAmount(page, UNIT_PRICE)
        await expectNotClosed(page)
        await expectFulfillmentNotStarted(page)

        // 开票完成后任务应离开队列；红冲前不得出现发票审批待办
        await kaipiao.page.goto("/workspace")
        await expect(kaipiao.page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: LONG,
        })
        await kaipiao.page.locator("#workspace-family-nav-finance").click()
        const invoiceSearch = kaipiao.page.locator("#workspace-queue-toolbar-search-input")
        await invoiceSearch.fill(order.orderNo)
        await invoiceSearch.press("Enter")
        await expect(
            kaipiao.page.getByRole("button", {
                name: new RegExp(`销项开票处理[\\s\\S]*${escapeRe(order.orderNo)}`),
            }),
        ).toHaveCount(0, { timeout: LONG })
        await expect(kaipiao.page.getByRole("button", { name: /发票审批|销项发票审批/ })).toHaveCount(0)

        // ── 4. 开票人：W11 对原蓝票登记销项红票，原票保留、可开票回退 ──
        const salesRedNo = await issueSalesRedInvoice(kaipiao.page, {
            invoiceNo: salesInvoiceNo,
            amount: UNIT_PRICE,
            reason: "销项发票开具错误，全额红冲回退可开票金额",
        })
        expect(salesRedNo.length).toBeGreaterThan(2)
        expect(salesRedNo).not.toEqual(salesInvoiceNo)
        await assertNoInvoiceApprovalUi(kaipiao.page)
        await expect(kaipiao.page.getByRole("button", { name: "提交审批" })).toHaveCount(0)

        await assertSalesInvoiceRow(kaipiao.page, salesInvoiceNo, {
            kind: "蓝票",
            status: "已作废",
        })
        await searchCustomerInvoices(kaipiao.page, salesRedNo)
        await expect(kaipiao.page.getByText("红票").first()).toBeVisible({ timeout: LONG })
        await expect(kaipiao.page.getByText(salesRedNo).first()).toBeVisible({ timeout: LONG })
        await expect(kaipiao.page.getByText("已登记").first()).toBeVisible({ timeout: LONG })

        await kaipiao.page.locator("#customer-receivables-view-receivable").click()
        await kaipiao.page.locator("#customer-receivables-toolbar-search").fill(order.orderNo)
        await kaipiao.page.locator("#customer-receivables-toolbar-search").press("Enter")
        const receivableRow = kaipiao.page.getByRole("row").filter({ hasText: order.orderNo })
        await expect(receivableRow).toBeVisible({ timeout: LONG })
        await expect(receivableRow.getByText(/0\.00/).first()).toBeVisible({ timeout: LONG })
        await expect(receivableRow.getByText(/1,288\.00|1288\.00/).first()).toBeVisible({
            timeout: LONG,
        })

        await page.goto(`/sales/orders/${order.id}`)
        await expectInvoicing(page, "未开")
        await expectInvoicedAmount(page, "0.00")
        await expectNotClosed(page)
        await expectFulfillmentNotStarted(page)
        await expectNoChangeOrder(page)
        await page.getByRole("tab", { name: /^票款/ }).click()
        await expect(page.getByText(salesInvoiceNo).first()).toBeVisible({ timeout: LONG })
        await expect(page.getByText("红票").first()).toBeVisible({ timeout: LONG })
        await expect(page.locator("#customer-receivables-preview-invoice-red")).toHaveCount(0)

        // 可开票余额回退后，W01 开票任务重开；仍不得出现审批任务
        await kaipiao.page.goto("/workspace")
        await expect(kaipiao.page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: LONG,
        })
        await kaipiao.page.locator("#workspace-family-nav-finance").click()
        const reopenSearch = kaipiao.page.locator("#workspace-queue-toolbar-search-input")
        await reopenSearch.fill(order.orderNo)
        await reopenSearch.press("Enter")
        await expect(
            kaipiao.page.getByRole("button", {
                name: new RegExp(`销项开票处理[\\s\\S]*${escapeRe(order.orderNo)}`),
            }).first(),
        ).toBeVisible({ timeout: LONG })
        await kaipiao.page
            .getByRole("button", {
                name: new RegExp(`销项开票处理[\\s\\S]*${escapeRe(order.orderNo)}`),
            })
            .first()
            .click()
        await expect(kaipiao.page.getByLabel("当前开票任务")).toBeVisible({ timeout: LONG })
        await assertNoInvoiceApprovalUi(kaipiao.page)
        await expect(kaipiao.page.getByRole("button", { name: /发票审批|销项发票审批/ })).toHaveCount(0)
        await expect(kaipiao.page.getByRole("button", { name: "通过", exact: true })).toHaveCount(0)
        await expect(kaipiao.page.getByRole("button", { name: "驳回", exact: true })).toHaveCount(0)

        await assertNoInvoiceApprovalInstances("kaipiao")

        // ── 5. 采购：供给分配创建采购单并立即提交审批 ──────────────────
        const caigou2 = await openRole(browser, extra, "caigou")
        await allocateAndSubmitPurchaseOrder(caigou2.page, order.orderNo)
        await caigou2.context.close()

        const caiwu = await openRole(browser, extra, "caiwu")
        await approveWorkspaceTask(caiwu.page, "采购单审批")
        await caiwu.context.close()

        await page.goto(`/sales/orders/${order.id}`)
        await page.getByRole("tab", { name: /^采购/ }).click()
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText(
            "采购单 1 笔",
            { timeout: LONG },
        )
        await expectFulfillmentNotStarted(page)
        await expectNotClosed(page)

        // ── 6. 开票人：W12 登记进项蓝票（与付款分轨，不进审批）────────
        const purchaseGross = await registerPurchaseInvoice(kaipiao.page, {
            supplier: SUPPLIER_SHORT,
            invoiceNo: purchaseInvoiceNo,
        })
        expect(Number(purchaseGross)).toBeGreaterThan(0)
        await assertNoInvoiceApprovalUi(kaipiao.page)

        await kaipiao.page.getByRole("tab", { name: "应付台账" }).click()
        await expect(kaipiao.page.getByText(/狮峰/).first()).toBeVisible({ timeout: LONG })
        const payableRow = kaipiao.page.getByRole("row").filter({ hasText: /狮峰/ }).first()
        await expect(payableRow).toBeVisible({ timeout: LONG })
        await expect(payableRow.getByText(formatMoneyish(purchaseGross)).first()).toBeVisible({
            timeout: LONG,
        })

        // ── 7. 开票人：进项红票，原蓝票保留，已收票进度回退 ────────────
        await issuePurchaseRedInvoice(kaipiao.page, {
            invoiceNo: purchaseInvoiceNo,
            redInvoiceNo: purchaseRedNo,
            reason: "进项发票开具错误，全额红冲回退可收票金额",
        })
        await assertNoInvoiceApprovalUi(kaipiao.page)
        await expect(kaipiao.page.getByRole("button", { name: "提交审批" })).toHaveCount(0)
        await expect(kaipiao.page.getByRole("button", { name: /发票审批|进项发票审批/ })).toHaveCount(0)

        await searchSupplierInvoices(kaipiao.page, purchaseInvoiceNo)
        const blueRow = kaipiao.page.getByRole("row").filter({ hasText: purchaseInvoiceNo }).filter({
            hasText: "蓝票",
        })
        await expect(blueRow).toBeVisible({ timeout: LONG })
        await expect(blueRow.getByText("已红冲")).toBeVisible({ timeout: LONG })

        await searchSupplierInvoices(kaipiao.page, purchaseRedNo)
        const redRow = kaipiao.page.getByRole("row").filter({ hasText: purchaseRedNo }).filter({
            hasText: "红票",
        })
        await expect(redRow).toBeVisible({ timeout: LONG })
        await expect(redRow.getByText("已登记")).toBeVisible({ timeout: LONG })

        await kaipiao.page.getByRole("tab", { name: "应付台账" }).click()
        await expect(kaipiao.page.getByText(/狮峰/).first()).toBeVisible({ timeout: LONG })
        await expect(kaipiao.page.getByText("收票").first()).toBeVisible({ timeout: TIMEOUT })
        await expect(kaipiao.page.getByText(/0\.00/).first()).toBeVisible({ timeout: LONG })

        await kaipiao.page.goto("/workspace")
        await expect(kaipiao.page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: LONG,
        })
        await kaipiao.page.locator("#workspace-family-nav-approval").click()
        await expect(kaipiao.page.getByRole("button", { name: /发票审批|进项发票审批|销项发票审批/ })).toHaveCount(0)
        await kaipiao.page.locator("#workspace-family-nav-finance").click()
        await expect(kaipiao.page.getByRole("button", { name: /发票审批/ })).toHaveCount(0)

        await assertNoInvoiceApprovalInstances("kaipiao")
        await assertNoInvoiceApprovalInstances("caiwu")
        await kaipiao.context.close()

        await page.goto(`/sales/orders/${order.id}`)
        await expectNotClosed(page)
        await expectFulfillmentNotStarted(page)
        await expectNoChangeOrder(page)
        await expectInvoicing(page, "未开")
    } finally {
        await Promise.allSettled(extra.map((context) => context.close()))
    }
})

// ─── 账号 / 登录 ───────────────────────────────────────────────────────────

function accountSpec(login: string) {
    const bag = ACCOUNTS as Record<string, unknown>
    const aliases: Record<string, string[]> = {
        xiaoshou: ["xiaoshou", "sales"],
        caigou: ["caigou", "procurement"],
        caiwu: ["caiwu", "finance"],
        kaipiao: ["kaipiao", "invoice"],
        admin: ["admin"],
    }
    for (const key of aliases[login] ?? [login]) {
        if (bag[key] != null) return bag[key]
    }
    return { account: login, password: "123456" }
}

async function openRole(
    browser: Browser,
    extra: BrowserContext[],
    login: string,
): Promise<{ context: BrowserContext; page: Page }> {
    const opened = await newLoggedInContext(
        browser,
        accountSpec(login) as Parameters<typeof newLoggedInContext>[1],
    )
    const bundle =
        opened && typeof opened === "object" && "page" in opened
            ? (opened as { context: BrowserContext; page: Page })
            : {
                  context: opened as BrowserContext,
                  page:
                      (opened as BrowserContext).pages()[0] ??
                      (await (opened as BrowserContext).newPage()),
              }
    extra.push(bundle.context)
    if (await bundle.page.locator("#governance-auth-login-account").isVisible().catch(() => false)) {
        await loginViaUi(bundle.page, accountSpec(login) as Parameters<typeof loginViaUi>[1])
    }
    return bundle
}

// ─── 通用 UI ───────────────────────────────────────────────────────────────

async function chooseCombobox(page: Page, inputId: string, query: string, optionId?: string) {
    const input = page.locator(`#${inputId}`)
    await expect(input).toBeVisible({ timeout: TIMEOUT })
    await input.click()
    await input.fill(query)
    if (optionId) {
        const byId = page.locator(`#${optionId}`)
        if (await byId.isVisible({ timeout: TIMEOUT }).catch(() => false)) {
            await byId.click()
            return
        }
    }
    const option = page.getByRole("option", { name: new RegExp(escapeRe(query)) })
    if (await option.first().isVisible({ timeout: 5000 }).catch(() => false)) {
        await option.first().click()
        return
    }
    const item = page.locator('[data-slot="combobox-item"]').filter({ hasText: query })
    await expect(item.first()).toBeVisible({ timeout: TIMEOUT })
    await item.first().click()
}

async function pickToday(page: Page, triggerId: string) {
    const today = new Date()
    const iso = [
        today.getFullYear(),
        String(today.getMonth() + 1).padStart(2, "0"),
        String(today.getDate()).padStart(2, "0"),
    ].join("-")
    const monthStart = `${iso.slice(0, 8)}01`
    await page.locator(`#${triggerId}`).click()
    const day = page.locator(`#${triggerId}-calendar-month-${monthStart}-day-${iso}`)
    await expect(day).toBeVisible({ timeout: TIMEOUT })
    await day.click()
}

function contractPdfFile() {
    if (fs.existsSync(CONTRACT_PDF)) {
        return CONTRACT_PDF
    }
    return {
        name: "sample-contract.pdf",
        mimeType: "application/pdf" as const,
        buffer: Buffer.from(
            "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n",
        ),
    }
}

function escapeRe(value: string) {
    return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
}

function parseAmount(raw: string): string {
    const match = raw.replace(/,/g, "").match(/-?\d+(?:\.\d+)?/)
    if (!match) throw new Error(`无法解析金额: ${raw}`)
    return Number(match[0]).toFixed(2)
}

function splitGross(gross: string, taxRatePercent = "13"): { net: string; tax: string } {
    const grossCents = Math.round(Number(parseAmount(gross)) * 100)
    const rate = Number(taxRatePercent)
    const netCents = Math.round(grossCents / (1 + rate / 100))
    const taxCents = grossCents - netCents
    return { net: (netCents / 100).toFixed(2), tax: (taxCents / 100).toFixed(2) }
}

function formatMoneyish(amount: string): RegExp {
    const n = parseAmount(amount)
    const [intPart, frac = "00"] = n.split(".")
    const grouped = intPart.replace(/\B(?=(\d{3})+(?!\d))/g, ",")
    return new RegExp(`${escapeRe(grouped)}\\.${frac}|${escapeRe(n)}`)
}

async function factValue(page: Page, label: string) {
    const dt = page.locator('[data-slot="formal-action-result"] dt', { hasText: label })
    await expect(dt).toBeVisible({ timeout: TIMEOUT })
    return (await dt.locator("xpath=following-sibling::dd[1]").innerText()).trim()
}

async function waitHeading(page: Page, name: string | RegExp) {
    await expect(page.getByRole("heading", { name })).toBeVisible({ timeout: LONG })
}

async function assertNoInvoiceApprovalUi(scope: Page | Locator) {
    for (const label of INVOICE_FORBIDDEN_ACTIONS) {
        await expect(scope.getByRole("button", { name: label, exact: true })).toHaveCount(0)
    }
    await expect(scope.getByText("发票审批")).toHaveCount(0)
    await expect(scope.getByText("销项发票审批")).toHaveCount(0)
    await expect(scope.getByText("进项发票审批")).toHaveCount(0)
    await expect(scope.getByText("选择流程")).toHaveCount(0)
}

async function assertNoInvoiceApprovalInstances(login: string) {
    const token = await apiLogin(
        accountSpec(login) as Parameters<typeof apiLogin>[0],
    )
    for (const view of ["mine", "started", "managed"] as const) {
        const listed = await apiGet<{ items?: Array<{ document_type?: string }> }>(
            token,
            "/admin/approval-instances",
            { view, document_type: "invoice", limit: 50 },
        )
        const items = listed?.items ?? []
        expect(items, `Invoice 不得创建审批实例 view=${view}`).toEqual([])
    }
}

// ─── 客户 / 合同 / 销售单 ─────────────────────────────────────────────────

async function createCustomer(
    page: Page,
    input: { legalName: string; shortName: string; creditCode: string },
) {
    await page.goto("/sales/customers")
    await waitHeading(page, "客户中心")
    await page.locator("#customers-directory-create").click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "新建客户" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.locator("#customers-form-legal-name").fill(input.legalName)
    await page.locator("#customers-form-short-name").fill(input.shortName)
    await page.locator("#customers-form-credit-code").fill(input.creditCode)
    await page.locator("#customers-form-submit").click()
    await expect(page.getByText("客户已创建")).toBeVisible({ timeout: LONG })
    await expect(page.getByRole("dialog").getByRole("heading", { name: "新建客户" })).toBeHidden({
        timeout: TIMEOUT,
    })

    await page.locator("#customers-directory-search").fill(input.legalName)
    await page.locator("#customers-directory-search").press("Enter")
    const open = page.getByRole("link", { name: input.shortName })
    await expect(open).toBeVisible({ timeout: LONG })
    await open.click()
    await expect(page.getByRole("heading", { name: input.legalName })).toBeVisible({
        timeout: LONG,
    })
    const match = page.url().match(/\/sales\/customers\/([^/?#]+)/)
    expect(match?.[1]).toBeTruthy()
    return match![1]
}

async function uploadContract(
    page: Page,
    input: { customerId: string; legalName: string; contractNo: string },
) {
    await page.goto(`/sales/contracts?customerId=${encodeURIComponent(input.customerId)}&upload=1`)
    await expect(page.getByRole("dialog").getByRole("heading", { name: "上传合同 PDF" })).toBeVisible({
        timeout: LONG,
    })
    await page.locator("#card-contracts-upload-pdf-input").setInputFiles(contractPdfFile())
    await page.locator("#card-contracts-upload-contract-no").fill(input.contractNo)
    const customerInput = page.locator("#card-contracts-upload-customer")
    const customerValue = (await customerInput.inputValue()).trim()
    if (!customerValue) {
        await chooseCombobox(page, "card-contracts-upload-customer", input.legalName)
    }
    await expect(page.locator("#card-contracts-upload-settlement-party")).not.toHaveValue("", {
        timeout: LONG,
    })
    await page.locator("#card-contracts-upload-submit").click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "上传合同 PDF" })).toBeHidden({
        timeout: LONG,
    })
    await expect(page.getByText(input.contractNo)).toBeVisible({ timeout: LONG })
}

async function createAndSubmitPhysicalSalesOrder(
    page: Page,
    input: { customerId: string; contractNo: string; legalName: string },
) {
    await page.goto(`/sales/orders?mode=create&customerId=${encodeURIComponent(input.customerId)}`)
    await expect(page.getByRole("heading", { name: "单据头" })).toBeVisible({ timeout: LONG })

    await chooseCombobox(page, "sales-orders-create-contract", input.contractNo)
    await expect(page.getByText(new RegExp(`客户\\s+${input.legalName}`))).toBeVisible({
        timeout: LONG,
    })
    await expect(page.locator("#sales-orders-create-header-owner-name")).not.toHaveValue("", {
        timeout: LONG,
    })

    await chooseCombobox(
        page,
        "sales-orders-create-header-welfare-scene",
        "年节礼包",
        "sales-orders-create-header-welfare-scene-option-annual-gift-bag",
    )
    await chooseCombobox(
        page,
        "sales-orders-create-header-payment-terms",
        "货到 30 天",
        "sales-orders-create-header-payment-terms-option-postpay-net30",
    )

    await page.getByRole("button", { name: "选择商品" }).click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "选择商品" })).toBeVisible({
        timeout: TIMEOUT,
    })
    const skuSearch = page.locator("#master-data-list-sellable-list-toolbar-search-input")
    await skuSearch.fill(SKU_NAME)
    await skuSearch.press("Enter")
    const skuCheckbox = page.getByRole("checkbox", { name: new RegExp(`选择 ${SKU_NAME}`) })
    await expect(skuCheckbox.first()).toBeVisible({ timeout: LONG })
    await skuCheckbox.first().check()
    await page.locator("#sales-orders-sku-picker-confirm").click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "选择商品" })).toBeHidden({
        timeout: TIMEOUT,
    })
    await expect(page.getByText(SKU_NAME)).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByTestId(/sales-line-procurement-owner-/)).not.toContainText(
        "暂未确定采购负责人",
        { timeout: LONG },
    )

    await pickToday(page, "sales-orders-create-batch-due-date")
    await page.locator("#sales-orders-create-batch-due-date-apply").click()
    await expect(page.getByText("已批量设置交期")).toBeVisible({ timeout: TIMEOUT })

    await page.locator("#sales-orders-create-submit").click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "提交销售单" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.locator("#sales-orders-submit-confirm-confirm").click()
    await expect(page).toHaveURL(/\/sales\/orders\/[^/?#]+/, { timeout: LONG })
    await expect(page.getByText("审批中", { exact: true }).first()).toBeVisible({ timeout: LONG })

    const id = page.url().split("/sales/orders/")[1]?.split(/[?#]/)[0] ?? ""
    expect(id).toBeTruthy()
    const identity = await page
        .locator("header")
        .filter({ has: page.getByRole("heading", { level: 1 }) })
        .innerText()
    const orderNo = identity.match(/单号\s+(\S+)/)?.[1] ?? ""
    expect(orderNo.length).toBeGreaterThan(4)
    return { id, orderNo }
}

async function expectEffectiveSalesOrder(page: Page, orderNo: string) {
    await expect(page.getByText(orderNo, { exact: true }).first()).toBeVisible({ timeout: LONG })
    await expect(page.getByText("已生效", { exact: true }).first()).toBeVisible({ timeout: LONG })
    await expectCollection(page, "未收")
    await expectInvoicing(page, "未开")
    await expectNotClosed(page)
}

async function expectCollection(page: Page, label: "未收" | "部分回款" | "已结清") {
    await expect(page.getByLabel("销售单金额摘要").getByText(label, { exact: true })).toBeVisible({
        timeout: LONG,
    })
}

async function expectInvoicing(page: Page, label: "未开" | "部分开票" | "已开齐") {
    await expect(page.getByLabel("销售单金额摘要").getByText(label, { exact: true })).toBeVisible({
        timeout: LONG,
    })
}

async function expectInvoicedAmount(page: Page, amount: string) {
    const cell = page.getByLabel("销售单金额摘要").locator("div").filter({ hasText: "已开票" })
    await expect(cell.getByText(formatMoneyish(amount)).first()).toBeVisible({ timeout: LONG })
}

async function expectFulfillmentNotStarted(page: Page) {
    await expect(
        page
            .locator("header")
            .filter({ has: page.getByRole("heading", { level: 1 }) })
            .getByText("未开始", { exact: true }),
    ).toBeVisible({ timeout: TIMEOUT })
}

async function expectNotClosed(page: Page) {
    const identity = page.getByRole("heading", { level: 1 }).locator("xpath=ancestor::header[1]")
    await expect(identity.getByText("已生效", { exact: true })).toBeVisible({ timeout: LONG })
    await expect(identity.getByText("已关闭", { exact: true })).toHaveCount(0)
}

async function expectNoChangeOrder(page: Page) {
    await expect(page.getByText("改单中", { exact: true })).toHaveCount(0)
    await expect(page.getByText("销售变更单审批")).toHaveCount(0)
    await expect(page.locator("#sales-orders-detail-start-change")).toBeVisible({
        timeout: TIMEOUT,
    })
}

// ─── 工作台审批 / 供给分配 ────────────────────────────────────────────────

async function approveWorkspaceTask(page: Page, typeLabel: string, hint?: string) {
    await page.goto("/workspace")
    await waitHeading(page, "我的工作台")
    await page.locator("#workspace-family-nav-approval").click()
    const list = page.getByRole("list", { name: "待办列表" })
    if (hint) {
        const search = page.locator("#workspace-queue-toolbar-search-input")
        await search.fill(hint)
        await search.press("Enter")
    }
    const task = hint
        ? list.getByRole("button", {
              name: new RegExp(`${escapeRe(typeLabel)}[\\s\\S]*${escapeRe(hint)}|${escapeRe(hint)}[\\s\\S]*${escapeRe(typeLabel)}`),
          })
        : list.getByRole("button", { name: new RegExp(escapeRe(typeLabel)) })
    await expect(task.first()).toBeVisible({ timeout: LONG })
    await task.first().click()
    const approve = page.getByRole("button", { name: "通过", exact: true })
    await expect(approve).toBeVisible({ timeout: LONG })
    await approve.click()
    await expect(page.getByRole("heading", { name: "确认通过" })).toBeVisible({ timeout: TIMEOUT })
    await page.getByRole("button", { name: "确认通过" }).click()
    await expect(page.getByRole("heading", { name: "确认通过" })).toBeHidden({ timeout: LONG })
    await expect(task.first()).toBeHidden({ timeout: LONG })
}

async function allocateAndSubmitPurchaseOrder(page: Page, orderNo: string) {
    await page.goto("/workspace")
    await waitHeading(page, "我的工作台")
    await page.locator("#workspace-family-nav-procurement").click()
    const search = page.locator("#workspace-queue-toolbar-search-input")
    await search.fill(orderNo)
    await search.press("Enter")
    const task = page.getByRole("list", { name: "待办列表" }).getByRole("button", {
        name: new RegExp(`待供给分配[\\s\\S]*${escapeRe(orderNo)}`),
    })
    await expect(task.first()).toBeVisible({ timeout: LONG })
    await task.first().click()
    await expect(page.getByRole("heading", { name: "供给分配" })).toBeVisible({ timeout: LONG })
    await expect(page.getByText("将创建采购单")).toBeVisible({ timeout: TIMEOUT })
    await page.locator("#procurement-orders-create-preview").click()
    const preview = page.getByRole("dialog", { name: "预览供给分配" })
    await expect(preview).toBeVisible({ timeout: TIMEOUT })
    await expect(preview.getByText("本次全部由现有库存满足")).toHaveCount(0)
    await preview.locator("#procurement-orders-create-preview-confirm").click()
    const confirmAlloc = page.getByRole("alertdialog").filter({ hasText: "确认供给分配" })
    await expect(confirmAlloc).toBeVisible({ timeout: TIMEOUT })
    await confirmAlloc.locator("#procurement-orders-create-confirm").click()
    await expect(page.getByText(/已创建 1 张采购单并提交审批|已将缺口拆成/).first()).toBeVisible({
        timeout: LONG,
    })
}

// ─── 销项发票 / 销项红票 ──────────────────────────────────────────────────

async function registerSalesInvoiceFromWorkspace(
    page: Page,
    orderNo: string,
    invoiceNo: string,
) {
    await page.goto("/workspace")
    await waitHeading(page, "我的工作台")
    await page.locator("#workspace-family-nav-finance").click()
    const search = page.locator("#workspace-queue-toolbar-search-input")
    await search.fill(orderNo)
    await search.press("Enter")
    const task = page.getByRole("list", { name: "待办列表" }).getByRole("button", {
        name: new RegExp(`销项开票处理[\\s\\S]*${escapeRe(orderNo)}`),
    })
    await expect(task.first()).toBeVisible({ timeout: LONG })
    await task.first().click()
    await expect(page.getByLabel("当前开票任务")).toBeVisible({ timeout: LONG })
    await expect(page.getByRole("heading", { name: /核销 · / })).toBeVisible({ timeout: LONG })
    await expect(page.getByRole("heading", { name: "销项发票记录" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await assertNoInvoiceApprovalUi(page)

    await page.locator("#customer-receivables-session-invoice-no").fill(invoiceNo)
    await page.locator("#customer-receivables-session-gross-amount").fill(UNIT_PRICE)
    const join = page.getByRole("button", { name: "加入" }).first()
    if (await join.isVisible().catch(() => false)) {
        await join.click()
    }
    const fill = page.getByRole("button", { name: "填满" }).first()
    await expect(fill).toBeVisible({ timeout: TIMEOUT })
    await fill.click()

    await page.locator("#customer-receivables-session-submit").click()
    await expect(page.getByRole("heading", { name: "确认登记销项发票并分配" })).toBeVisible({
        timeout: TIMEOUT,
    })
    const confirm = page.getByRole("alertdialog").filter({ hasText: "确认登记销项发票并分配" })
    await expect(confirm.getByText("提交审批")).toHaveCount(0)
    await page.locator("#customer-receivables-session-invoice-confirm-dialog-confirm").click()
    await expect(page.getByRole("heading", { name: "销项发票已登记并分配" })).toBeVisible({
        timeout: LONG,
    })
    const factNo = await factValue(page, "发票号码")
    return factNo
}

async function searchCustomerInvoices(page: Page, query: string) {
    await page.goto(`/finance/customer-accounts?view=sales_invoice&q=${encodeURIComponent(query)}`)
    await waitHeading(page, "客户往来")
    await expect(page.locator("#customer-receivables-view-sales_invoice")).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.locator("#customer-receivables-view-sales_invoice").click()
    await page.locator("#customer-receivables-toolbar-search").fill(query)
    await page.locator("#customer-receivables-toolbar-search").press("Enter")
}

async function assertSalesInvoiceRow(
    page: Page,
    invoiceNo: string,
    expectState: { kind: "蓝票" | "红票"; status: string },
) {
    await searchCustomerInvoices(page, invoiceNo)
    const row = page.getByRole("row").filter({ hasText: invoiceNo })
    await expect(row).toBeVisible({ timeout: LONG })
    await expect(row.getByText(expectState.kind, { exact: true })).toBeVisible({ timeout: TIMEOUT })
    await expect(row.getByText(expectState.status, { exact: true })).toBeVisible({
        timeout: TIMEOUT,
    })
}

async function issueSalesRedInvoice(
    page: Page,
    input: { invoiceNo: string; amount: string; reason: string },
) {
    await searchCustomerInvoices(page, input.invoiceNo)
    const row = page.getByRole("row").filter({ hasText: input.invoiceNo }).filter({ hasText: "蓝票" })
    await expect(row).toBeVisible({ timeout: LONG })
    await expect(row.getByText("已登记")).toBeVisible({ timeout: TIMEOUT })
    await row.getByRole("button", { name: "预览" }).click()
    await expect(page.getByRole("heading", { name: input.invoiceNo })).toBeVisible({
        timeout: LONG,
    })
    await expect(page.getByText("已登记发票只读")).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText("红票为独立记录加反向分配。")).toBeVisible()
    await expect(page.getByText("蓝票").first()).toBeVisible()
    await assertNoInvoiceApprovalUi(page)
    await expect(page.getByRole("button", { name: "提交审批" })).toHaveCount(0)

    const redButton = page.locator("#customer-receivables-preview-invoice-red")
    await expect(redButton).toBeVisible({ timeout: TIMEOUT })
    await expect(redButton).toBeEnabled()
    await redButton.click()

    const dialog = page.getByRole("dialog").filter({ hasText: "发起销项红票" })
    await expect(dialog).toBeVisible({ timeout: TIMEOUT })
    await expect(dialog.getByText("红票表示冲减原票的分配。")).toBeVisible()
    await expect(dialog.getByRole("button", { name: "提交审批" })).toHaveCount(0)
    const amount = dialog.locator("#customer-receivables-reverse-amount")
    await expect(amount).toBeVisible({ timeout: TIMEOUT })
    const current = (await amount.inputValue()).trim()
    if (!current) {
        await amount.fill(input.amount)
    }
    await dialog.locator("#customer-receivables-reverse-reason").fill(input.reason)
    await dialog.locator("#customer-receivables-reverse-confirm").click()
    await expect(dialog).toBeHidden({ timeout: LONG })
    await expect(page.getByRole("heading", { name: "反向记录已追加" })).toBeVisible({
        timeout: LONG,
    })
    await expect(page.getByText("已登记独立红票并追加反向分配，原蓝票保留。")).toBeVisible({
        timeout: TIMEOUT,
    })
    const redNo = await factValue(page, "反向单号")
    await expect(page.getByText(input.invoiceNo).first()).toBeVisible({ timeout: TIMEOUT })
    return redNo
}

// ─── 进项发票 / 进项红票 ──────────────────────────────────────────────────

async function registerPurchaseInvoice(
    page: Page,
    input: { supplier: string; invoiceNo: string },
) {
    await page.goto("/finance/supplier-accounts")
    await waitHeading(page, "供应商往来")
    await page.locator("#supplier-payables-header-register-invoice").click()
    const pickSupplier = page.getByRole("dialog", { name: /选择供应商 · 登记进项发票/ })
    await expect(pickSupplier).toBeVisible({ timeout: TIMEOUT })
    const supplierInput = pickSupplier.locator("#supplier-payables-pick-supplier-select")
    await expect(supplierInput).toBeVisible({ timeout: TIMEOUT })
    await supplierInput.click()
    await supplierInput.fill(input.supplier)
    const supplierOption = page.getByRole("option", { name: /狮峰/ })
    await expect(supplierOption.first()).toBeVisible({ timeout: LONG })
    await supplierOption.first().click()
    await pickSupplier.locator("#supplier-payables-pick-supplier-confirm").click()
    await expect(page.getByRole("heading", { name: "登记进项发票" })).toBeVisible({
        timeout: LONG,
    })
    await assertNoInvoiceApprovalUi(page)
    await expect(page.getByRole("button", { name: "提交审批" })).toHaveCount(0)

    const poolSelect = page
        .locator('[id^="supplier-payables-allocation-pool-row-"][id$="-select"]')
        .first()
    await expect(poolSelect).toBeVisible({ timeout: LONG })
    if (!(await poolSelect.isChecked())) {
        await page.locator("#supplier-payables-allocation-pool-select-all").click()
    }
    await page.locator("#supplier-payables-allocation-pool-fill-all").click()
    const allocatedInput = page
        .locator('[id^="supplier-payables-allocation-pool-row-"][id$="-amount"]')
        .first()
    await expect(allocatedInput).toHaveValue(/.+/, { timeout: TIMEOUT })
    const gross = parseAmount(await allocatedInput.inputValue())
    const { net, tax } = splitGross(gross)
    await page.locator("#supplier-payables-allocation-form-gross-amount").fill(gross)
    await page.locator("#supplier-payables-allocation-form-invoice-no").fill(input.invoiceNo)
    await page.locator("#supplier-payables-allocation-form-net-amount").fill(net)
    await page.locator("#supplier-payables-allocation-form-tax-amount").fill(tax)
    await page.locator("#supplier-payables-allocation-form-submit").click()
    const invoiceConfirm = page.getByRole("alertdialog").filter({
        hasText: "确认登记进项发票并核销",
    })
    await expect(invoiceConfirm).toBeVisible({ timeout: TIMEOUT })
    await expect(invoiceConfirm.getByText("提交审批")).toHaveCount(0)
    await invoiceConfirm.locator("#supplier-payables-invoice-allocate-confirm-confirm").click()
    await expect(page.getByRole("heading", { name: "进项发票已登记" })).toBeVisible({
        timeout: LONG,
    })
    await page.locator("#supplier-payables-allocation-result-close").click()
    await page.getByRole("tab", { name: "进项发票" }).click()
    await expect(page.getByText(input.invoiceNo).first()).toBeVisible({ timeout: LONG })
    await expect(page.getByText("蓝票").first()).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText("已登记").first()).toBeVisible({ timeout: TIMEOUT })
    return gross
}

async function searchSupplierInvoices(page: Page, query: string) {
    await page.goto(
        `/finance/supplier-accounts?view=purchase_invoice&q=${encodeURIComponent(query)}`,
    )
    await waitHeading(page, "供应商往来")
    await page.getByRole("tab", { name: "进项发票" }).click()
    await page.locator("#supplier-payables-toolbar-search").fill(query)
    await page.locator("#supplier-payables-toolbar-search").press("Enter")
}

async function issuePurchaseRedInvoice(
    page: Page,
    input: { invoiceNo: string; redInvoiceNo: string; reason: string },
) {
    await searchSupplierInvoices(page, input.invoiceNo)
    const row = page.getByRole("row").filter({ hasText: input.invoiceNo }).filter({
        hasText: "蓝票",
    })
    await expect(row).toBeVisible({ timeout: LONG })
    const redButton = row.getByRole("button", { name: "红票" }).or(
        page.locator('[id$="-red-invoice"]').filter({ hasText: "红票" }),
    )
    await expect(redButton.first()).toBeVisible({ timeout: LONG })
    await redButton.first().click()

    const dialog = page.getByRole("dialog").filter({ hasText: "进项红票" })
    await expect(dialog).toBeVisible({ timeout: TIMEOUT })
    await expect(dialog.getByText(new RegExp(`原单 .*${escapeRe(input.invoiceNo)}`))).toBeVisible()
    await expect(dialog.getByRole("button", { name: "提交审批" })).toHaveCount(0)
    await dialog.locator("#supplier-payables-reverse-dialog-reason").fill(input.reason)
    const redNo = dialog.locator("#supplier-payables-reverse-dialog-red-invoice-no")
    await expect(redNo).toBeVisible({ timeout: TIMEOUT })
    await redNo.fill(input.redInvoiceNo)
    await expect(dialog.locator("#supplier-payables-reverse-dialog-confirm")).toBeEnabled({
        timeout: TIMEOUT,
    })
    await dialog.locator("#supplier-payables-reverse-dialog-confirm").click()
    await expect(dialog).toBeHidden({ timeout: LONG })
    await expect(page.getByRole("heading", { name: "红票已登记" })).toBeVisible({
        timeout: LONG,
    })
    await expect(page.getByText("已登记红票并反向分配，原蓝票保留。")).toBeVisible({
        timeout: TIMEOUT,
    })
}
