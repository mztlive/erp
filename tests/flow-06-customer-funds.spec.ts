/**
 * 流程: [flow-06] 客户票款：分次回款、销项发票、核销与冲正
 * 文档: docs/erp-phase-1.md §9.1 + §6.5.4（回款冲正）+ §9.3（关闭与开票）
 * 账号: xiaoshou 建客户/合同/销售单；caigou 审批销售单；fukuan 提交回款与冲正；
 *       caiwu 审批回款/冲正入账（禁止自己提交）；lisiyong 确认冲正依据；kaipiao 在 W01 开票任务登记销项发票。
 *
 * 文档-代码差异（以代码为准）:
 * - 销项发票必须由 kaipiao 从 W01 SALES_INVOICE_EXECUTION 原地登记；W11「登记销项发票」无开票任务时禁用。
 * - 一次工作台开票只能核销当前任务绑定的一张应收子账，不能一张发票跨多张销售单。
 * - 回款正式入账后列表状态文案是「已过账」（按钮不用「过账」）。
 * - 销售单开票进度完成态文案是「已开齐」，不是文档表格里的「已完成」。
 * - 财务三人共用 role-finance：caiwu 可能看见「登记回款」，提交时被 ForbidSubmitterAsApprover 拒绝。
 */
import { test, expect, type Browser, type BrowserContext, type Page } from "@playwright/test"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

import { ACCOUNTS } from "../helpers/accounts"
import { loginViaUi, newLoggedInContext } from "../helpers/login"

const TIMEOUT = 20_000
const LONG = 40_000
const SKU_NAME = "狮峰明前龙井礼盒"
const UNIT_PRICE = "1288.00"
const SPLIT_AMOUNT = "644.00"
const RECEIPT_TOTAL = "1288.00"

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const CONTRACT_PDF = path.resolve(REPO_ROOT, "fixtures", "sample-contract.pdf")

test.describe.configure({ mode: "serial" })

test.describe("flow-06 客户票款：分次回款、销项发票、核销与冲正", () => {
    test("分次回款核销多单、开票不关闭、回款冲正重开应收", async ({
        page,
        browser,
    }) => {
        test.setTimeout(12 * 60 * 1000)

        const stamp = Date.now().toString(36).toUpperCase()
        const legalName = `票款测试客户${stamp}`
        const shortName = `票款${stamp.slice(-6)}`
        const creditCode = (`9111F06${stamp}000000000000`).replace(/[^0-9A-Z]/g, "0").slice(0, 18)
        const contractNo = `HT-F06-${stamp}`
        const extra: BrowserContext[] = []

        try {
            // ── 1. 销售：客户 + 合同 + 两张实物销售单（同主体，覆盖一张回款核销多单）──
            await loginViaUi(page, accountSpec("xiaoshou"))
            await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
                timeout: LONG,
            })

            const customerId = await createCustomer(page, {
                legalName,
                shortName,
                creditCode,
            })
            await uploadContract(page, {
                customerId,
                legalName,
                contractNo,
            })

            const orderA = await createAndSubmitPhysicalSalesOrder(page, {
                customerId,
                contractNo,
                legalName,
            })
            const orderB = await createAndSubmitPhysicalSalesOrder(page, {
                customerId,
                contractNo,
                legalName,
            })
            expect(orderA.orderNo).not.toEqual(orderB.orderNo)

            // ── 2. 采购：W01 原地通过销售单审批（不分配供给、不建采购单）──
            const caigou = await openRole(browser, extra, "caigou")
            await approveWorkspaceTask(caigou.page, "销售单审批", orderA.orderNo)
            await approveWorkspaceTask(caigou.page, "销售单审批", orderB.orderNo)
            await caigou.context.close()

            await page.goto(`/sales/orders/${orderA.id}`)
            await expectEffectiveSalesOrder(page, orderA.orderNo)
            await page.goto(`/sales/orders/${orderB.id}`)
            await expectEffectiveSalesOrder(page, orderB.orderNo)

            // ── 3. 负向：caiwu 不得自己提交回款（岗位分离，运行时 ForbidSubmitterAsApprover）──
            const caiwuDenied = await openRole(browser, extra, "caiwu")
            await assertCaiwuCannotSubmitReceipt(caiwuDenied.page, legalName)
            await caiwuDenied.context.close()

            // ── 4. 出纳分两次回款：同一张回款核销两张销售单；先部分再结清 ──
            const fukuan = await openRole(browser, extra, "fukuan")
            const receipt1No = await registerReceiptAllocatingBothOrders(fukuan.page, {
                customerName: legalName,
                orderNos: [orderA.orderNo, orderB.orderNo],
                amount: RECEIPT_TOTAL,
                bankReference: `BANK-F06-1-${stamp}`,
            })
            await fukuan.context.close()

            const caiwu1 = await openRole(browser, extra, "caiwu")
            await approveWorkspaceTask(caiwu1.page, "回款复核", receipt1No)
            await caiwu1.context.close()

            await page.goto(`/sales/orders/${orderA.id}`)
            await expectCollection(page, "部分回款")
            await expectNotClosed(page)
            await page.goto(`/sales/orders/${orderB.id}`)
            await expectCollection(page, "部分回款")

            const fukuan2 = await openRole(browser, extra, "fukuan")
            const receipt2No = await registerReceiptAllocatingBothOrders(fukuan2.page, {
                customerName: legalName,
                orderNos: [orderA.orderNo, orderB.orderNo],
                amount: RECEIPT_TOTAL,
                bankReference: `BANK-F06-2-${stamp}`,
            })
            await fukuan2.context.close()

            const caiwu2 = await openRole(browser, extra, "caiwu")
            await approveWorkspaceTask(caiwu2.page, "回款复核", receipt2No)
            await caiwu2.context.close()

            await page.goto(`/sales/orders/${orderA.id}`)
            await expectCollection(page, "已结清")
            await expectInvoicing(page, "未开")
            await expectNotClosed(page)
            await expectFulfillmentNotStarted(page)
            await page.getByRole("tab", { name: /^采购/ }).click()
            await expect(page.getByTestId("sales-order-purchase-status")).toContainText(
                "采购单 0 笔",
                { timeout: TIMEOUT },
            )
            await expect(page.getByText("本单还没有采购单。")).toBeVisible({ timeout: TIMEOUT })

            await page.goto(`/sales/orders/${orderB.id}`)
            await expectCollection(page, "已结清")
            await expectNotClosed(page)

            // ── 5. 开票人：W01 开票任务登记销项发票；开票完成不是关闭条件 ──
            const kaipiao = await openRole(browser, extra, "kaipiao")
            await registerSalesInvoiceFromWorkspace(kaipiao.page, orderA.orderNo)
            await kaipiao.context.close()

            await page.goto(`/sales/orders/${orderA.id}`)
            await expectInvoicing(page, "已开齐")
            await expectCollection(page, "已结清")
            await expectNotClosed(page)

            // ── 6. 回款冲正：fukuan 提交 → lisiyong 确认依据 → caiwu 审批入账 ──
            const fukuan3 = await openRole(browser, extra, "fukuan")
            const reversalNo = await submitReceiptReversal(fukuan3.page, receipt2No)
            await fukuan3.context.close()

            const leader = await openRole(browser, extra, "lisiyong")
            await approveWorkspaceTask(leader.page, "回款冲正审批", reversalNo)
            await leader.context.close()

            const caiwu3 = await openRole(browser, extra, "caiwu")
            await approveWorkspaceTask(caiwu3.page, "回款冲正审批", reversalNo)
            await caiwu3.context.close()

            const fukuan4 = await openRole(browser, extra, "fukuan")
            await fukuan4.page.goto("/finance/customer-accounts?view=receipt")
            await expect(fukuan4.page.getByRole("heading", { name: "客户往来" })).toBeVisible({
                timeout: LONG,
            })
            await fukuan4.page.locator("#customer-receivables-toolbar-search").fill(receipt2No)
            await fukuan4.page.locator("#customer-receivables-toolbar-search").press("Enter")
            await expect(
                fukuan4.page.getByRole("row", { name: new RegExp(receipt2No) }).getByText("已冲正"),
            ).toBeVisible({ timeout: LONG })
            await fukuan4.context.close()

            await page.goto(`/sales/orders/${orderA.id}`)
            await expectCollection(page, "部分回款")
            await expectNotClosed(page)
            await page.goto(`/sales/orders/${orderB.id}`)
            await expectCollection(page, "部分回款")
            await expectNotClosed(page)
        } finally {
            await Promise.allSettled(extra.map((context) => context.close()))
        }
    })
})

// ─── 账号 / 登录 ───────────────────────────────────────────────────────────

function accountSpec(login: string) {
    const bag = ACCOUNTS as Record<string, unknown>
    const aliases: Record<string, string[]> = {
        xiaoshou: ["xiaoshou", "sales"],
        lisiyong: ["lisiyong", "salesLeader", "sales_leader"],
        caigou: ["caigou", "procurement"],
        caiwu: ["caiwu", "finance"],
        fukuan: ["fukuan", "payment"],
        kaipiao: ["kaipiao", "invoice"],
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

async function factValue(page: Page, label: string) {
    const dt = page.locator('[data-slot="formal-action-result"] dt', { hasText: label })
    await expect(dt).toBeVisible({ timeout: TIMEOUT })
    return (await dt.locator("xpath=following-sibling::dd[1]").innerText()).trim()
}

function escapeRe(value: string) {
    return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
}

async function waitHeading(page: Page, name: string | RegExp) {
    await expect(page.getByRole("heading", { name })).toBeVisible({ timeout: LONG })
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
    await page.goto(
        `/sales/contracts?customerId=${encodeURIComponent(input.customerId)}&upload=1`,
    )
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

async function expectFulfillmentNotStarted(page: Page) {
    await expect(
        page.locator("header").filter({ has: page.getByRole("heading", { level: 1 }) }).getByText("未开始", { exact: true }),
    ).toBeVisible({ timeout: TIMEOUT })
}

async function expectNotClosed(page: Page) {
    const identity = page
        .getByRole("heading", { level: 1 })
        .locator("xpath=ancestor::header[1]")
    await expect(identity.getByText("已生效", { exact: true })).toBeVisible({ timeout: LONG })
    await expect(identity.getByText("已关闭", { exact: true })).toHaveCount(0)
}

// ─── 工作台审批 ────────────────────────────────────────────────────────────

async function approveWorkspaceTask(page: Page, typeLabel: string, hint: string) {
    await page.goto("/workspace")
    await waitHeading(page, "我的工作台")
    const search = page.locator("#workspace-queue-toolbar-search-input")
    await search.fill(hint)
    await search.press("Enter")
    const task = page.getByRole("button", {
        name: new RegExp(`${escapeRe(typeLabel)}[\\s\\S]*${escapeRe(hint)}`),
    })
    await expect(task.first()).toBeVisible({ timeout: LONG })
    await task.first().click()
    const approve = page.getByRole("button", { name: "通过" })
    await expect(approve).toBeVisible({ timeout: LONG })
    await approve.click()
    await expect(page.getByRole("heading", { name: "确认通过" })).toBeVisible({ timeout: TIMEOUT })
    await page.getByRole("button", { name: "确认通过" }).click()
    await expect(page.getByRole("heading", { name: "确认通过" })).toBeHidden({ timeout: LONG })
    await expect(task.first()).toBeHidden({ timeout: LONG })
}

// ─── 回款核销 ──────────────────────────────────────────────────────────────

async function resolveReceiptPartyPicker(page: Page, customerName: string) {
    const sessionHeading = page.getByRole("heading", { name: /核销 · / })
    const picker = page.getByRole("dialog").filter({ hasText: "登记回款 — 选择往来主体" })
    await Promise.race([
        sessionHeading.waitFor({ state: "visible", timeout: LONG }),
        picker.waitFor({ state: "visible", timeout: LONG }),
    ])
    if (await picker.isVisible().catch(() => false)) {
        await chooseCombobox(page, "customer-receivables-party-picker-input", customerName)
        await page.locator("#customer-receivables-party-picker-confirm").click()
    }
}

async function startReceiptSession(page: Page, customerName: string) {
    await page.goto("/finance/customer-accounts")
    await waitHeading(page, "客户往来")
    const register = page.locator("#customer-receivables-header-register-receipt")
    await expect(register).toBeEnabled({ timeout: LONG })
    await register.click()
    await resolveReceiptPartyPicker(page, customerName)
    await expect(page.getByRole("heading", { name: /核销 · / })).toBeVisible({ timeout: LONG })
    await expect(page.getByRole("heading", { name: "同主体待核销池" })).toBeVisible({
        timeout: TIMEOUT,
    })
}

async function addPoolTarget(page: Page, orderNo: string) {
    const item = page
        .locator("section")
        .filter({ has: page.getByRole("heading", { name: /同主体待核销池/ }) })
        .locator("li")
        .filter({ hasText: orderNo })
    await expect(item).toBeVisible({ timeout: TIMEOUT })
    const joined = item.getByText("已加入")
    if (await joined.isVisible().catch(() => false)) return
    await item.getByRole("button", { name: "加入" }).click()
    await expect(joined).toBeVisible({ timeout: TIMEOUT })
}

async function setAllocationAmount(page: Page, orderNo: string, amount: string) {
    const amountBox = page.getByLabel(new RegExp(`${orderNo}.*分配金额`))
    await expect(amountBox).toBeVisible({ timeout: TIMEOUT })
    await amountBox.fill(amount)
}

async function registerReceiptAllocatingBothOrders(
    page: Page,
    input: {
        customerName: string
        orderNos: readonly [string, string]
        amount: string
        bankReference: string
    },
) {
    await startReceiptSession(page, input.customerName)
    await page.locator("#customer-receivables-session-amount").fill(input.amount)
    await page.locator("#customer-receivables-session-bank-reference").fill(input.bankReference)

    await addPoolTarget(page, input.orderNos[0])
    await setAllocationAmount(page, input.orderNos[0], SPLIT_AMOUNT)
    await addPoolTarget(page, input.orderNos[1])
    await setAllocationAmount(page, input.orderNos[1], SPLIT_AMOUNT)

    await page.locator("#customer-receivables-session-submit").click()
    await expect(page.getByRole("heading", { name: /提交回款|确认提交回款/ })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.locator("#customer-receivables-session-receipt-confirm-dialog-confirm").click()
    await expect(page.getByRole("heading", { name: "回款已提交审批" })).toBeVisible({
        timeout: LONG,
    })
    const receiptNo = await factValue(page, "回款单号")
    expect(receiptNo.length).toBeGreaterThan(2)
    await page.locator("#customer-receivables-session-result-close").click()
    await waitHeading(page, "客户往来")
    return receiptNo
}

async function assertCaiwuCannotSubmitReceipt(page: Page, customerName: string) {
    await page.goto("/finance/customer-accounts")
    await waitHeading(page, "客户往来")
    const register = page.locator("#customer-receivables-header-register-receipt")
    await expect(register).toBeVisible({ timeout: LONG })
    if (await register.isDisabled()) {
        await expect(register).toBeDisabled()
        return
    }
    await register.click()
    await resolveReceiptPartyPicker(page, customerName)
    await expect(page.getByRole("heading", { name: /核销 · / })).toBeVisible({ timeout: LONG })
    await page.locator("#customer-receivables-session-amount").fill("1.00")
    await page.locator("#customer-receivables-session-bank-reference").fill("CAI-WU-SHOULD-FAIL")
    const join = page.getByRole("button", { name: "加入" }).first()
    if (await join.isVisible().catch(() => false)) {
        await join.click()
        const fill = page.getByRole("button", { name: "填满" }).first()
        if (await fill.isVisible().catch(() => false)) await fill.click()
    }
    const submit = page.locator("#customer-receivables-session-submit")
    if (await submit.isEnabled()) {
        await submit.click()
        const confirm = page.locator("#customer-receivables-session-receipt-confirm-dialog-confirm")
        if (await confirm.isVisible().catch(() => false)) await confirm.click()
    }
    await expect(
        page.getByText(/提交人不得审批自己的单据|当前账号没有执行此操作的权限|操作未成功/),
    ).toBeVisible({ timeout: LONG })
}

// ─── 销项发票（W01 开票任务）──────────────────────────────────────────────

async function registerSalesInvoiceFromWorkspace(page: Page, orderNo: string) {
    await page.goto("/workspace")
    await waitHeading(page, "我的工作台")
    await page.locator("#workspace-family-nav-finance").click()
    const search = page.locator("#workspace-queue-toolbar-search-input")
    await search.fill(orderNo)
    await search.press("Enter")
    const task = page.getByRole("button", {
        name: new RegExp(`销项开票处理[\\s\\S]*${escapeRe(orderNo)}`),
    })
    await expect(task.first()).toBeVisible({ timeout: LONG })
    await task.first().click()
    await expect(page.getByLabel("当前开票任务")).toBeVisible({ timeout: LONG })
    await expect(page.getByRole("heading", { name: /核销 · / })).toBeVisible({ timeout: LONG })

    await page.locator("#customer-receivables-session-invoice-no").fill(`FP${Date.now()}`)
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
    await page.locator("#customer-receivables-session-invoice-confirm-dialog-confirm").click()
    await expect(page.getByRole("heading", { name: "销项发票已登记并分配" })).toBeVisible({
        timeout: LONG,
    })
}

// ─── 回款冲正 ──────────────────────────────────────────────────────────────

async function submitReceiptReversal(page: Page, receiptNo: string) {
    await page.goto("/finance/customer-accounts?view=receipt")
    await waitHeading(page, "客户往来")
    await page.locator("#customer-receivables-view-receipt").click()
    await page.locator("#customer-receivables-toolbar-search").fill(receiptNo)
    await page.locator("#customer-receivables-toolbar-search").press("Enter")
    const row = page.getByRole("row", { name: new RegExp(receiptNo) })
    await expect(row).toBeVisible({ timeout: LONG })
    await row.getByRole("button", { name: "预览" }).click()
    await expect(page.getByRole("heading", { name: receiptNo })).toBeVisible({ timeout: TIMEOUT })
    await page.locator("#customer-receivables-preview-receipt-reverse").click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "发起回款冲正" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.locator("#customer-receivables-reversal-reason").fill("错回款，按原单全额冲正重开应收")
    await page.locator("#customer-receivables-reversal-request-submit").click()
    await expect(page.getByRole("heading", { name: /提交冲正|确认提交冲正/ })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.locator("#customer-receivables-reversal-submit-confirm-dialog-confirm").click()
    await expect(page.getByRole("heading", { name: "冲正已提交审批" })).toBeVisible({
        timeout: LONG,
    })
    return factValue(page, "冲正单号")
}
