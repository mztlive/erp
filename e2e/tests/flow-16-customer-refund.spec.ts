/**
 * 流程: [flow-16] 客户/供应商退款驳回后修改原单重提
 * 文档: docs/erp-phase-1.md §6.3 / §6.5.4；approval-workflow-contract.md §4.3/§4.4；
 *       workbench-workitem-contract.md 第 3 节
 * 账号: xiaoshou 建客户/合同/销售单；caigou 审批销售单、供给分配、确认供应商退款依据；
 *       fukuan 提交客户回款/客户退款/供应商退款（禁止 caiwu 自己提交）；
 *       lisiyong 确认客户退款依据并在驳回重提后再次处理首节点；
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
 * 5. 驳回结束本次审批；出纳点击「修改原单」，以同 ID、同单号及原资金来源编辑，
 *    填写重提说明后提交，新实例重新从首节点审批。
 */
import { workPage } from "../helpers/workspace-page"
import { archiveContractViaUi } from "../helpers/contracts"
import { fillSalesLineUnitPrice, selectExistingContractForSalesOrder } from "../helpers/sales-orders"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

import {
    test,
    expect,
    type Browser,
    type BrowserContext,
    type Page,
} from "../helpers/test"

import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { createCustomerViaUi } from "../helpers/customers"
import { approveSourcedPurchase } from "../helpers/financial-analytics"
import { readFinancialApprovalVersion, rejectFinancialOriginal } from "../helpers/financial-draft-edit"
import { headedAwareViewport } from "../helpers/headed"
import { loginViaUi, openLoggedInWorkspace } from "../helpers/login"
import { payOnlySupplierTask } from "../helpers/payments"
import {
    expectReceiptPreview,
    registerCustomerReceiptForOrder,
    submitCustomerRefundRequest,
} from "../helpers/receipts"
import { confirmSupplyAllocation, expandSourcingEditor } from "../helpers/sourcing"
import {
    approveCurrentDocument,
    chooseOption,
    expectToast,
    matchingWorkspaceTaskRows,
    openWorkspaceTask,
    selectWorkspaceFamily,
    pickCalendarDay,
    readHeaderDocumentNumber,
    salesOrderAmountSummary,
    expectSourcingCounts,
} from "../helpers/ui"

// 两个退款用例各自创建业务数据；单 worker 顺序执行，前一个失败也必须执行后一个。
test.describe.configure({ mode: "default" })
test.use(headedAwareViewport({ width: 1440, height: 960 }))

const TIMEOUT = 20_000
const LONG = 40_000
const SKU_NAME = "狮峰明前龙井礼盒"
const SUPPLIER_SHORT = "狮峰茶叶"
const CUSTOMER_NODE = "销售领导确认退款依据"
const FINANCE_NODE = "财务总监审批"
const PROCUREMENT_NODE = "采购确认退款依据"
const REJECT_REASON = "退款依据不足，请补充原回款与客户约定后再报"
const CUSTOMER_REFUND_REASON =
    "客户取消订单，按已入账回款全额退回资金，原回款保留"
const SUPPLIER_REFUND_REASON =
    "供应商退回多收款，按已入账付款全额退款，原付款保留"

const REPO_ROOT = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "..",
)
const CONTRACT_PDF = path.resolve(REPO_ROOT, "fixtures", "sample-contract.pdf")

test.describe("flow-16 客户退款单", () => {
    test("出纳提交客户退款：驳回后修改原单重提，应收恢复且原回款保留", async ({
        page,
        browser,
    }) => {
        test.setTimeout(12 * 60 * 1000)
        const stamp = Date.now().toString(36).toUpperCase()
        const legalName = `退款测试客户${stamp}`
        const shortName = `退款${stamp.slice(-6)}`
        const creditCode = `9111F16${stamp}000000000000`
            .replace(/[^0-9A-Z]/g, "0")
            .slice(0, 18)
        const contractNo = `HT-F16-${stamp}`
        const extra: BrowserContext[] = []

        try {
            // ── 1. 销售：客户 + 合同 + 实物销售单 ──
            await loginViaUi(page, "xiaoshou")
            await expect(
                workPage(page).getByRole("heading", { name: "待我处理" }),
            ).toBeVisible({
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
            await expect(
                workPage(page).locator("#sales-order-follow-up-delivery").locator(".."),
            ).toContainText("待采购", {
                timeout: TIMEOUT,
            })
            expect(await apiGet<{ purchase_order_count: number }>(
                await apiToken("xiaoshou"), `/admin/sales-orders/${order.id}`,
            )).toMatchObject({ purchase_order_count: 0 })
            await expectFulfillmentNotStarted(page)

            // ── 3. 出纳登记回款并核销本单 ──
            const fukuan = await openRole(browser, extra, "fukuan")
            const { receiptNo } = await registerCustomerReceiptForOrder(
                fukuan.page,
                {
                    customerName: legalName,
                    orderNo: order.orderNo,
                    bankReference: `BANK-F16-${stamp}`,
                },
            )
            await fukuan.context.close()

            const caiwuReceipt = await openRole(browser, extra, "caiwu")
            await approveWorkspaceTask(caiwuReceipt.page, "回款复核", receiptNo)
            await caiwuReceipt.context.close()

            await page.goto(`/sales/orders/${order.id}`)
            await expectCollection(page, "已结清")
            await expectNotClosed(page)

            // ── 4. 负向：caiwu 不得自己提交客户退款 ──
            const caiwuDenied = await openRole(browser, extra, "caiwu")
            await assertCaiwuCannotSubmitCustomerRefund(
                caiwuDenied.page,
                receiptNo,
            )
            await caiwuDenied.context.close()

            // ── 5. 出纳从已过账回款发起客户退款（与冲正不同入口）──
            const fukuanRefund = await openRole(browser, extra, "fukuan")
            const { refundNo, refundId } =
                await submitCustomerRefundFromReceipt(
                    fukuanRefund.page,
                    receiptNo,
                    CUSTOMER_REFUND_REASON,
                )
            await expect(
                workPage(fukuanRefund.page).getByText(CUSTOMER_NODE).first(),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(
                workPage(fukuanRefund.page)
                    .locator('[data-slot="quick-preview-summary"]')
                    .getByText("审批中", { exact: true }),
            ).toBeVisible({ timeout: TIMEOUT })
            await fukuanRefund.context.close()
            expect(refundNo.length).toBeGreaterThan(2)
            expect(refundId.length).toBeGreaterThan(2)

            // 提交后财务总监尚未收到本单任务。
            const caiwuEarly = await openRole(browser, extra, "caiwu")
            await caiwuEarly.page.goto("/workspace/tasks")
            await selectWorkspaceFamily(caiwuEarly.page, "approval")
            await expect(matchingWorkspaceTaskRows(caiwuEarly.page, "客户退款审批", refundNo)).toHaveCount(0)
            await caiwuEarly.context.close()

            // ── 6. 销售领导驳回：任务关闭，退款未入账 ──
            const leaderReject = await openRole(browser, extra, "lisiyong")
            await rejectWorkspaceTask(
                leaderReject.page,
                "客户退款审批",
                refundNo,
                REJECT_REASON,
            )
            await leaderReject.page.goto("/workspace/tasks")
            await selectWorkspaceFamily(leaderReject.page, "approval")
            await expect(matchingWorkspaceTaskRows(leaderReject.page, "客户退款审批", refundNo)).toHaveCount(0)
            await leaderReject.context.close()

            const fukuanAfterReject = await openRole(browser, extra, "fukuan")
            await assertCustomerRefundPreview(
                fukuanAfterReject.page,
                refundId,
                refundNo,
                "已驳回",
            )
            await expect(workPage(fukuanAfterReject.page).getByText(REJECT_REASON, { exact: true }).first())
                .toBeVisible({ timeout: LONG })
            await workPage(fukuanAfterReject.page)
                .locator("#customer-receivables-preview-close")
                .click()
            await assertReceiptStillPosted(fukuanAfterReject.page, receiptNo)
            await fukuanAfterReject.context.close()

            await page.goto(`/sales/orders/${order.id}`)
            await expectCollection(page, "已结清")
            await expectNotClosed(page)

            // ── 7. 按驳回意见修改原退款，保留来源、单号与金额后重新提交。 ──
            const fukuanRevise = await openRole(browser, extra, "fukuan")
            await assertCustomerRefundPreview(
                fukuanRevise.page,
                refundId,
                refundNo,
                "已驳回",
            )
            await reviseRefundOriginal(fukuanRevise.page, {
                kind: "customer_refund",
                id: refundId,
                refundNo,
                reason: "已补充客户取消约定及原回款凭证，按原金额退还",
            })
            await fukuanRevise.context.close()

            // ── 8. 销售领导通过 → 财务总监审批入账 ──
            const leaderApprove = await openRole(browser, extra, "lisiyong")
            await approveWorkspaceTask(
                leaderApprove.page,
                "客户退款审批",
                refundNo,
            )
            await leaderApprove.context.close()

            const caiwuRefund = await openRole(browser, extra, "caiwu")
            await openWorkspaceApprovalTask(
                caiwuRefund.page,
                "客户退款审批",
                refundNo,
            )
            await expect(
                workPage(caiwuRefund.page).getByText(FINANCE_NODE).first(),
            ).toBeVisible({ timeout: LONG })
            await confirmApprove(caiwuRefund.page)
            await caiwuRefund.context.close()

            // ── 8. 断言：退款已过账、应收恢复、原回款仍为已过账（不是冲正）──
            const fukuanPosted = await openRole(browser, extra, "fukuan")
            await assertCustomerRefundPreview(
                fukuanPosted.page,
                refundId,
                refundNo,
                "已过账",
            )
            await expect(workPage(fukuanPosted.page).getByRole("button", { name: "修改原单", exact: true })).toHaveCount(0)
            await workPage(fukuanPosted.page)
                .locator("#customer-receivables-preview-close")
                .click()
            await assertReceiptStillPosted(fukuanPosted.page, receiptNo)
            await workPage(fukuanPosted.page)
                .getByRole("row")
                .filter({ hasText: receiptNo })
                .click()
            await expectReceiptPreview(fukuanPosted.page, receiptNo)
            await expect(
                workPage(fukuanPosted.page)
                    .locator('[data-slot="quick-preview-summary"]')
                    .getByText("已过账", {
                        exact: true,
                    }),
            ).toBeVisible({ timeout: TIMEOUT })
            await expect(workPage(fukuanPosted.page).locator("#customer-receivables-preview-receipt-continue-allocate")).toHaveCount(0)
            await workPage(fukuanPosted.page)
                .locator("#customer-receivables-preview-close")
                .click()

            await fukuanPosted.page.goto(
                "/finance/customer-accounts?view=receivable",
            )
            await waitHeading(fukuanPosted.page, "客户往来")
            await workPage(fukuanPosted.page)
                .locator("#customer-receivables-view-receivable")
                .click()
            await workPage(fukuanPosted.page)
                .locator("#customer-receivables-toolbar-search")
                .fill(order.orderNo)
            await workPage(fukuanPosted.page)
                .locator("#customer-receivables-toolbar-search")
                .press("Enter")
            const receivableRow = workPage(fukuanPosted.page)
                .getByRole("row")
                .filter({ hasText: order.orderNo })
            await expect(receivableRow.getByText("未结")).toBeVisible({
                timeout: LONG,
            })
            await expect(receivableRow.getByText("已结清")).toHaveCount(0)
            await fukuanPosted.context.close()

            await page.goto(`/sales/orders/${order.id}`)
            await expectCollection(page, "未收")
            await expectNotClosed(page)
            await expectFulfillmentNotStarted(page)
            await expect(
                workPage(page).locator("#sales-order-follow-up-delivery").locator(".."),
            ).toContainText("待采购", {
                timeout: TIMEOUT,
            })
        } finally {
            await Promise.allSettled(extra.map((context) => context.close()))
        }
    })

    test("供应商退款：采购驳回后出纳修改原单，重新确认并审批入账", async ({
        page,
        browser,
    }) => {
        test.setTimeout(12 * 60 * 1000)
        const stamp = Date.now().toString(36).toUpperCase()
        const legalName = `供退测试客户${stamp}`
        const shortName = `供退${stamp.slice(-6)}`
        const creditCode = `9111F16B${stamp}00000000000`
            .replace(/[^0-9A-Z]/g, "0")
            .slice(0, 18)
        const contractNo = `HT-F16S-${stamp}`
        const extra: BrowserContext[] = []
        let purchaseNo = ""

        try {
            // ── 1. 销售：客户 + 合同 + 销售单 ──
            await loginViaUi(page, "xiaoshou")
            await expect(
                workPage(page).getByRole("heading", { name: "待我处理" }),
            ).toBeVisible({
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
            await openWorkspaceTask(
                caigou.page,
                "待安排采购",
                order.orderNo,
                "procurement",
            )
            await expect(
                workPage(caigou.page).getByRole("heading", { name: "供给分配" }),
            ).toBeVisible({
                timeout: LONG,
            })
            await expectSourcingCounts(caigou.page)
            await expandSourcingEditor(caigou.page)
            const sourcing = workPage(caigou.page).locator('[id$="-sourcing-option"]')
            await expect(sourcing.first()).toBeVisible({ timeout: TIMEOUT })
            let purchase = sourcing.first()
            const sourcingCount = await sourcing.count()
            let sawWarehousePurchase = false
            for (let index = 0; index < sourcingCount; index += 1) {
                const combo = sourcing.nth(index)
                if (!(await combo.isVisible().catch(() => false))) continue
                const shown = (
                    (await combo.inputValue().catch(() => "")) || ""
                ).replace(/\s+/g, " ")
                if (/入仓/.test(shown) && !/可用/.test(shown)) {
                    purchase = combo
                    sawWarehousePurchase = true
                    break
                }
            }
            if (!sawWarehousePurchase) {
                await chooseOption(caigou.page, purchase, /入仓/, "入仓")
            }
            const purchaseId = await purchase.getAttribute("id")
            const purchasePrefix = purchaseId?.replace(/-sourcing-option$/, "")
            expect(
                purchasePrefix,
                `未能读取入仓履约方案身份: ${purchaseId}`,
            ).toBeTruthy()
            const warehouse = workPage(caigou.page).locator(
                `#${purchasePrefix}-warehouse`,
            )
            await expect(warehouse).toBeVisible({ timeout: TIMEOUT })
            await warehouse.click()
            await warehouse.fill("BJ-TZ-01")
            const option = workPage(caigou.page)
                .getByRole("option", { name: /BJ-TZ-01|北京通州/ })
                .first()
            await expect(option).toBeVisible({ timeout: TIMEOUT })
            await option.click()
            await workPage(caigou.page).locator("#sourcing-product-adjust-done").click()
            await workPage(caigou.page)
                .locator("#procurement-orders-create-preview")
                .click()
            const preview = workPage(caigou.page).getByRole("dialog", {
                name: "预览供给分配",
            })
            await expect(preview).toBeVisible({ timeout: TIMEOUT })
            await expect(
                preview.getByText("本次全部由现有库存满足"),
            ).toHaveCount(0)
            const allocation = await confirmSupplyAllocation(caigou.page)
            expect(allocation.orders).toHaveLength(1)
            purchaseNo = allocation.orders[0]!.purchase_no
            await caigou.context.close()

            // ── 3. 采购部门：采购单审批通过，形成应付 ──
            const purchaseApprover = await openRole(browser, extra, "caigoushenpi")
            await approveSourcedPurchase(purchaseApprover.page, allocation.orders[0]!.purchase_order_id)
            await purchaseApprover.context.close()
            // 采购审批与财务只读核对应付由各自岗位执行，财审不得登记付款。
            const financeReader = await openRole(browser, extra, "caiwu")
            await financeReader.page.goto("/finance/supplier-accounts")
            await expect(
                workPage(financeReader.page).getByRole("heading", { name: "供应商往来" }),
            ).toBeVisible({
                timeout: LONG,
            })
            await expect(
                workPage(financeReader.page).getByText(new RegExp(SUPPLIER_SHORT)).first(),
            ).toBeVisible({
                timeout: LONG,
            })
            await expect(workPage(financeReader.page).getByText("未结").first()).toBeVisible({
                timeout: TIMEOUT,
            })
            const supplierWorkspace = workPage(financeReader.page).locator('[data-slot="page-scaffold"]').filter({
                has: workPage(financeReader.page).getByRole("heading", { name: "供应商往来", exact: true }),
            })
            await expect(supplierWorkspace).toHaveCount(1)
            const paymentRequestLink = supplierWorkspace.getByRole("group", { name: "页面操作", exact: true })
                .locator('a[href="/procurement/payment-requests"]')
            // 财审从工作台处理申请；供应商往来页首不再提供申请或付款入口。
            await expect(paymentRequestLink).toHaveCount(0)
            const financeProfile = await apiGet<{ account: string; permissions: string[] }>(
                await apiToken("caiwu"), "/account/profile",
            )
            expect(financeProfile.account).toBe("caiwu")
            expect(financeProfile.permissions.filter((permission) =>
                ["*", "*:*", "*:pay", "purchase_payment_request:*", "purchase_payment_request:pay"].includes(permission),
            )).toEqual([])
            await expect(workPage(financeReader.page).getByRole("button", { name: "登记付款", exact: true })).toHaveCount(0)
            await expect(workPage(financeReader.page).locator("#ppr-pay-submit")).toHaveCount(0)
            await financeReader.context.close()

            // ── 4. 出纳：W01 付款任务一次确认入账（NO_APPROVAL）──
            const fukuanPay = await openRole(browser, extra, "fukuan")
            const payment = await payOnlySupplierTask(fukuanPay.page, purchaseNo)
            expect(payment.status).toBe("posted")

            await fukuanPay.page.goto("/finance/supplier-accounts?view=payment")
            await expect(
                workPage(fukuanPay.page).getByRole("heading", { name: "供应商往来" }),
            ).toBeVisible({
                timeout: LONG,
            })
            await workPage(fukuanPay.page)
                .locator("#supplier-payables-view-tabs-trigger-payment")
                .click()
            await workPage(fukuanPay.page)
                .locator("#supplier-payables-toolbar-search")
                .fill(SUPPLIER_SHORT)
            await workPage(fukuanPay.page)
                .locator("#supplier-payables-toolbar-search")
                .press("Enter")
            const paymentRow = workPage(fukuanPay.page)
                .getByRole("row")
                .filter({ hasText: payment.payment_no })
            await expect(
                paymentRow.getByText("已过账", { exact: true }),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(paymentRow.getByText("已冲正")).toHaveCount(0)
            await paymentRow.click()
            const paymentRefundBtn = workPage(fukuanPay.page).locator(
                "#supplier-payables-preview-record-refund",
            )
            await expect(paymentRefundBtn).toBeVisible({ timeout: LONG })

            const cashierToken = await apiToken("fukuan")
            const scopedPayment = () => apiGet<{ permission_limited: boolean; effective_amount?: string }>(
                cashierToken, `/admin/supplier-payments/${payment.id}`,
            )
            expect(await scopedPayment()).toMatchObject({
                permission_limited: false, effective_amount: payment.effective_amount,
            })

            // ── 5. 负向：caiwu 不得自己提交供应商退款 ──
            const caiwuDenied = await openRole(browser, extra, "caiwu")
            await assertCaiwuCannotSubmitSupplierRefund(caiwuDenied.page, payment)
            await caiwuDenied.context.close()

            // ── 6. 出纳提交供应商退款 ──
            await paymentRefundBtn.click()
            const refundRequest = workPage(fukuanPay.page).getByRole("dialog", {
                name: "供应商退款",
                exact: true,
            })
            await expect(refundRequest).toContainText(payment.payment_no)
            const refundAmount = refundRequest.locator("#supplier-payables-refund-request-amount")
            await expect(refundAmount).toHaveValue(payment.effective_amount)
            await refundRequest.locator("#supplier-payables-refund-request-reason").fill(SUPPLIER_REFUND_REASON)
            const supplierRefundCommitted = fukuanPay.page.waitForResponse(
                (response) =>
                    response.request().method() === "POST" &&
                    new URL(response.url()).pathname === "/admin/supplier-refunds/commit",
                { timeout: 60_000 },
            )
            await refundRequest.locator("#supplier-payables-refund-request-submit").click()
            const supplierRefundResponse = await supplierRefundCommitted
            expect(supplierRefundResponse.ok(), await supplierRefundResponse.text()).toBeTruthy()
            const { id: supplierRefundId, refund_no: supplierRefundNo, status, amount } =
                (await supplierRefundResponse.json()).data as RefundDetail
            expect(status).toBe("IN_APPROVAL")
            expect(amount).toBe(payment.effective_amount)
            expect(supplierRefundNo).toBeTruthy()
            expect(supplierRefundId).toBeTruthy()
            await expect(fukuanPay.page).toHaveURL((url) =>
                url.searchParams.get("previewKind") === "refund" &&
                url.searchParams.get("detailId") === supplierRefundId,
            )
            await expect(
                workPage(fukuanPay.page).getByText(PROCUREMENT_NODE).first(),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await fukuanPay.context.close()

            // ── 7. 采购驳回后，出纳修改同一退款单，再由首节点重新确认。 ──
            const caigouReject = await openRole(browser, extra, "caigou")
            await rejectWorkspaceTask(
                caigouReject.page,
                "供应商退款审批",
                supplierRefundNo,
                "退款凭证需补齐供应商对账依据",
            )
            await caigouReject.context.close()
            const fukuanRevise = await openRole(browser, extra, "fukuan")
            await fukuanRevise.page.goto(
                `/finance/supplier-accounts?view=payment&previewKind=refund&detailId=${encodeURIComponent(supplierRefundId)}`,
            )
            await expect(
                workPage(fukuanRevise.page).getByText(supplierRefundNo).first(),
            ).toBeVisible({
                timeout: LONG,
            })
            await reviseRefundOriginal(fukuanRevise.page, {
                kind: "supplier_refund",
                id: supplierRefundId,
                refundNo: supplierRefundNo,
                reason: "已补充供应商对账确认，按原付款金额退还多收款",
            })
            await fukuanRevise.context.close()

            // ── 8. 采购确认依据 → 财务总监审批入账 ──
            const caigouRefund = await openRole(browser, extra, "caigou")
            await approveWorkspaceTask(
                caigouRefund.page,
                "供应商退款审批",
                supplierRefundNo,
            )
            await caigouRefund.context.close()

            const caiwuRefund = await openRole(browser, extra, "caiwu")
            await openWorkspaceApprovalTask(
                caiwuRefund.page,
                "供应商退款审批",
                supplierRefundNo,
            )
            await expect(
                workPage(caiwuRefund.page).getByText(FINANCE_NODE).first(),
            ).toBeVisible({ timeout: LONG })
            await confirmApprove(caiwuRefund.page)
            await caiwuRefund.context.close()

            // ── 8. 断言：退款已过账、应付恢复、原付款仍为已过账 ──
            const fukuanAssert = await openRole(browser, extra, "fukuan")
            await fukuanAssert.page.goto(
                `/finance/supplier-accounts?view=payment&previewKind=refund&detailId=${encodeURIComponent(supplierRefundId)}`,
            )
            await expect(
                workPage(fukuanAssert.page)
                    .getByText(
                        new RegExp(
                            `退款单：${supplierRefundNo}|${supplierRefundNo}`,
                        ),
                    )
                    .first(),
            ).toBeVisible({ timeout: LONG })
            await expect(
                workPage(fukuanAssert.page).getByText("已过账", { exact: true }).first(),
            ).toBeVisible({
                timeout: LONG,
            })

            await fukuanAssert.page.goto(
                "/finance/supplier-accounts?view=payment",
            )
            await workPage(fukuanAssert.page)
                .locator("#supplier-payables-view-tabs-trigger-payment")
                .click()
            await workPage(fukuanAssert.page)
                .locator("#supplier-payables-toolbar-search")
                .fill(SUPPLIER_SHORT)
            await workPage(fukuanAssert.page)
                .locator("#supplier-payables-toolbar-search")
                .press("Enter")
            const postedPayment = workPage(fukuanAssert.page)
                .getByRole("row")
                .filter({ hasText: payment.payment_no })
            await expect(
                postedPayment.getByText("已过账", { exact: true }),
            ).toBeVisible({
                timeout: LONG,
            })
            await expect(postedPayment.getByText("已冲正")).toHaveCount(0)
            expect(await scopedPayment()).toMatchObject({
                permission_limited: false, effective_amount: "0.00",
            })
            await postedPayment.click()
            await expect(workPage(fukuanAssert.page).locator("#supplier-payables-preview-record-detail")).toBeVisible()
            await expect(workPage(fukuanAssert.page).locator("#supplier-payables-preview-record-refund")).toHaveCount(0)
            await fukuanAssert.page.goto(
                `/finance/supplier-accounts?view=payable&q=${encodeURIComponent(SUPPLIER_SHORT)}`,
            )
            await expect(
                workPage(fukuanAssert.page).getByRole("heading", { name: "供应商往来" }),
            ).toBeVisible({
                timeout: LONG,
            })
            await expect(
                workPage(fukuanAssert.page)
                    .getByRole("row")
                    .filter({ hasText: purchaseNo })
                    .getByText("未结", { exact: true }),
            ).toBeVisible({ timeout: LONG })
            await fukuanAssert.context.close()
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

async function waitHeading(page: Page, name: string | RegExp) {
    await expect(workPage(page).getByRole("heading", { name })).toBeVisible({
        timeout: LONG,
    })
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
    await workPage(page).locator("#customers-directory-search").fill(input.legalName)
    await workPage(page).locator("#customers-directory-search").press("Enter")
    const open = workPage(page).getByRole("link", { name: input.shortName })
    await expect(open).toBeVisible({ timeout: LONG })
    await open.click()
    await expect(
        workPage(page).getByRole("heading", { name: input.legalName }),
    ).toBeVisible({
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
    await expect(
        workPage(page).getByRole("button", {
            name: `打开合同 ${input.contractNo}`,
            exact: true,
        }),
    ).toBeVisible({ timeout: LONG })
}

// 负责销售是展示名，不是输入框：必须等于侧栏账号菜单里的当前用户，且不能停在占位文案。
async function expectLoggedInSalesOwner(page: Page, timeout: number) {
    const placeholders = ["加载当前用户…", "无法获取登录用户", "当前用户未就绪"]
    const accountName = page.locator(
        "#workspace-topbar-account-trigger span.truncate.font-medium",
    )
    const ownerName = workPage(page)
        .locator("#sales-orders-create-header-owner-name > span")
        .nth(1)
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
    await page.goto(
        `/sales/orders?mode=create&customerId=${encodeURIComponent(input.customerId)}`,
    )
    await expect(
        workPage(page).getByRole("heading", { name: /新建销售单|业务信息/ }),
    ).toBeVisible({
        timeout: LONG,
    })

    await selectExistingContractForSalesOrder(page, input.contractNo)
    await expect(workPage(page).locator("#sales-orders-create-customer")).toContainText(new RegExp(input.legalName))
    await expectLoggedInSalesOwner(page, LONG)

    await chooseOption(
        page,
        workPage(page).locator("#sales-orders-create-header-welfare-scene"),
        "礼包",
        "礼包",
    )
    await expect(workPage(page).locator("#sales-orders-create-header-payment-terms")).toContainText("货到 30 天")
    await expect(workPage(page).locator("#sales-orders-create-header-payment-terms").locator("input, button, [role=combobox]")).toHaveCount(0)

    await workPage(page).locator("#sales-orders-create-line-items-add").click()
    await expect(
        workPage(page).getByRole("dialog").getByRole("heading", { name: "添加商品" }),
    ).toBeVisible({
        timeout: TIMEOUT,
    })
    const skuSearch = workPage(page).locator(
        "#sales-orders-sku-picker-filter-search",
    )
    await skuSearch.fill(SKU_NAME)
    await skuSearch.press("Enter")
    const skuCheckbox = workPage(page).getByRole("checkbox", {
        name: new RegExp(`选择 ${SKU_NAME}`),
    })
    await expect(skuCheckbox.first()).toBeVisible({ timeout: LONG })
    await skuCheckbox.first().check()
    await workPage(page).locator("#sales-orders-sku-picker-confirm").click()
    await expect(
        workPage(page).getByRole("dialog").getByRole("heading", { name: "添加商品" }),
    ).toBeHidden({
        timeout: TIMEOUT,
    })
    await expect(
        workPage(page).getByRole("button", {
            name: new RegExp(`更换销售项目 ${SKU_NAME}`),
        }),
    ).toBeVisible({ timeout: TIMEOUT })
    await expect(
        workPage(page).getByTestId(/sales-line-procurement-owner-/),
    ).not.toContainText("暂未确定采购负责人", { timeout: LONG })
    await fillSalesLineUnitPrice(page, "1288.00")

    await workPage(page).locator("#sales-orders-create-batch-due-date-open").click()
    await pickCalendarDay(
        page,
        workPage(page).locator("#sales-orders-create-batch-due-date"),
        todayIso(),
    )
    await workPage(page).locator("#sales-orders-create-batch-due-date-apply").click()
    await expectToast(page, "已批量设置交期")

    await workPage(page).locator("#sales-orders-create-submit").click()
    await expect(
        workPage(page).getByRole("dialog").getByRole("heading", { name: "提交销售单" }),
    ).toBeVisible({
        timeout: TIMEOUT,
    })
    await workPage(page)
        .locator("#sales-orders-submit-confirm-confirm")
        .click({ force: true })
    await expect(page).toHaveURL(/\/sales\/orders\/[^/?#]+/, { timeout: LONG })
    await expect(workPage(page).getByText("审批中", { exact: true }).first()).toBeVisible(
        { timeout: LONG },
    )

    const id = page.url().split("/sales/orders/")[1]?.split(/[?#]/)[0] ?? ""
    expect(id).toBeTruthy()
    const orderNo = await readHeaderDocumentNumber(page)
    expect(orderNo.length).toBeGreaterThan(4)
    return { id, orderNo }
}

async function expectEffectiveSalesOrder(page: Page, orderNo: string) {
    await expect(workPage(page).locator("header").getByText(orderNo)).toBeVisible({
        timeout: LONG,
    })
    await expect(workPage(page).getByText("已生效", { exact: true }).first()).toBeVisible(
        { timeout: LONG },
    )
    await expectCollection(page, "未收")
    await expectNotClosed(page)
}

async function expectCollection(
    page: Page,
    label: "未收" | "部分回款" | "已结清",
) {
    await expect(
        salesOrderAmountSummary(page).getByText(label, { exact: true }),
    ).toBeVisible({
        timeout: LONG,
    })
}

async function expectFulfillmentNotStarted(page: Page) {
    await expect(workPage(page).getByText("未开始", { exact: true }).first()).toBeVisible(
        {
            timeout: TIMEOUT,
        },
    )
}

async function expectNotClosed(page: Page) {
    const identity = workPage(page)
        .getByRole("heading", { level: 1 })
        .locator("xpath=ancestor::header[1]")
    await expect(identity.getByText("已生效", { exact: true })).toBeVisible({
        timeout: LONG,
    })
    await expect(identity.getByText("已关闭", { exact: true })).toHaveCount(0)
}

// ─── 工作台审批 ────────────────────────────────────────────────────────────

async function openWorkspaceApprovalTask(
    page: Page,
    typeLabel: string,
    hint: string,
) {
    await openWorkspaceTask(page, typeLabel, hint, "approval")
}

async function confirmApprove(page: Page) {
    await approveCurrentDocument(page)
}

async function approveWorkspaceTask(
    page: Page,
    typeLabel: string,
    hint: string,
) {
    await openWorkspaceApprovalTask(page, typeLabel, hint)
    await confirmApprove(page)
}

async function rejectWorkspaceTask(
    page: Page,
    typeLabel: string,
    hint: string,
    reason: string,
) {
    await openWorkspaceApprovalTask(page, typeLabel, hint)
    await rejectFinancialOriginal(page, reason)
}

// ─── 回款 / 客户退款 ──────────────────────────────────────────────────────

async function assertCustomerRefundPreview(
    page: Page,
    refundId: string,
    refundNo: string,
    status: "审批中" | "已驳回" | "已过账",
) {
    await page.goto(
        `/finance/customer-accounts?view=receipt&previewKind=refund&previewId=${encodeURIComponent(refundId)}`,
    )
    await expect(workPage(page).getByRole("heading", { name: "客户退款" })).toBeVisible({
        timeout: LONG,
    })
    await expect(workPage(page).getByText(`退款单：${refundNo}`)).toBeVisible({
        timeout: LONG,
    })
    await expect(
        workPage(page)
            .locator('[data-slot="quick-preview-summary"]')
            .getByText(status, { exact: true }),
    ).toBeVisible({ timeout: LONG })
}

async function assertReceiptStillPosted(page: Page, receiptNo: string) {
    await page.goto("/finance/customer-accounts?view=receipt")
    await waitHeading(page, "客户往来")
    await workPage(page).locator("#customer-receivables-view-receipt").click()
    await workPage(page).locator("#customer-receivables-toolbar-search").fill(receiptNo)
    await workPage(page).locator("#customer-receivables-toolbar-search").press("Enter")
    const row = workPage(page).getByRole("row").filter({ hasText: receiptNo })
    await expect(row).toBeVisible({ timeout: LONG })
    await expect(row.getByText("已过账", { exact: true })).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(row.getByText("已冲正")).toHaveCount(0)
}

async function openPostedReceiptPreview(page: Page, receiptNo: string) {
    await page.goto("/finance/customer-accounts?view=receipt")
    await waitHeading(page, "客户往来")
    await workPage(page).locator("#customer-receivables-view-receipt").click()
    await workPage(page).locator("#customer-receivables-toolbar-search").fill(receiptNo)
    await workPage(page).locator("#customer-receivables-toolbar-search").press("Enter")
    await expect(page).toHaveURL((url) => url.searchParams.get("q") === receiptNo)
    const row = workPage(page).getByRole("row").filter({ hasText: receiptNo })
    await expect(row).toBeVisible({ timeout: LONG })
    await expect(row.getByText("已过账", { exact: true })).toBeVisible({
        timeout: TIMEOUT,
    })
    await row.click()
    await expectReceiptPreview(page, receiptNo)
}

async function submitCustomerRefundFromReceipt(
    page: Page,
    receiptNo: string,
    reason: string,
) {
    await openPostedReceiptPreview(page, receiptNo)
    await expect(
        workPage(page).locator("#customer-receivables-preview-receipt-refund"),
    ).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(
        workPage(page).locator("#customer-receivables-preview-receipt-reverse"),
    ).toBeVisible({
        timeout: TIMEOUT,
    })
    await workPage(page).locator("#customer-receivables-preview-receipt-refund").click()
    const { refundId, refundNo, status } = await submitCustomerRefundRequest(page, reason)
    expect(status).toBe("IN_APPROVAL")
    expect(refundId).toBeTruthy()
    expect(refundNo).toBeTruthy()
    await expect(page).toHaveURL((url) =>
        url.searchParams.get("previewKind") === "refund" &&
        url.searchParams.get("previewId") === refundId,
    )
    return { refundNo, refundId }
}

async function assertCaiwuCannotSubmitCustomerRefund(
    page: Page,
    receiptNo: string,
) {
    await openPostedReceiptPreview(page, receiptNo)
    await expect(workPage(page).locator("#customer-receivables-preview-receipt-refund")).toHaveCount(0)
}

async function assertCaiwuCannotSubmitSupplierRefund(
    page: Page,
    payment: { id: string; payment_no: string; effective_amount: string },
) {
    const token = await apiToken("caiwu")
    const response = await page.request.post(`${API_BASE}/admin/supplier-refunds/commit`, {
        headers: { Authorization: `Bearer ${token}` },
        data: {
            source_fact_id: payment.id,
            amount: payment.effective_amount,
            reason: "财务总监不得提交自己的供应商退款",
            idempotency_key: `denied-f16-${payment.payment_no}`,
        },
    })
    const result = await response.json()
    expect(response.status(), result.errorMessage).toBe(403)
    expect(result).toMatchObject({ success: false, code: "PERMISSION_DENIED" })
}

type RefundDetail = {
    id: string
    refund_no: string
    status: string
    amount: string
    reason_text: string
    original_receipt_id?: string | null
    original_payment_id?: string | null
    original_receivable_entry_id?: string | null
    original_payable_entry_id?: string | null
    approval?: {
        instance?: {
            id: string
            status: string
            current_round_no: number
            subject_version?: number | string
        }
    }
}

/** 驳回结束审批；编辑同一退款单后按新版本提交。 */
async function reviseRefundOriginal(
    page: Page,
    input: {
        kind: "customer_refund" | "supplier_refund"
        id: string
        refundNo: string
        reason: string
    },
) {
    const resource =
        input.kind === "customer_refund"
            ? "customer-refunds"
            : "supplier-refunds"
    const token = await apiToken("fukuan")
    const before = await apiGet<RefundDetail>(
        token,
        `/admin/${resource}/${input.id}`,
    )
    expect(before.status).toBe("REJECTED")
    expect(before.approval?.instance?.status).toBe("REJECTED")
    expect(before.refund_no).toBe(input.refundNo)
    expect(before.approval?.instance?.current_round_no).toBe(1)
    expect(before.approval?.instance?.id).toBeTruthy()
    const originalSubjectVersion = await readFinancialApprovalVersion(
        input.kind,
        input.id,
        before.approval!.instance!.id,
    )
    await workPage(page).getByRole("button", { name: "修改原单", exact: true }).click()
    const editor = workPage(page).getByRole("dialog", {
        name:
            input.kind === "customer_refund"
                ? "修改客户退款单"
                : "修改供应商退款单",
        exact: true,
    })
    await expect(editor).toBeVisible({ timeout: LONG })
    await expect(page).toHaveURL(new RegExp(`editId=${input.id}`), {
        timeout: LONG,
    })
    await expect(editor.getByLabel(/^金额\s*\*?$/)).toHaveValue(before.amount)
    await expect(editor.getByLabel(/^原因说明\s*\*?$/)).toHaveValue(before.reason_text)
    await editor.getByLabel(/^原因说明\s*\*?$/).fill(input.reason)
    await editor.getByLabel(/^重新提交说明\s*\*?$/).fill("已补齐原退款单依据")
    const submitted = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes(`/admin/${resource}/${input.id}/submit`),
        { timeout: LONG },
    )
    await editor
        .getByRole("button", { name: "保存并提交审批", exact: true })
        .click()
    const response = await submitted
    expect(response.ok(), await response.text()).toBeTruthy()
    await expect(editor).toBeHidden({ timeout: LONG })
    const resubmitted = await apiGet<RefundDetail>(
        token,
        `/admin/${resource}/${input.id}`,
    )
    expect(resubmitted.id).toBe(input.id)
    expect(resubmitted.refund_no).toBe(input.refundNo)
    expect(resubmitted.status).toBe("IN_APPROVAL")
    expect(resubmitted.amount).toBe(before.amount)
    expect(resubmitted.reason_text).toBe(input.reason)
    for (const field of [
        "original_receipt_id",
        "original_payment_id",
        "original_receivable_entry_id",
        "original_payable_entry_id",
    ] as const) {
        expect(resubmitted[field]).toBe(before[field])
    }
    expect(resubmitted.approval?.instance?.id).not.toBe(
        before.approval?.instance?.id,
    )
    const resubmittedSubjectVersion = await readFinancialApprovalVersion(
        input.kind,
        input.id,
        resubmitted.approval!.instance!.id,
    )
    expect(resubmittedSubjectVersion).toBe(originalSubjectVersion + 1)
    expect(resubmitted.approval?.instance?.current_round_no).toBe(1)
}
