/**
 * [flow-36] 付款申请 review 回归：同主体历史更名与同会话付款后的先款履约缓存。
 * 只通过正式建档、销售、采购、任务改派和付款入口形成事实；不写库伪造来源或权限。
 */
import { randomUUID } from "node:crypto"

import { test, expect, type Page } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { analyticsDate, createAnalyticsPurchase } from "../helpers/financial-analytics"
import { openLoggedInWorkspace } from "../helpers/login"
import { BANK_RECEIPT_FILE, purchasePaymentRequestCommand, type PaymentRequestCandidate, type PurchasePaymentRequestResult } from "../helpers/payments"
import { assignReviewTask, createReviewFixture, createReviewOrder, reviewCommand, type ReviewCompany } from "../helpers/ppr-review-fixture"
import { confirmSupplyAllocation, expandSourcingEditor } from "../helpers/sourcing"
import { approveCurrentDocument, chooseOption, openFulfillmentWorkspaceForm, openWorkspaceTask } from "../helpers/ui"

const ROOT = "/admin/purchase-payment-requests"
const VISIBLE = { timeout: 20_000 }

type Detail = Omit<PurchasePaymentRequestResult, "lines"> & {
    payer_party_id: string; payer_party_name: string;
    current_lines: Array<{ purchase_order_id: string; source_changed: boolean; current_reserved_total: string }>
    lines: Array<{
        payable_account_id: string; purchase_order_id: string; requested_amount: string; paid_amount: string; closed_amount: string;
        source_snapshot: {
            purchase_preview: { payment_subject: { settlement_party_id: string; settlement_party_name: string } };
            sales_sources: Array<{ sales_order_id: string; payer_party_id: string; payer_party_name: string }>;
        };
    }>
}

function requestData(rows: PaymentRequestCandidate[]) {
    const first = rows[0]!
    const account = first.bank_accounts.find((item) => item.is_default) ?? first.bank_accounts[0]!
    expect(account).toBeTruthy()
    expect(rows.every((row) => row.bank_accounts.some((item) => item.id === account.id))).toBe(true)
    return { data: {
        supplier_id: first.supplier_id, payee_bank_account_id: account.id,
        desired_payment_date: analyticsDate(), reason: "采购核对历史主体名称与真实来源后申请", attachment_ids: [],
        lines: rows.map((row) => ({ payable_account_id: row.payable_account_id, amount: row.available_total })),
    } }
}

async function createSubmitted(page: Page, rows: PaymentRequestCandidate[]) {
    const draft = await purchasePaymentRequestCommand<PurchasePaymentRequestResult>(page, "caigou", "POST", ROOT, requestData(rows))
    const submitted = await purchasePaymentRequestCommand<PurchasePaymentRequestResult>(page, "caigou", "POST", `${ROOT}/${draft.id}/submit`, {
        expected_version: draft.version, idempotency_key: randomUUID(),
    })
    expect(submitted.status).toBe("in_approval")
    return submitted
}

function taskButton(page: Page, taskId: string) {
    const segment = taskId.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")
    return page.locator(`[id="workspace-task-${segment}"]`)
}

test.describe.configure({ mode: "serial" })

test("同一公司正常更名后，历史名称不同的采购可以合并提交并批准", async ({ browser }) => {
    test.setTimeout(6 * 60 * 1000)
    const procurement = await openLoggedInWorkspace(browser, "caigou")
    const adminToken = await apiToken("admin")
    const salesToken = await apiToken("xiaoshou")
    const procurementToken = await apiToken("caigou")
    const suffix = Date.now().toString(36)
    const fixture = await createReviewFixture(procurement.page, adminToken, salesToken, suffix)
    const originalName = fixture.company.legal_name
    const firstOrder = await createReviewOrder(procurement.page, procurementToken, salesToken, fixture, `${suffix}-A`, "service")
    const firstPurchase = await createAnalyticsPurchase(firstOrder, procurement.page, procurementToken)
    const original = fixture.company
    const renamedName = `E2E 付款主体更名 ${suffix}`
    fixture.company = await reviewCommand<ReviewCompany>(procurement.page, adminToken, "PUT", `/admin/companies/${original.id}`, {
        party_no: original.party_no, version: original.version, legal_name: renamedName, short_name: original.short_name,
        aliases: original.aliases, unified_credit_code: original.unified_credit_code, status: original.status,
    })
    expect(fixture.company.id, "更名只能推进同一主体的版本").toBe(original.id)
    expect(fixture.company.version).toBeGreaterThan(original.version)
    const secondOrder = await createReviewOrder(procurement.page, procurementToken, salesToken, fixture, `${suffix}-B`, "service")
    const secondPurchase = await createAnalyticsPurchase(secondOrder, procurement.page, procurementToken)
    const purchaseNames = new Map([[firstPurchase.purchaseId, originalName], [secondPurchase.purchaseId, renamedName]])
    const rows = (await apiGet<PaymentRequestCandidate[]>(procurementToken, `${ROOT}/candidates`))
        .filter((row) => purchaseNames.has(row.purchase_order_id))
        .sort((a, b) => a.purchase_order_id === firstPurchase.purchaseId ? -1 : b.purchase_order_id === firstPurchase.purchaseId ? 1 : 0)
    expect(rows).toHaveLength(2)
    expect(new Set(rows.map((row) => row.supplier_id)).size).toBe(1)
    expect(rows.every((row) => row.payer_party_id === original.id)).toBe(true)
    expect(rows.map((row) => row.payer_party_name)).toEqual([originalName, renamedName])
    const submitted = await createSubmitted(procurement.page, rows)
    const finance = await openLoggedInWorkspace(browser, "caiwu")
    await openWorkspaceTask(finance.page, /付款申请/, submitted.request_no, "approval")
    await approveCurrentDocument(finance.page)
    const approved = await apiGet<Detail>(procurementToken, `${ROOT}/${submitted.id}`)
    expect(approved).toMatchObject({ status: "approved", payer_party_id: original.id, payer_party_name: originalName })
    expect(approved.lines).toHaveLength(2)
    expect(approved.current_lines.every((line) => !line.source_changed), "同主体历史名称不同不得误报来源变更").toBe(true)
    for (const line of approved.lines) {
        const frozenName = purchaseNames.get(line.purchase_order_id)!
        expect(line.source_snapshot.purchase_preview.payment_subject).toMatchObject({
            settlement_party_id: original.id, settlement_party_name: frozenName,
        })
        expect(line.source_snapshot.sales_sources).toHaveLength(1)
        expect(line.source_snapshot.sales_sources[0]).toMatchObject({ payer_party_id: original.id, payer_party_name: frozenName })
    }
    expect((await apiGet<ReviewCompany>(adminToken, `/admin/companies/${original.id}`)).legal_name).toBe(renamedName)
})

test("同一合格账号付款后返回已缓存的先款履约任务，立即解除门槛且不重新加载页面", async ({ browser }) => {
    test.setTimeout(6 * 60 * 1000)
    const procurement = await openLoggedInWorkspace(browser, "caigou")
    const adminToken = await apiToken("admin")
    const salesToken = await apiToken("xiaoshou")
    const procurementToken = await apiToken("caigou")
    const suffix = `cache-${Date.now().toString(36)}`
    const fixture = await createReviewFixture(procurement.page, adminToken, salesToken, suffix)
    const order = await createReviewOrder(procurement.page, procurementToken, salesToken, fixture, suffix, "goods")
    await openWorkspaceTask(procurement.page, "待供给分配", order.order_no, "procurement")
    await expandSourcingEditor(procurement.page, "狮峰明前龙井礼盒")
    const editor = procurement.page.getByRole("region", { name: "当前商品供给编辑", exact: true })
    await chooseOption(procurement.page, editor.locator('[id$="-sourcing-option"]:visible'), "杭州狮峰茶叶有限公司 · 集采入仓", "入仓")
    await chooseOption(procurement.page, editor.locator('[id$="-warehouse"]:visible'), "BJ-TZ-01", "BJ-TZ-01")
    await confirmSupplyAllocation(procurement.page)
    const purchaseApprover = await openLoggedInWorkspace(browser, "caigoushenpi")
    await openWorkspaceTask(purchaseApprover.page, "采购单审批", order.order_no, "approval")
    await approveCurrentDocument(purchaseApprover.page)
    const queue = await apiGet<{ items: Array<{
        work_item_id: string; operation_id: string; purchase_order_id: string; gate_state: string;
    }> }>(await apiToken("cangchu"), "/admin/work-items/fulfillment-queue", {
        sales_order_id: order.id, operation_types: "RECEIPT", page: 1, page_size: 100,
    })
    expect(queue.items).toHaveLength(1)
    const receipt = queue.items[0]!
    expect(receipt.gate_state).toBe("BLOCKED")
    const rows = (await apiGet<PaymentRequestCandidate[]>(procurementToken, `${ROOT}/candidates`))
        .filter((row) => row.purchase_order_id === receipt.purchase_order_id)
    expect(rows).toHaveLength(1)
    const submitted = await createSubmitted(procurement.page, rows)
    const finance = await openLoggedInWorkspace(browser, "caiwu")
    await openWorkspaceTask(finance.page, /付款申请/, submitted.request_no, "approval")
    await approveCurrentDocument(finance.page)
    const approved = await apiGet<Detail>(procurementToken, `${ROOT}/${submitted.id}`)
    expect(approved.status).toBe("approved")
    expect(approved.work_item_id).toBeTruthy()
    // 管理员本身有两类完整业务资格；两个本规格任务都经正式候选及改派检查成为本人任务。
    await assignReviewTask(procurement.page, adminToken, receipt.work_item_id, "admin")
    await assignReviewTask(procurement.page, adminToken, approved.work_item_id!, "admin")
    const executor = await openLoggedInWorkspace(browser, "admin")
    const page = executor.page
    // 当前工作台不展示类型筛选；全部任务列表允许直接切换本人两类任务，无需导航页面。
    await page.goto("/workspace")
    await expect(taskButton(page, approved.work_item_id!)).toBeVisible(VISIBLE)
    await taskButton(page, receipt.work_item_id).click()
    let form = await openFulfillmentWorkspaceForm(page)
    await form.locator('[id^="fulfillment-operations-receipt-form-received-quantity-"]').first().fill("1")
    await chooseOption(page, form.locator('[id^="fulfillment-operations-receipt-form-quality-result-"]').first(), "合格", "合格")
    await expect(form.locator("#fulfillment-operations-work-surface-confirm")).toBeDisabled(VISIBLE)
    const gateTrigger = form.locator("#prepayment-gate")
    await expect(gateTrigger).toHaveCount(1)
    let gate = gateTrigger
    if (await gateTrigger.getAttribute("data-slot") === "collapsible-trigger") {
        if (await gateTrigger.getAttribute("aria-expanded") !== "true") await gateTrigger.click()
        gate = form.locator('#fulfillment-dialog-gate-details[data-slot="prepayment-gate"]')
    }
    await expect(gate).toHaveAttribute("data-allowed", "false", VISIBLE)
    const cachedAt = await page.evaluate(() => performance.now())
    const documentToken = randomUUID()
    await page.evaluate((token) => {
        document.documentElement.dataset.pprReviewDocument = token
    }, documentToken)
    let documentNavigations = 0
    let paymentApplied = false
    page.on("request", (request) => {
        if (request.isNavigationRequest() && request.frame() === page.mainFrame()) documentNavigations += 1
    })
    page.on("response", (response) => {
        if (new URL(response.url()).pathname === `${ROOT}/${approved.id}/payments/commit` && response.ok()) paymentApplied = true
    })
    const refreshedQueue = page.waitForResponse((response) => {
        const url = new URL(response.url())
        return paymentApplied && response.ok() && url.pathname === "/admin/work-items/fulfillment-queue" &&
            url.searchParams.get("operation_id") === receipt.operation_id
    }, { timeout: 45_000 })
    await form.locator('[data-slot="dialog-close"]').click()
    await expect(form).toBeHidden(VISIBLE)
    // 从缓存触发段开始仅使用 SPA 控件，禁止 goto、reload、刷新待办或更换账号。
    await taskButton(page, approved.work_item_id!).click()
    await expect(page.locator("#ppr-pay-form")).toBeVisible(VISIBLE)
    const line = approved.lines[0]!
    await expect(page.locator("#ppr-pay-amount")).toHaveValue(line.requested_amount)
    await page.locator("#ppr-pay-bank-receipt-input").setInputFiles(BANK_RECEIPT_FILE)
    await page.locator("#ppr-pay-submit").click()
    const posted = page.waitForResponse((response) => response.request().method() === "POST" &&
        new URL(response.url()).pathname === `${ROOT}/${approved.id}/payments/commit`)
    await page.locator("#ppr-pay-confirm-confirm").click()
    const paymentResponse = await posted
    expect(paymentResponse.ok(), await paymentResponse.text()).toBe(true)
    expect((await paymentResponse.json()).data.status).toBe("posted")
    await expect(page.locator("#ppr-pay-confirm-confirm")).toBeHidden(VISIBLE)
    // 付清会自动切换下一条并恢复队列焦点；等待收尾完成再开表单，避免旧回调关闭刚打开的弹窗。
    await expect(taskButton(page, approved.work_item_id!)).toHaveCount(0, VISIBLE)
    await expect(page.getByRole("status").filter({ hasText: /^任务已完成，已切换到采购入库/ })).toHaveCount(1, VISIBLE)
    await expect(taskButton(page, receipt.work_item_id)).toBeFocused(VISIBLE)
    await expect.poll(() => new URL(page.url()).searchParams.get("currentWorkItemId"), VISIBLE).toBe(receipt.work_item_id)
    form = await openFulfillmentWorkspaceForm(page)
    await form.locator('[id^="fulfillment-operations-receipt-form-received-quantity-"]').first().fill("1")
    await chooseOption(page, form.locator('[id^="fulfillment-operations-receipt-form-quality-result-"]').first(), "合格", "合格")
    const refreshed = await refreshedQueue
    const refreshedEnvelope = await refreshed.json() as { data: { items: Array<{ operation_id: string; gate_state: string }> } }
    expect(refreshedEnvelope.data.items.find((item) => item.operation_id === receipt.operation_id)?.gate_state).toBe("SATISFIED")
    await expect(form.locator("#fulfillment-operations-work-surface-confirm")).toBeEnabled(VISIBLE)
    expect(await page.evaluate(() => performance.now()) - cachedAt, "必须在旧查询60秒有效期内验证主动失效，不能等待缓存自然过期").toBeLessThan(60_000)
    expect(documentNavigations, "验证过程不得通过整页导航清除 Query 缓存").toBe(0)
    expect(await page.evaluate(() => document.documentElement.dataset.pprReviewDocument), "付款前后必须保留同一 document 及其 QueryClient").toBe(documentToken)
    expect((await apiGet<Detail>(procurementToken, `${ROOT}/${submitted.id}`)).status).toBe("paid")
})
