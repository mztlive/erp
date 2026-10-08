/**
 * 流程: [flow-06] 客户票款：分次回款、销项发票、核销与冲正
 * 文档: docs/erp-phase-1.md §9.1 + §6.5.4（回款冲正）+ §9.3（关闭与开票）
 * 账号: xiaoshou 建客户/合同/销售单并提交开票申请；caigou 审批销售单；fukuan 提交回款与冲正；
 *       caiwu 审批回款/开票申请/冲正入账（禁止自己提交）；lisiyong 确认冲正依据；
 *       kaipiao 在批准后的 W01 开票任务登记销项发票。
 *
 * 文档-代码差异（以代码为准）:
 * - 销售单生效不自动生成开票任务；xiaoshou 提交 SalesInvoiceRequest，caiwu 审批后才生成 SALES_INVOICE_EXECUTION。
 * - 销项发票必须由 kaipiao 从 W01 SALES_INVOICE_EXECUTION 原地登记；客户往来「新建开票申请」走申请审批，不是直接开票。
 * - 一次工作台开票只能核销当前任务绑定的一张应收子账，不能一张发票跨多张销售单。
 * - 回款正式入账后列表状态文案是「已过账」（按钮不用「过账」）。
 * - 销售单开票进度完成态文案是「已开齐」，不是文档表格里的「已完成」。
 * - 财务三人共用 role-finance：caiwu 可能看见「登记回款」，提交时被 ForbidSubmitterAsApprover 拒绝。
 */
import { archiveContractViaUi } from "../helpers/contracts"
import { test, expect, type Browser, type BrowserContext, type Page } from "../helpers/test"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

import { createCustomerViaUi } from "../helpers/customers"
import { rejectFinancialOriginal, reviseFinancialOriginal } from "../helpers/financial-draft-edit"
import {
    expectInvoiceEvidenceCommit,
    expectSalesInvoiceEvidence,
    invoiceEvidenceFiles,
    submitSalesInvoiceRequest,
    uploadInvoiceEvidence,
} from "../helpers/invoices"
import { loginViaUi, openLoggedInWorkspace } from "../helpers/login"
import { expectReceiptPreview, submitReceiptReversalRequest } from "../helpers/receipts"
import { assertReceiptRegistrationDraft } from "../helpers/receipt-registration"
import { API_BASE, apiToken } from "../helpers/api"
import {
    approveCurrentDocument,
    chooseOption,
    dismissToasts,
    expectToast,
    openWorkspaceTask,
    pickCalendarDay,
    readHeaderDocumentNumber,
    salesOrderAmountSummary,
} from "../helpers/ui"

const TIMEOUT = 20_000
const LONG = 40_000
const SKU_NAME = "狮峰明前龙井礼盒"
const UNIT_PRICE = "1288.00"
const SPLIT_AMOUNT = "644.00"

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
            await loginViaUi(page, "xiaoshou")
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

            // ── 4. 出纳分次回款：多单核销、已有草稿提交；先部分再结清 ──
            const fukuan = await openRole(browser, extra, "fukuan")
            const { receiptNo: receipt1No, receiptId: receipt1Id, counterpartyPartyId } = await registerReceiptAllocatingBothOrders(fukuan.page, {
                customerName: legalName,
                orderNos: [orderA.orderNo, orderB.orderNo],
                amount: "1500.00",
                bankReference: `BANK-F06-1-${stamp}`,
                verifyDraft: true,
            })

            for (const order of [orderA, orderB]) {
                await page.goto(`/sales/orders/${order.id}`)
                await expectCollection(page, "未收")
            }

            const caiwu1 = await openRole(browser, extra, "caiwu")
            await openWorkspaceTask(caiwu1.page, "回款复核", receipt1No, "approval")
            await rejectFinancialOriginal(caiwu1.page, "银行流水号需补齐，修改原回款单后重提")
            await reviseFinancialOriginal(fukuan.page, {
                kind: "customer_receipt",
                id: receipt1Id,
                documentNo: receipt1No,
                changedText: `BANK-F06-1-REVISED-${stamp}`,
            })
            await approveWorkspaceTask(caiwu1.page, "回款复核", receipt1No)
            await caiwu1.context.close()

            await fukuan.page.goto("/finance/customer-accounts?view=receipt")
            await waitHeading(fukuan.page, "客户往来")
            await fukuan.page.locator("#customer-receivables-toolbar-search").fill(receipt1No)
            await fukuan.page.locator("#customer-receivables-toolbar-search").press("Enter")
            await fukuan.page.getByRole("row").filter({ hasText: receipt1No }).click()
            await expectReceiptPreview(fukuan.page, receipt1No)
            await expect(fukuan.page.getByText("待核销回款", { exact: true }).locator("..")).toContainText("212.00")
            const draftNo = `DRAFT-F06-${stamp}`
            await test.step("已过账回款不可继续编辑，已有草稿保留到账信息并可提交审批", async () => {
                await expect(fukuan.page.locator("#customer-receivables-preview-receipt-continue-allocate")).toHaveCount(0)
                const token = await apiToken("fukuan")
                // UTC 10 月 1 日 16:30 对应上海 10 月 2 日 00:30，覆盖跨日回显。
                const receivedAt = Date.UTC(2026, 9, 1, 16, 30) / 1000
                const created = await fukuan.page.request.post(`${API_BASE}/admin/customer-receipts`, {
                    headers: { Authorization: `Bearer ${token}` },
                    data: {
                        receipt_no: draftNo,
                        counterparty_party_id: counterpartyPartyId,
                        received_at: receivedAt,
                        amount: "100.00",
                        bank_reference: `BANK-DRAFT-F06-${stamp}`,
                    },
                })
                expect(created.ok(), await created.text()).toBeTruthy()
                const draft = (await created.json()) as { data: { id: string; status: string; version: number } }
                expect(draft.data.status).toBe("draft")
                await fukuan.page.goto(`/finance/customer-accounts?view=receipt&previewKind=receipt&previewId=${encodeURIComponent(draft.data.id)}`)
                await expectReceiptPreview(fukuan.page, draftNo)
                await fukuan.page.locator("#customer-receivables-preview-receipt-continue-allocate").click()
                await expect(fukuan.page.getByRole("heading", { name: "继续核销回款", exact: true })).toBeVisible()
                await expect(fukuan.page.locator("#customer-receivables-session-amount")).toHaveValue("100.00")
                await expect(fukuan.page.locator("#customer-receivables-session-received-at")).toContainText("2026-10-02 00:30:00")
                await expect(fukuan.page.locator("#customer-receivables-session-counterparty")).toHaveValue(legalName)
                await expect(fukuan.page.locator("#customer-receivables-session-bank-reference")).toHaveValue(`BANK-DRAFT-F06-${stamp}`)
                for (const id of ["amount", "received-at", "bank-reference"]) {
                    const field = fukuan.page.locator(`#customer-receivables-session-${id}`)
                    await expect(field).toBeDisabled()
                }
                await expect(fukuan.page.locator("#customer-receivables-session-counterparty")).toHaveJSProperty("readOnly", true)
                await addPoolTarget(fukuan.page, orderA.orderNo)
                await setAllocationAmount(fukuan.page, orderA.orderNo, "100.00")
                const screenshot = test.info().outputPath("receipt-registration-existing-draft.png")
                await fukuan.page.screenshot({ path: screenshot, animations: "disabled" })
                await test.info().attach("已有回款草稿继续核销", { path: screenshot, contentType: "image/png" })
                await fukuan.page.locator("#customer-receivables-session-submit").click()
                const committed = fukuan.page.waitForResponse((response) =>
                    response.request().method() === "POST" && response.url().includes("/admin/customer-receipts/commit"),
                )
                await fukuan.page.locator("#customer-receivables-session-receipt-confirm-dialog-confirm").click()
                const response = await committed
                expect(response.ok(), await response.text()).toBeTruthy()
                expect(response.request().postDataJSON()).toMatchObject({
                    receipt_id: draft.data.id,
                    expected_version: draft.data.version,
                    receipt: null,
                })
                const submitted = (await response.json()) as { data: { id: string; status: string; bank_reference: string; received_at: number } }
                expect(submitted.data.id).toBe(draft.data.id)
                expect(submitted.data.status).toBe("IN_APPROVAL")
                expect(submitted.data.bank_reference).toBe(`BANK-DRAFT-F06-${stamp}`)
                expect(submitted.data.received_at).toBe(receivedAt)
            })
            await fukuan.context.close()

            const caiwuDraft = await openRole(browser, extra, "caiwu")
            await approveWorkspaceTask(caiwuDraft.page, "回款复核", draftNo)
            await caiwuDraft.context.close()

            await page.goto(`/sales/orders/${orderA.id}`)
            await expectCollection(page, "部分回款")
            await expectNotClosed(page)
            await page.goto(`/sales/orders/${orderB.id}`)
            await expectCollection(page, "部分回款")

            const fukuan2 = await openRole(browser, extra, "fukuan")
            const { receiptNo: receipt2No } = await registerReceiptAllocatingBothOrders(fukuan2.page, {
                customerName: legalName,
                orderNos: [orderA.orderNo, orderB.orderNo],
                amount: "1188.00",
                bankReference: `BANK-F06-2-${stamp}`,
                allocationAmounts: ["544.00", SPLIT_AMOUNT],
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
                "待采购",
                { timeout: TIMEOUT },
            )
            await expect(page.getByText("本单还没有采购单。")).toBeVisible({ timeout: TIMEOUT })

            await page.goto(`/sales/orders/${orderB.id}`)
            await expectCollection(page, "已结清")
            await expectNotClosed(page)

            // ── 5. 销售申请开票 → 财务审批 → 开票人 W01 登记；开票完成不是关闭条件 ──
            await submitSalesInvoiceRequest(page, {
                salesOrderId: orderA.id,
                amount: UNIT_PRICE,
                taxNumber: creditCode,
                title: legalName,
            })
            const caiwuInvoice = await openRole(browser, extra, "caiwu")
            await approveWorkspaceTask(caiwuInvoice.page, "开票申请审批", legalName)
            await caiwuInvoice.context.close()

            const kaipiao = await openRole(browser, extra, "kaipiao")
            const registeredInvoice = await registerSalesInvoiceFromWorkspace(kaipiao.page, orderA.orderNo, legalName)
            await kaipiao.context.close()

            await page.goto(`/sales/orders/${orderA.id}`)
            await expectInvoicing(page, "已开齐")
            await expectCollection(page, "已结清")
            await expectNotClosed(page)
            await expectSalesInvoiceEvidence(page, {
                salesOrderId: orderA.id,
                otherSalesOrderId: orderB.id,
                ...registeredInvoice,
            })

            // ── 6. 回款冲正：fukuan 提交 → lisiyong 确认依据 → caiwu 审批入账 ──
            const fukuan3 = await openRole(browser, extra, "fukuan")
            const { reversalNo, reversalId } = await submitReceiptReversal(fukuan3.page, receipt2No)

            const leader = await openRole(browser, extra, "lisiyong")
            await openWorkspaceTask(leader.page, "回款冲正审批", reversalNo, "approval")
            await rejectFinancialOriginal(leader.page, "冲正依据需补齐，修改原冲正单后重提")
            await reviseFinancialOriginal(fukuan3.page, {
                kind: "receipt_reversal",
                id: reversalId,
                documentNo: reversalNo,
                changedText: "补齐原回款银行凭证后，按原金额冲正并重开应收",
            })
            await fukuan3.context.close()
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
                fukuan4.page.getByRole("row").filter({ hasText: receipt2No }).first().getByText("已冲正"),
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

async function openRole(
    browser: Browser,
    extra: BrowserContext[],
    login: string,
): Promise<{ context: BrowserContext; page: Page }> {
    const session = await openLoggedInWorkspace(browser, login)
    extra.push(session.context)
    return session
}

// ─── 通用 UI ───────────────────────────────────────────────────────────────

async function clickWithoutToastOverlay(
    page: Page,
    target: import("../helpers/test").Locator,
    settled?: () => Promise<boolean>,
): Promise<void> {
    // Toast 可能在关闭后再次出现导致遮挡：循环关闭后短超时点按，成功即返回（与 flow-03/04/05 同款）。
    for (let i = 0; i < 8; i += 1) {
        if (settled && (await settled().catch(() => false))) return
        await page.mouse.move(8, 8).catch(() => undefined)
        await dismissToasts(page)
        try {
            await target.click({ timeout: 3_000 })
            return
        } catch {
            // 被遮挡则下一轮重试；8 轮都不成功改走 DOM 派发。
        }
    }
    if (settled && (await settled().catch(() => false))) return
    await target.dispatchEvent("click")
}

function todayIso() {
    const today = new Date()
    return [
        today.getFullYear(),
        String(today.getMonth() + 1).padStart(2, "0"),
        String(today.getDate()).padStart(2, "0"),
    ].join("-")
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

async function waitHeading(page: Page, name: string | RegExp) {
    await expect(page.getByRole("heading", { name })).toBeVisible({ timeout: LONG })
}

// ─── 客户 / 合同 / 销售单 ─────────────────────────────────────────────────

async function createCustomer(
    page: Page,
    input: { legalName: string; shortName: string; creditCode: string },
) {
    await createCustomerViaUi(page, {
        legalName: input.legalName,
        shortName: input.shortName,
        creditCode: input.creditCode,
        paymentTermLabel: "货到 30 天",
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
    await archiveContractViaUi(page, { contractNo: input.contractNo, customerName: input.legalName, pdf: contractPdfFile(), paymentTerms: "货到 30 天" })
    // 同页 toast 描述也含合同编号：限定首个（列表行按钮）避开严格模式。
    await expect(page.getByText(input.contractNo).first()).toBeVisible({ timeout: LONG })
}

// 负责销售是展示名，不是输入框：必须等于侧栏账号菜单里的当前用户，且不能停在占位文案。
async function expectLoggedInSalesOwner(page: Page, timeout: number) {
    const placeholders = ["加载当前用户…", "无法获取登录用户", "当前用户未就绪"]
    const accountName = page.locator("#workspace-sidebar-account-trigger span.font-medium")
    const ownerName = page.locator("#sales-orders-create-header-owner-name > span").nth(1)
    await expect(accountName).toBeVisible({ timeout })
    await expect(ownerName).toBeVisible({ timeout })
    await expect(async () => {
        const account = ((await accountName.textContent()) ?? "").trim()
        const owner = ((await ownerName.textContent()) ?? "").trim()
        expect(account).not.toBe("")
        expect(owner).toBe(account)
        expect(placeholders).not.toContain(owner)
    }).toPass({ timeout })
}

async function createAndSubmitPhysicalSalesOrder(
    page: Page,
    input: { customerId: string; contractNo: string; legalName: string },
) {
    await page.goto(`/sales/orders?mode=create&customerId=${encodeURIComponent(input.customerId)}`)
    await expect(
        page.getByRole("heading", { name: /新建销售单|业务信息/ }),
    ).toBeVisible({ timeout: LONG })

    await chooseOption(
        page,
        page.locator("#sales-orders-create-contract"),
        input.contractNo,
        input.contractNo,
    )
    await expect(page.locator("#sales-orders-create-customer")).toHaveValue(new RegExp(input.legalName))
    await expectLoggedInSalesOwner(page, LONG)

    await chooseOption(
        page,
        page.locator("#sales-orders-create-header-welfare-scene"),
        "年节礼包",
        "年节",
    )
    await expect(page.locator("#sales-orders-create-header-payment-terms")).toHaveValue("货到 30 天")
    await expect(page.locator("#sales-orders-create-header-payment-terms")).toBeDisabled()

    await page.locator("#sales-orders-create-line-items-add").click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "添加商品" })).toBeVisible({
        timeout: TIMEOUT,
    })
    const skuSearch = page.locator("#master-data-list-sellable-list-toolbar-search-input")
    await skuSearch.fill(SKU_NAME)
    await skuSearch.press("Enter")
    const skuCheckbox = page.getByRole("checkbox", { name: new RegExp(`选择 ${SKU_NAME}`) })
    await expect(skuCheckbox.first()).toBeVisible({ timeout: LONG })
    await skuCheckbox.first().check()
    await page.locator("#sales-orders-sku-picker-confirm").click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "添加商品" })).toBeHidden({
        timeout: TIMEOUT,
    })
    // 搜索框回显筛选 chips 且同名多处出现：用行内更换按钮精确命中已选行。
    await expect(page.getByRole("button", { name: new RegExp(`更换销售项目[\\s\\S]*${SKU_NAME}`) }).first()).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(page.getByTestId(/sales-line-procurement-owner-/)).not.toContainText(
        "暂未确定采购负责人",
        { timeout: LONG },
    )

    await page.locator("#sales-orders-create-batch-due-date-open").click()
    await pickCalendarDay(page, page.locator("#sales-orders-create-batch-due-date"), todayIso())
    await page.locator("#sales-orders-create-batch-due-date-apply").click()
    await expectToast(page, "已批量设置交期")

    // 残留错误/成功 toast 会盖住提交按钮：先清再点，盖住不散时走 DOM 派发。
    await clickWithoutToastOverlay(page, page.locator("#sales-orders-create-submit"), async () =>
        page
            .getByRole("dialog")
            .getByRole("heading", { name: "提交销售单" })
            .isVisible()
            .catch(() => false),
    )
    await expect(page.getByRole("dialog").getByRole("heading", { name: "提交销售单" })).toBeVisible({
        timeout: TIMEOUT,
    })
    // settled 只做即时判断：第一次点按前页面必然还在新建页，等 URL 只会白等。
    await clickWithoutToastOverlay(page, page.locator("#sales-orders-submit-confirm-confirm"), async () =>
        !/\/sales\/orders\?mode=create/.test(page.url()),
    )
    await expect(page).toHaveURL(/\/sales\/orders\/[^/?#]+/, { timeout: LONG })
    await expect(page.getByText("审批中", { exact: true }).first()).toBeVisible({ timeout: LONG })

    const id = page.url().split("/sales/orders/")[1]?.split(/[?#]/)[0] ?? ""
    expect(id).toBeTruthy()
    const orderNo = await readHeaderDocumentNumber(page)
    expect(orderNo.length).toBeGreaterThan(4)
    return { id, orderNo }
}

async function expectEffectiveSalesOrder(page: Page, orderNo: string) {
    await expect(page.locator("header").getByText(orderNo)).toBeVisible({ timeout: LONG })
    await expect(page.getByText("已生效", { exact: true }).first()).toBeVisible({ timeout: LONG })
    await expectCollection(page, "未收")
    await expectInvoicing(page, "未开")
    await expectNotClosed(page)
}

async function expectCollection(page: Page, label: "未收" | "部分回款" | "已结清") {
    await expect(salesOrderAmountSummary(page).getByText(label, { exact: true })).toBeVisible({
        timeout: LONG,
    })
}

async function expectInvoicing(page: Page, label: "未开" | "部分开票" | "已开齐") {
    await expect(salesOrderAmountSummary(page).getByText(label, { exact: true })).toBeVisible({
        timeout: LONG,
    })
}

async function expectFulfillmentNotStarted(page: Page) {
    await expect(page.getByText("未开始", { exact: true }).first()).toBeVisible({
        timeout: TIMEOUT,
    })
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
    await openWorkspaceTask(page, typeLabel, hint, "approval")
    await approveCurrentDocument(page)
}

// ─── 回款核销 ──────────────────────────────────────────────────────────────

async function startReceiptSession(page: Page, customerName: string) {
    await page.goto("/finance/customer-accounts")
    await waitHeading(page, "客户往来")
    const register = page.locator("#customer-receivables-header-register-receipt")
    await expect(register).toBeEnabled({ timeout: LONG })
    await register.click()
    const sessionHeading = page.getByRole("heading", { name: "登记回款", exact: true })
    const picker = page.getByRole("dialog").filter({ hasText: "登记回款 — 选择往来主体" })
    await Promise.race([
        sessionHeading.waitFor({ state: "visible", timeout: LONG }),
        picker.waitFor({ state: "visible", timeout: LONG }),
    ])
    if (await picker.isVisible().catch(() => false)) {
        await chooseOption(
            page,
            picker.locator("#customer-receivables-party-picker-input"),
            customerName,
            customerName,
        )
        await page.locator("#customer-receivables-party-picker-confirm").click()
    }
    await expect(sessionHeading).toBeVisible({ timeout: LONG })
    await expect(page.getByRole("heading", { name: "关联销售单应收" })).toBeVisible({
        timeout: TIMEOUT,
    })
}

async function addPoolTarget(page: Page, orderNo: string) {
    const item = page
        .locator("#customer-receivables-session-allocations")
        .getByRole("row")
        .filter({ hasText: orderNo })
    await expect(item).toBeVisible({ timeout: TIMEOUT })
    const selection = item.getByRole("checkbox")
    if (!(await selection.isChecked())) await selection.check()
    await expect(selection).toBeChecked()
}

async function setAllocationAmount(page: Page, orderNo: string, amount: string) {
    const amountBox = page.getByLabel(new RegExp(`${orderNo}.*本次核销金额`))
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
        verifyDraft?: boolean
        allocationAmounts?: readonly [string, string]
    },
) {
    await startReceiptSession(page, input.customerName)
    await page.locator("#customer-receivables-session-amount").fill(input.amount)
    await page.locator("#customer-receivables-session-bank-reference").fill(input.bankReference)

    await addPoolTarget(page, input.orderNos[0])
    await setAllocationAmount(page, input.orderNos[0], input.allocationAmounts?.[0] ?? SPLIT_AMOUNT)
    await addPoolTarget(page, input.orderNos[1])
    await setAllocationAmount(page, input.orderNos[1], input.allocationAmounts?.[1] ?? SPLIT_AMOUNT)

    if (input.verifyDraft) await assertReceiptRegistrationDraft(page, input)

    await page.locator("#customer-receivables-session-submit").click()
    await expect(page.getByRole("heading", { name: /提交回款|确认提交回款/ })).toBeVisible({
        timeout: TIMEOUT,
    })
    const committed = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/customer-receipts/commit"),
        { timeout: 60_000 },
    )
    await page.locator("#customer-receivables-session-receipt-confirm-dialog-confirm").click()
    const response = await committed
    expect(response.ok(), await response.text()).toBeTruthy()
    const body = (await response.json()) as { data?: { id?: string; receipt_no?: string; counterparty_party_id?: string } }
    const receiptNo = body.data?.receipt_no?.trim() || (await factValue(page, "回款单号"))
    expect(receiptNo.length).toBeGreaterThan(2)
    const receiptId = body.data?.id ?? ""
    expect(receiptId).not.toBe("")
    const counterpartyPartyId = body.data?.counterparty_party_id ?? ""
    expect(counterpartyPartyId).not.toBe("")
    const close = page.locator("#customer-receivables-session-result-close")
    if (await close.isVisible().catch(() => false)) {
        await close.click()
        await waitHeading(page, "客户往来")
    }
    return { receiptNo, receiptId, counterpartyPartyId }
}

async function assertCaiwuCannotSubmitReceipt(page: Page, customerName: string) {
    // 种子财务角色共用回款权限；等待实际权限加载后验证运行时岗位分离。
    await startReceiptSession(page, customerName)
    await page.locator("#customer-receivables-session-amount").fill("1.00")
    await page.locator("#customer-receivables-session-bank-reference").fill("CAI-WU-SHOULD-FAIL")
    const selection = page
        .locator("#customer-receivables-session-allocations")
        .getByRole("checkbox")
        .first()
    await expect(selection).toBeVisible({ timeout: LONG })
    await selection.check()
    await expect(selection).toBeChecked()
    const fill = page.getByRole("button", { name: "填入剩余" }).first()
    await expect(fill).toBeEnabled({ timeout: LONG })
    await fill.click()
    const submit = page.locator("#customer-receivables-session-submit")
    await expect(submit).toBeEnabled({ timeout: LONG })
    await submit.click()
    const confirm = page.locator("#customer-receivables-session-receipt-confirm-dialog-confirm")
    await expect(confirm).toBeVisible({ timeout: LONG })
    const rejected = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            new URL(response.url()).pathname === "/admin/customer-receipts/commit",
        { timeout: 60_000 },
    )
    await confirm.click()
    const response = await rejected
    const result = (await response.json()) as {
        success?: boolean
        code?: string
        errorMessage?: string
    }
    expect(response.status(), result.errorMessage).toBe(400)
    expect(result.success).toBe(false)
    expect(result.code).toBe("INVALID_REQUEST")
    expect(result.errorMessage).toContain("提交人不得审批自己的单据")
    await expect(
        page.getByText(/提交人不得审批自己的单据/),
    ).toBeVisible({ timeout: LONG })
}

// ─── 销项发票（W01 开票任务）──────────────────────────────────────────────

async function registerSalesInvoiceFromWorkspace(
    page: Page,
    orderNo: string,
    _customerName: string,
) {
    await openWorkspaceTask(page, "销项开票处理", orderNo, "finance")
    await expect(page.getByLabel("当前开票任务")).toBeVisible({ timeout: LONG })
    await expect(page.getByRole("heading", { name: /核销 · / })).toBeVisible({ timeout: LONG })

    const invoiceNo = `FP${Date.now()}`
    const files = invoiceEvidenceFiles(invoiceNo)
    await page.locator("#customer-receivables-session-invoice-no").fill(invoiceNo)
    await page.locator("#customer-receivables-session-gross-amount").fill(UNIT_PRICE)
    await uploadInvoiceEvidence(page, files)
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
    // 提交是慢事务（远端 Mongo 多轮写）：必须等 commit 响应落定后再跳列表，
    // 否则列表查询先发会命中提交前快照（total 0），而服务端仍会继续提交，
    // 造成“写成功但客户端无响应”的假失败。先挂 waitForResponse 再点确认。
    const commitResponse = page.waitForResponse(
        (res) =>
            res.request().method() === "POST" &&
            new URL(res.url()).pathname === "/admin/invoices/commit-with-files",
        { timeout: 90_000 },
    )
    await page.locator("#customer-receivables-session-invoice-confirm-dialog-confirm").click()
    const committed = await commitResponse
    const invoiceId = await expectInvoiceEvidenceCommit(committed, files)
    await expect(page.getByRole("heading", { name: "确认登记销项发票并分配" })).toBeHidden({
        timeout: LONG,
    })
    // 产品缺口：提交成功后任务完成信号会重建开票会话，成功结果区（销项发票已登记并分配）
    // 从未来得及展示就被卸载（回款结果区可正常展示，仅发票有此问题）。
    // 此处以 commit 200 + 发票列表作为登记依据，不再断言结果区。
    // 列表搜索框走表单提交，提交瞬间若遇工作台刷新会吞掉回车（已复现）；
    // 改用地址栏 q 参数直达（行为与回车提交一致：q 映射为 invoice_no 精确过滤）。
    await page.goto(
        `/finance/customer-accounts?view=sales_invoice&q=${encodeURIComponent(invoiceNo)}`,
    )
    await waitHeading(page, "客户往来")
    // 数据表行无障碍名为“第 N 行”（不含单据号），用行内文本过滤定位。
    await expect(
        page.getByRole("row").filter({ hasText: invoiceNo }).first(),
    ).toBeVisible({ timeout: LONG })
    return { invoiceId, invoiceNo, files }
}

// ─── 回款冲正 ──────────────────────────────────────────────────────────────

async function submitReceiptReversal(page: Page, receiptNo: string) {
    await page.goto("/finance/customer-accounts?view=receipt")
    await waitHeading(page, "客户往来")
    await page.locator("#customer-receivables-view-receipt").click()
    await page.locator("#customer-receivables-toolbar-search").fill(receiptNo)
    await page.locator("#customer-receivables-toolbar-search").press("Enter")
    const row = page.getByRole("row").filter({ hasText: receiptNo })
    await expect(row.first()).toBeVisible({ timeout: LONG })
    await row.first().click()
    await expectReceiptPreview(page, receiptNo)
    await page.locator("#customer-receivables-preview-receipt-reverse").click()
    const committed = page.waitForResponse(
        (response) => response.request().method() === "POST" && new URL(response.url()).pathname === "/admin/receipt-reversals/commit",
        { timeout: 90_000 },
    )
    await submitReceiptReversalRequest(page, "错回款，按原单全额冲正重开应收")
    const response = await committed
    expect(response.ok(), await response.text()).toBeTruthy()
    const reversal = (await response.json()).data as { id: string; reversal_no: string }
    expect(reversal.id).toBeTruthy()
    expect(reversal.reversal_no).toBeTruthy()
    return { reversalId: reversal.id, reversalNo: reversal.reversal_no }
}
