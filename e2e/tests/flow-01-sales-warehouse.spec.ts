/**
 * 流程: [flow-01] 外部采购入仓后仓发完整基准流程
 * 文档: docs/erp-phase-1.md §7.3.1 + §7.4（供给分配）+ §9.1/§9.3（票款与关闭）
 * 账号: xiaoshou / caigou / cangchu / caiwu / fukuan / kaipiao / admin
 *
 * 文档-代码差异（编写时已对照组件源码）:
 * - 开票进度 COMPLETED：文档写「已完成」，页面 mapInvoicing 为「已开齐」
 * - 销售单关闭：文档列关闭条件，代码由服务端在履约完成且应收结清后自动关闭，无「关闭销售单」按钮
 * - W06 不是独立页面，而是销售单详情 `section=acceptance`；验收任务在 W01 原地处理
 * - 文档 7.3.1 时序把「采购创建采购单」和「提交审批」画成两步；代码在供给分配确认同一事务内建单并立即提交
 * - 种子供应商狮峰茶叶付款条件为 PREPAY_50，入库/仓发可能被先款门禁拦住；本流程客户侧用货到付款
 * - 工作台 registry 短名为「今日工作台」，W01 PageHeader 实际为「我的工作台」
 * - 销售单生效只形成可申请开票额度；xiaoshou 提交开票申请，caiwu 审批通过后 kaipiao 才有销项开票任务
 * - 销售单票款页只读，回款改在客户往来页登记
 */
import path from "node:path";
import { test, expect, type Page } from "@playwright/test";

import { createCustomerViaUi } from "../helpers/customers";
import { submitSalesInvoiceRequest } from "../helpers/invoices";
import { openLoggedInWorkspace, type LoggedInSession } from "../helpers/login";
import { payOnlySupplierTask } from "../helpers/payments";
import {
    ensureDefaultProcurementOwner,
    submitCreatedSalesOrder,
} from "../helpers/procurement";
import { registerCustomerReceiptForOrder } from "../helpers/receipts";
import { confirmSupplyAllocation, expandSourcingEditor } from "../helpers/sourcing";
import {
    approveCurrentDocument,
    chooseOption,
    expectToast,
    openFulfillmentWorkspaceForm,
    openWorkspaceTask,
    pickCalendarDay,
    readHeaderDocumentNumber,
} from "../helpers/ui";

const UI_TIMEOUT = 20_000;
const FLOW_TIMEOUT = 12 * 60 * 1000;
const CONTRACT_PDF = path.resolve(process.cwd(), "fixtures/sample-contract.pdf");
const SKU_KEYWORD = "龙井";
const SKU_NAME = "狮峰明前龙井礼盒";
const WAREHOUSE_NAME = "北京通州仓";
const WAREHOUSE_CODE = "BJ-TZ-01";
const SALES_QTY = "2";

type LoginName =
    | "xiaoshou"
    | "caigou"
    | "cangchu"
    | "caiwu"
    | "fukuan"
    | "kaipiao"
    | "admin";

function orderTitleRow(page: Page, customerName: string) {
    return page.getByRole("heading", { name: customerName }).locator("xpath=..");
}

async function confirmFormal(page: Page, title: string | RegExp, confirmName: string | RegExp) {
    const dialog = page.getByRole("alertdialog").or(page.getByRole("dialog")).filter({
        hasText: title,
    });
    await expect(dialog.first()).toBeVisible({ timeout: UI_TIMEOUT });
    await dialog.getByRole("button", { name: confirmName }).click();
    await expect(dialog.first()).toBeHidden({ timeout: UI_TIMEOUT });
}

function plusDaysIso(days: number): string {
    const date = new Date();
    date.setDate(date.getDate() + days);
    const pad = (value: number) => String(value).padStart(2, "0");
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function uniqueCreditCode(stamp: string): string {
    const raw = `91${stamp.replace(/[^0-9A-Za-z]/g, "").toUpperCase()}E2EWAREHOUSE`;
    return raw.slice(0, 18).padEnd(18, "0");
}

test.describe.configure({ mode: "serial" });

test("flow-01 外部采购入仓后由公司仓库发货", async ({ browser }) => {
    test.setTimeout(FLOW_TIMEOUT);
    const stamp = Date.now().toString(36).toUpperCase();
    const customerName = `E2E仓发客户${stamp}`;
    const creditCode = uniqueCreditCode(stamp);
    const contractNo = `HT-E2E-WH-${stamp}`;
    const dueDate = plusDaysIso(21);
    let session: LoggedInSession | undefined;
    let salesOrderId = "";
    let salesOrderNo = "";

    const switchTo = async (login: LoginName) => {
        await session?.context.close();
        session = await openLoggedInWorkspace(browser, login);
        return session.page;
    };

    try {
        // 0) 采购责任默认调度人：销售提交实物单前必须能解析采购负责人
        let page = await switchTo("admin");
        await ensureDefaultProcurementOwner(page);

        // 1) W03 客户创建
        page = await switchTo("xiaoshou");
        const shortName = `仓发${stamp}`;
        await createCustomerViaUi(page, {
            legalName: customerName,
            shortName,
            creditCode,
            paymentTermLabel: "货到 15 天",
            contact: { name: "李测", phone: "13800138001" },
            address: "北京市朝阳区测试路 1 号",
        });
        await expect(page.getByRole("link", { name: shortName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });

        // 2) W04 上传合同 PDF
        await page.goto("/sales/contracts");
        await expect(
            page.getByRole("heading", { name: "合同", exact: true }),
        ).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#page-actions-action-upload").click();
        const contractDialog = page.getByRole("dialog", { name: "上传合同 PDF" });
        await expect(contractDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await contractDialog.locator("#card-contracts-upload-pdf-input").setInputFiles(CONTRACT_PDF);
        await contractDialog.locator("#card-contracts-upload-contract-no").fill(contractNo);
        await chooseOption(
            page,
            contractDialog.locator("#card-contracts-upload-customer"),
            customerName,
        );
        await expect(
            contractDialog.locator("#card-contracts-upload-settlement-party"),
        ).not.toHaveValue("", { timeout: UI_TIMEOUT });
        await chooseOption(
            page,
            contractDialog.locator("#card-contracts-upload-payment-terms"),
            "货到 15 天",
        );
        await contractDialog.locator("#card-contracts-upload-submit").click();
        await expectToast(page, "合同 PDF 已归档");
        await expect(contractDialog).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(page.getByText(contractNo).first()).toBeVisible({ timeout: UI_TIMEOUT });

        // 3) W05 销售单：实物 SKU + 货到付款，提交后进入采购确认
        await page.goto("/sales/orders");
        await expect(page.getByRole("heading", { name: "销售单", exact: true })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#sales-orders-list-header-create").click();
        await expect(page.getByRole("heading", { name: "销售明细" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByLabel("供应商")).toHaveCount(0);
        await expect(page.getByLabel("履约责任")).toHaveCount(0);
        await expect(page.getByLabel("采购成本")).toHaveCount(0);
        await chooseOption(page, page.locator("#sales-orders-create-contract"), contractNo);
        await expect(page.getByText(customerName).first()).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(
            page,
            page.locator("#sales-orders-create-header-welfare-scene"),
            "年节礼包",
        );
        await chooseOption(
            page,
            page.locator("#sales-orders-create-header-payment-terms"),
            "货到 15 天",
        );
        await page.locator("#sales-orders-create-line-items-add").click();
        const skuDialog = page.getByRole("dialog", { name: "添加商品" });
        await expect(skuDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await skuDialog
            .locator("#master-data-list-sellable-list-toolbar-search-input")
            .fill(SKU_KEYWORD);
        await skuDialog.locator("#master-data-list-sellable-list-toolbar-search-input").press("Enter");
        const skuRow = skuDialog.getByRole("checkbox", { name: new RegExp(`选择.*${SKU_NAME}`) });
        await expect(skuRow.first()).toBeVisible({ timeout: UI_TIMEOUT });
        await skuRow.first().check();
        await skuDialog.locator("#sales-orders-sku-picker-confirm").click();
        await expect(skuDialog).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(page.getByText(SKU_NAME).first()).toBeVisible({ timeout: UI_TIMEOUT });
        await page.getByLabel("数量").fill(SALES_QTY);
        await page.locator("#sales-orders-create-batch-due-date-open").click()
        await pickCalendarDay(
            page,
            page.locator("#sales-orders-create-batch-due-date"),
            dueDate,
        );
        await page.locator("#sales-orders-create-batch-due-date-apply").click();
        await expectToast(page, "已批量设置交期");
        await submitCreatedSalesOrder(page);
        const submitDialog = page.getByRole("dialog", { name: "提交销售单" });
        await expect(submitDialog.getByText("审批中")).toBeVisible();
        await submitDialog.locator("#sales-orders-submit-confirm-confirm").click();
        await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, { timeout: UI_TIMEOUT });
        salesOrderId = page.url().split("/sales/orders/")[1]?.split("?")[0] ?? "";
        expect(salesOrderId).toBeTruthy();
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("审批中")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        salesOrderNo = await readHeaderDocumentNumber(page)
        expect(salesOrderNo).toBeTruthy();
        await expect(page.locator("#sales-orders-detail-start-change")).toBeDisabled();
        await page.getByRole("tab", { name: /采购/ }).click();
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("待采购", {
            timeout: UI_TIMEOUT,
        });

        // 4) W01 采购确认节点：只通过/驳回，不选源、不录入成本/交期
        page = await switchTo("caigou");
        await openWorkspaceTask(page, "销售单审批", salesOrderNo, "approval");
        await expect(page.getByLabel("供给来源 / 履约责任")).toHaveCount(0);
        await expect(page.getByLabel("含税成本")).toHaveCount(0);
        await expect(page.getByLabel("预计交付日")).toHaveCount(0);
        await approveCurrentDocument(page);

        // 5) 销售单生效后才出现供给分配；确认全部走外部采购入仓
        await page.getByRole("button", { name: "刷新" }).click();
        await openWorkspaceTask(page, "待供给分配", salesOrderNo, "procurement");
        await expect(page.getByRole("heading", { name: "供给分配" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByRole("heading", { name: "销售明细与供给方案" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expandSourcingEditor(page);
        const sourcingOption = page
            .locator('[id^="procurement-orders-create-row-"][id$="-sourcing-option"]')
            .first();
        await expect(sourcingOption).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(page, sourcingOption, /入仓/, "入仓");
        const warehouseInput = page
            .locator('[id^="procurement-orders-create-row-"][id$="-warehouse"]')
            .first();
        await expect(warehouseInput).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(page, warehouseInput, new RegExp(WAREHOUSE_CODE), WAREHOUSE_CODE);
        await expect(page.getByText("将创建采购单").locator("xpath=..")).toContainText("1 张");
        await expect(page.getByText("将建立库存预留").locator("xpath=..")).toContainText("0 条");
        await page.locator("#procurement-orders-create-preview").click();
        const previewDialog = page.getByRole("dialog", { name: "预览供给分配" });
        await expect(previewDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(previewDialog.getByText("现有库存分配")).toHaveCount(0);
        await expect(previewDialog.getByText(/确认提交 1 张采购单/)).toBeVisible();
        await confirmSupplyAllocation(page, /供给分配已完成|本次供给分配已保存/);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.getByRole("tab", { name: /采购/ }).click();
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("采购已覆盖", {
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("草稿")).toHaveCount(0);
        // 销售账号无采购单明细查看权限，面板仅显示计数提示，不显示审批中/已生效。
        await expect(page.getByTestId("sales-order-purchase-count-only")).toContainText(
            /已创建 1 张采购单/,
            { timeout: UI_TIMEOUT },
        );

        // 6) 采购单由供给分配立即提交，财务总监审批后生效并形成应付
        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "采购单审批", salesOrderNo, "approval");
        await approveCurrentDocument(page);

        // 7) 先履约后付款：仓储入库 → 仓发。若供给是先款条件，则先由出纳确认付款
        page = await switchTo("cangchu");
        await openWorkspaceTask(page, "履约处理", customerName, "fulfillment");
        let receiptForm = await openFulfillmentWorkspaceForm(page);
        const gate = page.locator("#prepayment-gate");
        if (
            (await gate.count()) &&
            /暂时不能|先款未到/.test((await gate.innerText()) ?? "")
        ) {
            page = await switchTo("fukuan");
            await payOnlySupplierTask(page);
            page = await switchTo("cangchu");
            await openWorkspaceTask(page, "履约处理", customerName, "fulfillment");
            receiptForm = await openFulfillmentWorkspaceForm(page);
        }
        await expect(receiptForm.locator('[aria-label="入库表单"]')).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByRole("button", { name: "过账" })).toHaveCount(0);
        const receivedQty = page
            .locator('[id^="fulfillment-operations-receipt-form-received-quantity-"]')
            .first();
        if (await receivedQty.count()) {
            const current = await receivedQty.inputValue();
            if (!current || current === "0") {
                await receivedQty.fill(SALES_QTY);
            }
        }
        const quality = page
            .locator('[id^="fulfillment-operations-receipt-form-quality-result-"]')
            .first();
        if (await quality.count()) {
            await chooseOption(page, quality, "合格");
        }
        const confirmReceipt = page.locator("#fulfillment-operations-work-surface-confirm");
        if (!(await confirmReceipt.isEnabled().catch(() => false))) {
            page = await switchTo("fukuan");
            await payOnlySupplierTask(page);
            page = await switchTo("cangchu");
            await openWorkspaceTask(page, "履约处理", customerName, "fulfillment");
            receiptForm = await openFulfillmentWorkspaceForm(page);
        }
        await expect(page.locator("#fulfillment-operations-work-surface-confirm")).toBeEnabled({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#fulfillment-operations-work-surface-confirm").click();
        await confirmFormal(page, "确认入库？", "确认入库");

        const shipForm = page.locator('[aria-label="公司仓发表单"]');
        if (!(await shipForm.isVisible().catch(() => false))) {
            const continueShip = page.locator(
                "#fulfillment-operations-result-continue-warehouse-ship",
            );
            if (await continueShip.count()) {
                await continueShip.click();
            } else {
                await openWorkspaceTask(page, "履约处理", customerName, "fulfillment");
            }
        }
        if (await page.getByRole("button", { name: "处理履约" }).isVisible().catch(() => false)) {
            await openFulfillmentWorkspaceForm(page);
        }
        await expect(page.locator('[aria-label="公司仓发表单"]')).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(
            page,
            page.locator("#fulfillment-operations-ship-form-carrier"),
            "顺丰速运",
        );
        await page
            .locator("#fulfillment-operations-ship-form-tracking-no")
            .fill(`SF${stamp}`);
        const shipQty = page
            .locator('[id^="fulfillment-operations-ship-form-quantity-"]')
            .first();
        if (await shipQty.count()) {
            const current = await shipQty.inputValue();
            if (!current || current === "0") {
                await shipQty.fill(SALES_QTY);
            }
        }
        await page.locator("#fulfillment-operations-work-surface-confirm").click();
        await confirmFormal(page, "确认发货？", "确认发货");

        // 8) 销售登记客户验收。验收后未回款不得关闭；开票未完成不阻塞关闭
        page = await switchTo("xiaoshou");
        await openWorkspaceTask(page, "客户验收登记", salesOrderNo, "fulfillment");
        await page.locator("#sales-orders-acceptance-register-open").click();
        const acceptanceDialog = page.getByRole("dialog", { name: "登记客户验收" });
        await expect(acceptanceDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await acceptanceDialog.locator("#sales-orders-acceptance-register-submit").click();
        await confirmFormal(page, "确认客户验收", "确认本次验收");

        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toHaveCount(0);
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible();
        await expect(page.getByText("已完成").first()).toBeVisible();
        await expect(page.getByText("未收").first()).toBeVisible();
        await expect(page.getByText(/未开/).first()).toBeVisible();
        await expect(page.getByText("应收结清").locator("xpath=..")).toBeVisible();

        // 9) 出纳在客户往来登记回款并提交，财务总监审批入账
        page = await switchTo("fukuan");
        await registerCustomerReceiptForOrder(page, {
            customerName,
            orderNo: salesOrderNo,
            amount: "2576.00",
            bankReference: `RC${stamp}`,
        });

        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "回款复核", customerName, "approval");
        await approveCurrentDocument(page);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("已结清").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByText(/未开|部分开票/).first()).toBeVisible();

        // 10) 销售提交开票申请，财务审批后开票人 W01 登记销项发票；开票不阻塞关闭
        await submitSalesInvoiceRequest(page, {
            salesOrderId,
            amount: "2576.00",
            taxNumber: creditCode,
            title: customerName,
        });
        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "开票申请审批", customerName, "approval");
        await approveCurrentDocument(page);

        page = await switchTo("kaipiao");
        await openWorkspaceTask(page, "销项开票处理", salesOrderNo, "finance");
        await expect(page.getByRole("heading", { name: `核销 · ${customerName}` })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#customer-receivables-session-invoice-no").fill(`INV${stamp}`);
        const gross = page.locator("#customer-receivables-session-gross-amount");
        if (!(await gross.inputValue())) {
            await gross.fill("2576.00");
        }
        const addInvoicePool = page.getByRole("button", { name: "加入" }).first();
        if (await addInvoicePool.count()) {
            await addInvoicePool.click();
            await expect(page.getByText("已加入").first()).toBeVisible({ timeout: UI_TIMEOUT });
        }
        const fillInvoice = page.getByRole("button", { name: "填满" }).first();
        await expect(fillInvoice).toBeVisible({ timeout: UI_TIMEOUT });
        await fillInvoice.click();
        await page.locator("#customer-receivables-session-submit").click();
        await expect(
            page.getByRole("heading", { name: "确认登记销项发票并分配" }),
        ).toBeVisible({ timeout: UI_TIMEOUT });
        await page
            .locator("#customer-receivables-session-invoice-confirm-dialog-confirm")
            .click({ force: true });
        // 工作台提交后任务完成、会话面板随之卸载（结果标题不渲染）；任务消失证明提交成功，
        // 发票登记即时生效，下游已开齐断言覆盖正确性。
        await expect(
            page.getByRole("button", {
                name: new RegExp(
                    `销项开票处理[\\s\\S]*${salesOrderNo}|${salesOrderNo}[\\s\\S]*销项开票处理`,
                ),
            }),
        ).toHaveCount(0, { timeout: UI_TIMEOUT });

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("已完成").first()).toBeVisible();
        await expect(page.getByText("已结清").first()).toBeVisible();
        await expect(page.getByText("已开齐").first()).toBeVisible();
        await expect(page.getByRole("button", { name: "过账" })).toHaveCount(0);
    } finally {
        await session?.context.close();
    }
});
