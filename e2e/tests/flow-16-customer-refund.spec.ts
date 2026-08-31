/**
 * 流程: [flow-16] 客户退款单（含驳回轮次 + 供应商退款）
 * 文档: docs/erp-phase-1.md §6.3 / §6.5.4；approval-workflow-contract.md §4.3/§4.4；
 *       workbench-workitem-contract.md 第 3 节
 * 账号: xiaoshou 建客户/合同/销售单；caigou 审批销售单、供给分配、确认供应商退款依据；
 *       fukuan 提交客户回款/客户退款/供应商退款（禁止 caiwu 自己提交）；
 *       lisiyong 确认客户退款依据（含驳回后轮次加一回到首节点）；
 *       caiwu 审批回款入账、客户退款出账、采购单、供应商退款入账。
 *
 * 文档-代码差异（以代码为准）:
 * 1. 文档 §6.5.4 画「业务部门先确认依据、财务经办再建单」；代码由出纳 fukuan 一次
 *    commit（/admin/customer-refunds/commit、/admin/supplier-refunds/commit）创建并
 *    启动审批，销售领导/采购作为审批首节点确认依据。
 * 2. 文档写「过账」；按钮不用「过账」，状态徽标仍是「已过账」。终态由末节点通过
 *    原子执行 post_*，页面没有独立确认入账按钮。
 * 3. 文档客户侧业务部门写「销售」；已发布定义首节点是「销售领导确认退款依据」
 *    （lisiyong），不是 xiaoshou。
 * 4. 客户往来没有退款 Tab（仅应收/回款/销项发票/待核销）；退款详情走 previewKind=refund。
 * 5. 合同 §4.4.2：驳回保持 IN_APPROVAL、轮次加一回到入口节点；前端 REJECTED 文案
 *    映射为「草稿」，但驳回后页面仍展示「审批中」+「第 2 轮」。
 */
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

import { test, expect, type Browser, type BrowserContext, type Page } from "@playwright/test"

import { ACCOUNTS } from "../helpers/accounts"
import { loginViaUi, newLoggedInContext } from "../helpers/login"
import "../helpers/ui"

test.describe.configure({ mode: "serial" })
test.use({ viewport: { width: 1440, height: 960 } })

const TIMEOUT = 20_000
const LONG = 40_000
const SKU_NAME = "狮峰明前龙井礼盒"
const SUPPLIER_SHORT = "狮峰茶叶"
const CUSTOMER_NODE = "销售领导确认退款依据"
const FINANCE_NODE = "财务总监审批"
const PROCUREMENT_NODE = "采购确认退款依据"
const REJECT_REASON = "退款依据不足，请补充原回款与客户约定后再报"
const CUSTOMER_REFUND_REASON = "客户取消订单，按已入账回款全额退回资金，原回款保留"
const SUPPLIER_REFUND_REASON = "供应商退回多收款，按已入账付款全额退款，原付款保留"
const RECEIPT_PNG = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    "base64",
)

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const CONTRACT_PDF = path.resolve(REPO_ROOT, "fixtures", "sample-contract.pdf")

test.describe("flow-16 客户退款单", () => {
    test("出纳提交客户退款：驳回轮次加一后通过，应收恢复且原回款保留", async ({
        page,
        browser,
    }) => {
        test.setTimeout(12 * 60 * 1000)
        const stamp = Date.now().toString(36).toUpperCase()
        const legalName = `退款测试客户${stamp}`
        const shortName = `退款${stamp.slice(-6)}`
        const creditCode = `9111F16${stamp}000000000000`.replace(/[^0-9A-Z]/g, "0").slice(0, 18)
        const contractNo = `HT-F16-${stamp}`
        const extra: BrowserContext[] = []

        try {
            // ── 1. 销售：客户 + 合同 + 实物销售单 ──
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

            // ── 2. 采购：W01 通过销售单审批（不分配供给、不建采购单）──
            const caigou = await openRole(browser, extra, "caigou")
            await approveWorkspaceTask(caigou.page, "销售单审批", order.orderNo)
            await caigou.context.close()

            await page.goto(`/sales/orders/${order.id}`)
            await expectEffectiveSalesOrder(page, order.orderNo)
            await page.getByRole("tab", { name: /^采购/ }).click()
            await expect(page.getByTestId("sales-order-purchase-status")).toContainText(
                "采购单 0 笔",
                { timeout: TIMEOUT },
            )
            await expect(page.getByText("本单还没有采购单。")).toBeVisible({ timeout: TIMEOUT })
            await expectFulfillmentNotStarted(page)

            // ── 3. 出纳登记回款并核销本单 ──
            const fukuan = await openRole(browser, extra, "fukuan")
            const receiptNo = await registerReceiptForOrder(fukuan.page, {
                customerName: legalName,
                orderNo: order.orderNo,
                bankReference: `BANK-F16-${stamp}`,
            })
            await fukuan.context.close()

            const caiwuReceipt = await openRole(browser, extra, "caiwu")
            await approveWorkspaceTask(caiwuReceipt.page, "回款复核", receiptNo)
            await caiwuReceipt.context.close()

            await page.goto(`/sales/orders/${order.id}`)
            await expectCollection(page, "已结清")
            await expectNotClosed(page)

            // ── 4. 负向：caiwu 不得自己提交客户退款 ──
            const caiwuDenied = await openRole(browser, extra, "caiwu")
            await assertCaiwuCannotSubmitCustomerRefund(caiwuDenied.page, receiptNo)
            await caiwuDenied.context.close()

            // ── 5. 出纳从已过账回款发起客户退款（与冲正不同入口）──
            const fukuanRefund = await openRole(browser, extra, "fukuan")
            const { refundNo, refundId } = await submitCustomerRefundFromReceipt(
                fukuanRefund.page,
                receiptNo,
                CUSTOMER_REFUND_REASON,
            )
            await expect(fukuanRefund.page.getByText(CUSTOMER_NODE)).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(
                fukuanRefund.page.locator('[data-slot="quick-preview-summary"]').getByText(
                    "审批中",
                    { exact: true },
                ),
            ).toBeVisible({ timeout: TIMEOUT })
            await fukuanRefund.context.close()
            expect(refundNo.length).toBeGreaterThan(2)
            expect(refundId.length).toBeGreaterThan(2)

            // 提交后财务总监尚不是当前节点
            const caiwuEarly = await openRole(browser, extra, "caiwu")
            await caiwuEarly.page.goto("/workspace")
            await waitHeading(caiwuEarly.page, "我的工作台")
            await caiwuEarly.page.locator("#workspace-queue-toolbar-search-input").fill(refundNo)
            await caiwuEarly.page.locator("#workspace-queue-toolbar-search-input").press("Enter")
            await expect(
                caiwuEarly.page.getByRole("button", { name: /客户退款审批/ }),
            ).toHaveCount(0)
            await caiwuEarly.context.close()

            // ── 6. 销售领导驳回：轮次加一，回到首节点，退款未入账 ──
            const leaderReject = await openRole(browser, extra, "lisiyong")
            await rejectWorkspaceTask(
                leaderReject.page,
                "客户退款审批",
                refundNo,
                REJECT_REASON,
            )
            await openWorkspaceApprovalTask(leaderReject.page, "客户退款审批", refundNo)
            await expect(leaderReject.page.getByText("第 2 轮")).toBeVisible({ timeout: LONG })
            await expect(leaderReject.page.getByText(CUSTOMER_NODE)).toBeVisible({
                timeout: LONG,
            })
            await expect(leaderReject.page.getByText(REJECT_REASON)).toBeVisible({
                timeout: LONG,
            })
            await expect(leaderReject.page.getByRole("button", { name: "通过" })).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(leaderReject.page.getByRole("button", { name: "驳回" })).toBeVisible({
                timeout: TIMEOUT,
            })
            await leaderReject.context.close()

            const fukuanAfterReject = await openRole(browser, extra, "fukuan")
            await assertCustomerRefundPreview(fukuanAfterReject.page, refundId, refundNo, "审批中")
            await expect(fukuanAfterReject.page.getByText("第 2 轮")).toBeVisible({ timeout: LONG })
            await expect(fukuanAfterReject.page.getByText(CUSTOMER_NODE)).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(fukuanAfterReject.page.getByText(REJECT_REASON)).toBeVisible({
                timeout: TIMEOUT,
            })
            await fukuanAfterReject.page.locator("#customer-receivables-preview-close").click()
            await assertReceiptStillPosted(fukuanAfterReject.page, receiptNo)
            await fukuanAfterReject.context.close()

            await page.goto(`/sales/orders/${order.id}`)
            await expectCollection(page, "已结清")
            await expectNotClosed(page)

            // ── 7. 销售领导通过 → 财务总监审批入账 ──
            const leaderApprove = await openRole(browser, extra, "lisiyong")
            await approveWorkspaceTask(leaderApprove.page, "客户退款审批", refundNo)
            await leaderApprove.context.close()

            const caiwuRefund = await openRole(browser, extra, "caiwu")
            await openWorkspaceApprovalTask(caiwuRefund.page, "客户退款审批", refundNo)
            await expect(caiwuRefund.page.getByText(FINANCE_NODE)).toBeVisible({ timeout: LONG })
            await confirmApprove(caiwuRefund.page)
            await caiwuRefund.context.close()

            // ── 8. 断言：退款已过账、应收恢复、原回款仍为已过账（不是冲正）──
            const fukuanPosted = await openRole(browser, extra, "fukuan")
            await assertCustomerRefundPreview(fukuanPosted.page, refundId, refundNo, "已过账")
            await expect(fukuanPosted.page.getByText("已过账记录只读")).toBeVisible({
                timeout: TIMEOUT,
            })
            await fukuanPosted.page.locator("#customer-receivables-preview-close").click()
            await assertReceiptStillPosted(fukuanPosted.page, receiptNo)
            await fukuanPosted.page.getByRole("row", { name: new RegExp(receiptNo) }).getByRole("button", { name: "预览" }).click()
            await expect(fukuanPosted.page.getByRole("heading", { name: receiptNo })).toBeVisible({
                timeout: LONG,
            })
            await expect(
                fukuanPosted.page.locator('[data-slot="quick-preview-summary"]').getByText("已过账", {
                    exact: true,
                }),
            ).toBeVisible({ timeout: TIMEOUT })
            await expect(
                fukuanPosted.page.locator("#customer-receivables-preview-receipt-refund"),
            ).toBeVisible({ timeout: TIMEOUT })
            await expect(
                fukuanPosted.page.locator("#customer-receivables-preview-receipt-reverse"),
            ).toBeVisible({ timeout: TIMEOUT })
            await fukuanPosted.page.locator("#customer-receivables-preview-close").click()

            await fukuanPosted.page.goto("/finance/customer-accounts?view=receivable")
            await waitHeading(fukuanPosted.page, "客户往来")
            await fukuanPosted.page.locator("#customer-receivables-view-receivable").click()
            await fukuanPosted.page.locator("#customer-receivables-toolbar-search").fill(order.orderNo)
            await fukuanPosted.page.locator("#customer-receivables-toolbar-search").press("Enter")
            const receivableRow = fukuanPosted.page.getByRole("row", {
                name: new RegExp(order.orderNo),
            })
            await expect(receivableRow.getByText("未结")).toBeVisible({ timeout: LONG })
            await expect(receivableRow.getByText("已结清")).toHaveCount(0)
            await fukuanPosted.context.close()

            await page.goto(`/sales/orders/${order.id}`)
            await expectCollection(page, "未收")
            await expectNotClosed(page)
            await expectFulfillmentNotStarted(page)
            await page.getByRole("tab", { name: /^采购/ }).click()
            await expect(page.getByTestId("sales-order-purchase-status")).toContainText(
                "采购单 0 笔",
                { timeout: TIMEOUT },
            )
        } finally {
            await Promise.allSettled(extra.map((context) => context.close()))
        }
    })

    test("供应商退款：fukuan 提交 → caigou 确认依据 → caiwu 审批入账", async ({
        page,
        browser,
    }) => {
        test.setTimeout(12 * 60 * 1000)
        const stamp = Date.now().toString(36).toUpperCase()
        const legalName = `供退测试客户${stamp}`
        const shortName = `供退${stamp.slice(-6)}`
        const creditCode = `9111F16B${stamp}00000000000`.replace(/[^0-9A-Z]/g, "0").slice(0, 18)
        const contractNo = `HT-F16S-${stamp}`
        const extra: BrowserContext[] = []

        try {
            // ── 1. 销售：客户 + 合同 + 销售单 ──
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

            // ── 2. 采购：销售单通过 → 供给分配创建采购单并立即提交 ──
            const caigou = await openRole(browser, extra, "caigou")
            await approveWorkspaceTask(caigou.page, "销售单审批", order.orderNo)
            await gotoWorkspace(caigou.page)
            await caigou.page.locator("#workspace-family-nav-procurement").click()
            await openWorkspaceTask(caigou.page, /待供给分配/)
            await expect(caigou.page.getByRole("heading", { name: "供给分配" })).toBeVisible({
                timeout: LONG,
            })
            await expect(caigou.page.getByText("将创建采购单")).toBeVisible({ timeout: TIMEOUT })
            await caigou.page.locator("#procurement-orders-create-preview").click()
            const preview = caigou.page.getByRole("dialog", { name: "预览供给分配" })
            await expect(preview).toBeVisible({ timeout: TIMEOUT })
            await expect(preview.getByText("本次全部由现有库存满足")).toHaveCount(0)
            await preview.locator("#procurement-orders-create-preview-confirm").click()
            const confirmAlloc = caigou.page
                .getByRole("alertdialog")
                .filter({ hasText: "确认供给分配" })
            await expect(confirmAlloc).toBeVisible({ timeout: TIMEOUT })
            await confirmAlloc.locator("#procurement-orders-create-confirm").click()
            await expect(caigou.page.getByText(/已创建 1 张采购单并提交审批|已将缺口拆成/)).toBeVisible({
                timeout: LONG,
            })
            await caigou.context.close()

            // ── 3. 财务：采购单审批通过，形成应付 ──
            const caiwuPo = await openRole(browser, extra, "caiwu")
            await gotoWorkspace(caiwuPo.page)
            await caiwuPo.page.locator("#workspace-family-nav-approval").click()
            await openWorkspaceTask(caiwuPo.page, /采购单审批/)
            await confirmApprove(caiwuPo.page)
            await caiwuPo.page.goto("/finance/supplier-accounts")
            await expect(caiwuPo.page.getByRole("heading", { name: "供应商往来" })).toBeVisible({
                timeout: LONG,
            })
            await expect(caiwuPo.page.getByText(new RegExp(SUPPLIER_SHORT)).first()).toBeVisible({
                timeout: LONG,
            })
            await expect(caiwuPo.page.getByText("未结").first()).toBeVisible({ timeout: TIMEOUT })
            await expect(
                caiwuPo.page.locator("#supplier-payables-header-register-payment"),
            ).toBeDisabled()
            await caiwuPo.context.close()

            // ── 4. 出纳：W01 付款任务一次确认入账（NO_APPROVAL）──
            const fukuanPay = await openRole(browser, extra, "fukuan")
            await gotoWorkspace(fukuanPay.page)
            await fukuanPay.page.locator("#workspace-family-nav-finance").click()
            await openWorkspaceTask(fukuanPay.page, /供应商付款处理/)
            await expect(fukuanPay.page.getByRole("heading", { name: /向.+付款/ })).toBeVisible({
                timeout: LONG,
            })
            await expect(fukuanPay.page.getByRole("button", { name: "提交审批" })).toHaveCount(0)
            await expect(fukuanPay.page.getByText("供应商付款单审批")).toHaveCount(0)
            await fukuanPay.page.locator("#supplier-payables-allocation-form-bank-receipt-input").setInputFiles({
                name: "bank-receipt-f16.png",
                mimeType: "image/png",
                buffer: RECEIPT_PNG,
            })
            await fukuanPay.page.locator("#supplier-payables-allocation-form-submit").click()
            const payConfirm = fukuanPay.page.getByRole("alertdialog").filter({ hasText: "确认付款" })
            await expect(payConfirm).toBeVisible({ timeout: TIMEOUT })
            await expect(payConfirm.getByText("提交审批")).toHaveCount(0)
            await payConfirm.locator("#supplier-payables-payment-submit-confirm-confirm").click()
            await expect(fukuanPay.page.getByText("付款已登记")).toBeVisible({ timeout: LONG })

            await fukuanPay.page.goto("/finance/supplier-accounts?view=payment")
            await expect(fukuanPay.page.getByRole("heading", { name: "供应商往来" })).toBeVisible({
                timeout: LONG,
            })
            await fukuanPay.page.locator("#supplier-payables-view-tabs-trigger-payment").click()
            await fukuanPay.page.locator("#supplier-payables-toolbar-search").fill(SUPPLIER_SHORT)
            await fukuanPay.page.locator("#supplier-payables-toolbar-search").press("Enter")
            const paymentRefundBtn = fukuanPay.page.getByRole("button", { name: "退款" })
            await expect(paymentRefundBtn).toBeVisible({ timeout: LONG })
            const paymentRow = fukuanPay.page.getByRole("row").filter({ has: paymentRefundBtn })
            await expect(paymentRow.getByText("已过账")).toBeVisible({ timeout: TIMEOUT })
            await expect(paymentRow.getByText("已冲正")).toHaveCount(0)

            // ── 5. 负向：caiwu 不得自己提交供应商退款 ──
            const caiwuDenied = await openRole(browser, extra, "caiwu")
            await assertCaiwuCannotSubmitSupplierRefund(caiwuDenied.page)
            await caiwuDenied.context.close()

            // ── 6. 出纳提交供应商退款 ──
            await paymentRefundBtn.click()
            await expect(
                fukuanPay.page.getByRole("dialog").getByRole("heading", { name: "发起供应商退款" }),
            ).toBeVisible({ timeout: TIMEOUT })
            await fukuanPay.page.locator("#supplier-payables-refund-request-reason").fill(
                SUPPLIER_REFUND_REASON,
            )
            await fukuanPay.page.locator("#supplier-payables-refund-request-submit").click()
            await expect(
                fukuanPay.page.getByRole("heading", { name: /提交退款|确认提交退款/ }),
            ).toBeVisible({ timeout: TIMEOUT })
            await expect(fukuanPay.page.getByText("任一层驳回后将从第一节点开始下一轮。")).toBeVisible({
                timeout: TIMEOUT,
            })
            await fukuanPay.page.locator("#supplier-payables-refund-submit-confirm-confirm").click()
            await expect(fukuanPay.page.getByRole("heading", { name: "退款已提交审批" })).toBeVisible(
                { timeout: LONG },
            )
            await expect(fukuanPay.page).toHaveURL(/previewKind=refund/, { timeout: LONG })
            const supplierRefundNo = await factValue(fukuanPay.page, "退款单号")
            expect(supplierRefundNo.length).toBeGreaterThan(2)
            const supplierRefundId =
                new URL(fukuanPay.page.url()).searchParams.get("detailId") ?? ""
            expect(supplierRefundId).toBeTruthy()
            await expect(fukuanPay.page.getByText(PROCUREMENT_NODE)).toBeVisible({
                timeout: TIMEOUT,
            })
            await fukuanPay.context.close()

            // ── 7. 采购确认依据 → 财务总监审批入账 ──
            const caigouRefund = await openRole(browser, extra, "caigou")
            await approveWorkspaceTask(caigouRefund.page, "供应商退款审批", supplierRefundNo)
            await caigouRefund.context.close()

            const caiwuRefund = await openRole(browser, extra, "caiwu")
            await openWorkspaceApprovalTask(caiwuRefund.page, "供应商退款审批", supplierRefundNo)
            await expect(caiwuRefund.page.getByText(FINANCE_NODE)).toBeVisible({ timeout: LONG })
            await confirmApprove(caiwuRefund.page)
            await caiwuRefund.context.close()

            // ── 8. 断言：退款已过账、应付恢复、原付款仍为已过账 ──
            const fukuanAssert = await openRole(browser, extra, "fukuan")
            await fukuanAssert.page.goto(
                `/finance/supplier-accounts?view=payment&previewKind=refund&detailId=${encodeURIComponent(supplierRefundId)}`,
            )
            await expect(fukuanAssert.page.getByRole("heading", { name: "供应商往来" })).toBeVisible(
                { timeout: LONG },
            )
            await expect(
                fukuanAssert.page.getByRole("heading", { name: supplierRefundNo }),
            ).toBeVisible({ timeout: LONG })
            await expect(fukuanAssert.page.getByText("已过账").first()).toBeVisible({
                timeout: LONG,
            })

            await fukuanAssert.page.goto("/finance/supplier-accounts?view=payment")
            await fukuanAssert.page.locator("#supplier-payables-view-tabs-trigger-payment").click()
            await fukuanAssert.page.locator("#supplier-payables-toolbar-search").fill(SUPPLIER_SHORT)
            await fukuanAssert.page.locator("#supplier-payables-toolbar-search").press("Enter")
            const postedPayment = fukuanAssert.page
                .getByRole("row")
                .filter({ has: fukuanAssert.page.getByRole("button", { name: "退款" }) })
            await expect(postedPayment.getByText("已过账")).toBeVisible({ timeout: LONG })
            await expect(postedPayment.getByText("已冲正")).toHaveCount(0)

            await fukuanAssert.page.locator("#supplier-payables-view-tabs-trigger-payable").click()
            await fukuanAssert.page.locator("#supplier-payables-toolbar-search").fill(SUPPLIER_SHORT)
            await fukuanAssert.page.locator("#supplier-payables-toolbar-search").press("Enter")
            await expect(fukuanAssert.page.getByText("未结").first()).toBeVisible({ timeout: LONG })
            await fukuanAssert.context.close()
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

async function gotoWorkspace(page: Page) {
    await page.goto("/workspace")
    await waitHeading(page, "我的工作台")
}

async function openWorkspaceTask(page: Page, name: RegExp | string) {
    const list = page.getByRole("list", { name: "待办列表" })
    await expect(list).toBeVisible({ timeout: TIMEOUT })
    await list.getByRole("button", { name }).first().click()
}

function parseAmount(raw: string): string {
    const match = raw.replace(/,/g, "").match(/\d+(?:\.\d+)?/)
    if (!match) throw new Error(`无法解析金额: ${raw}`)
    return Number(match[0]).toFixed(2)
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
    await expect(page.getByRole("dialog").getByRole("heading", { name: "上传合同 PDF" })).toBeVisible(
        { timeout: LONG },
    )
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
    await expect(page.getByRole("dialog").getByRole("heading", { name: "上传合同 PDF" })).toBeHidden(
        { timeout: LONG },
    )
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
    await expectNotClosed(page)
}

async function expectCollection(page: Page, label: "未收" | "部分回款" | "已结清") {
    await expect(page.getByLabel("销售单金额摘要").getByText(label, { exact: true })).toBeVisible({
        timeout: LONG,
    })
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

// ─── 工作台审批 ────────────────────────────────────────────────────────────

async function openWorkspaceApprovalTask(page: Page, typeLabel: string, hint: string) {
    await gotoWorkspace(page)
    const search = page.locator("#workspace-queue-toolbar-search-input")
    await search.fill(hint)
    await search.press("Enter")
    const list = page.getByRole("list", { name: "待办列表" })
    await expect(list).toBeVisible({ timeout: TIMEOUT })
    const withHint = list.getByRole("button", {
        name: new RegExp(`${escapeRe(typeLabel)}[\\s\\S]*${escapeRe(hint)}`),
    })
    const byType = list.getByRole("button", { name: new RegExp(escapeRe(typeLabel)) })
    const task = (await withHint.count()) > 0 ? withHint : byType
    await expect(task).toBeVisible({ timeout: LONG })
    await task.click()
    return task
}

async function confirmApprove(page: Page) {
    const approve = page.getByRole("button", { name: "通过" })
    await expect(approve).toBeVisible({ timeout: LONG })
    await approve.click()
    await expect(page.getByRole("heading", { name: "确认通过" })).toBeVisible({ timeout: TIMEOUT })
    await page.getByRole("button", { name: "确认通过" }).click()
    await expect(page.getByRole("heading", { name: "确认通过" })).toBeHidden({ timeout: LONG })
}

async function approveWorkspaceTask(page: Page, typeLabel: string, hint: string) {
    const task = await openWorkspaceApprovalTask(page, typeLabel, hint)
    await confirmApprove(page)
    await expect(task).toBeHidden({ timeout: LONG })
}

async function rejectWorkspaceTask(
    page: Page,
    typeLabel: string,
    hint: string,
    reason: string,
) {
    await openWorkspaceApprovalTask(page, typeLabel, hint)
    const reject = page.getByRole("button", { name: "驳回" })
    await expect(reject).toBeVisible({ timeout: LONG })
    await reject.click()
    await expect(page.getByRole("heading", { name: "确认驳回" })).toBeVisible({ timeout: TIMEOUT })
    await page.getByLabel("驳回原因").fill(reason)
    await page.getByRole("button", { name: "确认驳回" }).click()
    await expect(page.getByRole("heading", { name: "确认驳回" })).toBeHidden({ timeout: LONG })
}

// ─── 回款 / 客户退款 ──────────────────────────────────────────────────────

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

async function registerReceiptForOrder(
    page: Page,
    input: { customerName: string; orderNo: string; bankReference: string },
) {
    await page.goto("/finance/customer-accounts")
    await waitHeading(page, "客户往来")
    const register = page.locator("#customer-receivables-header-register-receipt")
    await expect(register).toBeEnabled({ timeout: LONG })
    await register.click()
    await resolveReceiptPartyPicker(page, input.customerName)
    await expect(page.getByRole("heading", { name: /核销 · / })).toBeVisible({ timeout: LONG })
    await expect(page.getByRole("heading", { name: "同主体待核销池" })).toBeVisible({
        timeout: TIMEOUT,
    })

    const poolItem = page
        .locator("section")
        .filter({ has: page.getByRole("heading", { name: /同主体待核销池/ }) })
        .locator("li")
        .filter({ hasText: input.orderNo })
    await expect(poolItem).toBeVisible({ timeout: TIMEOUT })
    const openAmount = parseAmount(await poolItem.innerText())
    await page.locator("#customer-receivables-session-amount").fill(openAmount)
    await page.locator("#customer-receivables-session-bank-reference").fill(input.bankReference)
    if (!(await poolItem.getByText("已加入").isVisible().catch(() => false))) {
        await poolItem.getByRole("button", { name: "加入" }).click()
        await expect(poolItem.getByText("已加入")).toBeVisible({ timeout: TIMEOUT })
    }
    const fill = page.getByRole("button", { name: "填满" })
    await expect(fill).toBeVisible({ timeout: TIMEOUT })
    await fill.click()

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

async function assertCustomerRefundPreview(
    page: Page,
    refundId: string,
    refundNo: string,
    status: "审批中" | "已过账",
) {
    await page.goto(
        `/finance/customer-accounts?view=receipt&previewKind=refund&previewId=${encodeURIComponent(refundId)}`,
    )
    await waitHeading(page, "客户往来")
    await expect(page.getByRole("heading", { name: refundNo })).toBeVisible({ timeout: LONG })
    await expect(
        page.locator('[data-slot="quick-preview-summary"]').getByText(status, { exact: true }),
    ).toBeVisible({ timeout: LONG })
}

async function assertReceiptStillPosted(page: Page, receiptNo: string) {
    await page.goto("/finance/customer-accounts?view=receipt")
    await waitHeading(page, "客户往来")
    await page.locator("#customer-receivables-view-receipt").click()
    await page.locator("#customer-receivables-toolbar-search").fill(receiptNo)
    await page.locator("#customer-receivables-toolbar-search").press("Enter")
    const row = page.getByRole("row", { name: new RegExp(receiptNo) })
    await expect(row).toBeVisible({ timeout: LONG })
    await expect(row.getByText("已过账")).toBeVisible({ timeout: TIMEOUT })
    await expect(row.getByText("已冲正")).toHaveCount(0)
}

async function openPostedReceiptPreview(page: Page, receiptNo: string) {
    await page.goto("/finance/customer-accounts?view=receipt")
    await waitHeading(page, "客户往来")
    await page.locator("#customer-receivables-view-receipt").click()
    await page.locator("#customer-receivables-toolbar-search").fill(receiptNo)
    await page.locator("#customer-receivables-toolbar-search").press("Enter")
    const row = page.getByRole("row", { name: new RegExp(receiptNo) })
    await expect(row).toBeVisible({ timeout: LONG })
    await expect(row.getByText("已过账")).toBeVisible({ timeout: TIMEOUT })
    await row.getByRole("button", { name: "预览" }).click()
    await expect(page.getByRole("heading", { name: receiptNo })).toBeVisible({ timeout: TIMEOUT })
}

async function submitCustomerRefundFromReceipt(page: Page, receiptNo: string, reason: string) {
    await openPostedReceiptPreview(page, receiptNo)
    await expect(page.locator("#customer-receivables-preview-receipt-refund")).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(page.locator("#customer-receivables-preview-receipt-reverse")).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.locator("#customer-receivables-preview-receipt-refund").click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "发起客户退款" })).toBeVisible(
        { timeout: TIMEOUT },
    )
    await expect(page.getByText("冲正表示撤销本次回款记录。")).toHaveCount(0)
    await expect(page.getByText("退款表示向客户退回资金。")).toBeVisible({ timeout: TIMEOUT })
    await page.locator("#customer-receivables-refund-reason").fill(reason)
    await page.locator("#customer-receivables-refund-request-submit").click()
    await expect(page.getByRole("heading", { name: /提交退款|确认提交退款/ })).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(page.getByText("任一层驳回后将从第一节点开始下一轮。")).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(page.getByText(/销售领导/)).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText(/财务总监/)).toBeVisible({ timeout: TIMEOUT })
    await page.locator("#customer-receivables-refund-submit-confirm-dialog-confirm").click()
    await expect(page.getByRole("heading", { name: "退款已提交审批" })).toBeVisible({
        timeout: LONG,
    })
    await expect(page).toHaveURL(/previewKind=refund/, { timeout: LONG })
    const refundNo = await factValue(page, "退款单号")
    const refundId = new URL(page.url()).searchParams.get("previewId") ?? ""
    expect(refundId).toBeTruthy()
    return { refundNo, refundId }
}

async function assertCaiwuCannotSubmitCustomerRefund(page: Page, receiptNo: string) {
    await openPostedReceiptPreview(page, receiptNo)
    const refund = page.locator("#customer-receivables-preview-receipt-refund")
    await expect(refund).toBeVisible({ timeout: LONG })
    if (await refund.isDisabled()) {
        await expect(refund).toBeDisabled()
        return
    }
    await refund.click()
    const request = page.getByRole("dialog").getByRole("heading", { name: "发起客户退款" })
    await expect(request).toBeVisible({ timeout: TIMEOUT })
    await page.locator("#customer-receivables-refund-reason").fill("财务总监不得提交自己的退款")
    await page.locator("#customer-receivables-refund-request-submit").click()
    const confirm = page.locator("#customer-receivables-refund-submit-confirm-dialog-confirm")
    if (await confirm.isVisible({ timeout: TIMEOUT }).catch(() => false)) {
        await confirm.click()
    }
    await expect(
        page.getByText(/提交人不得审批自己的单据|当前账号没有执行此操作的权限|操作未成功/),
    ).toBeVisible({ timeout: LONG })
}

async function assertCaiwuCannotSubmitSupplierRefund(page: Page) {
    await page.goto("/finance/supplier-accounts?view=payment")
    await expect(page.getByRole("heading", { name: "供应商往来" })).toBeVisible({ timeout: LONG })
    await page.locator("#supplier-payables-view-tabs-trigger-payment").click()
    await page.locator("#supplier-payables-toolbar-search").fill(SUPPLIER_SHORT)
    await page.locator("#supplier-payables-toolbar-search").press("Enter")
    const refund = page.getByRole("button", { name: "退款" })
    await expect(refund).toBeVisible({ timeout: LONG })
    if (await refund.isDisabled()) {
        await expect(refund).toBeDisabled()
        return
    }
    await refund.click()
    await expect(page.getByRole("dialog").getByRole("heading", { name: "发起供应商退款" })).toBeVisible(
        { timeout: TIMEOUT },
    )
    await page.locator("#supplier-payables-refund-request-reason").fill(
        "财务总监不得提交自己的供应商退款",
    )
    await page.locator("#supplier-payables-refund-request-submit").click()
    const confirm = page.locator("#supplier-payables-refund-submit-confirm-confirm")
    if (await confirm.isVisible({ timeout: TIMEOUT }).catch(() => false)) {
        await confirm.click()
    }
    await expect(
        page.getByText(/提交人不得审批自己的单据|当前账号没有执行此操作的权限|退款失败|操作未成功/),
    ).toBeVisible({ timeout: LONG })
}
