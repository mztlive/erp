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
 * - 种子供应商付款条件为 PREPAY_50，仅供应商 API 下单受门禁；手工入库和仓发可先办理
 * - 工作台 registry 短名为「今日工作台」，W01 PageHeader 实际为「我的工作台」
 * - 销售单生效只形成可申请开票额度；xiaoshou 提交开票申请，caiwu 审批通过后 kaipiao 才有销项开票任务
 * - 销售单票款页只读，回款改在客户往来页登记
 */
import { workPage } from "../helpers/workspace-page"
import { setBeijingShippingAddress } from "../helpers/sales-orders"
import { archiveContractViaUi } from "../helpers/contracts";
import {
    expectSalesOrderContractSummary,
    fillSalesLineUnitPrice,
    selectExistingContractForSalesOrder,
} from "../helpers/sales-orders"
import path from "node:path";
import { test, expect, type Page } from "../helpers/test";

import { createCustomerViaUi } from "../helpers/customers";
import { apiGet, apiToken } from "../helpers/api";
import { fillFirstDeliveryParcel, openFulfillmentTaskForSource, uploadAcceptanceEvidence } from "../helpers/fulfillment";
import { completeSalesInvoiceFromWorkspace, submitSalesInvoiceRequest } from "../helpers/invoices";
import { ensureWarehouseSalesSourceAccess } from "../helpers/inventory";
import { openLoggedInWorkspace, type LoggedInSession } from "../helpers/login";
import { payOnlySupplierTask } from "../helpers/payments";
import {
    ensureDefaultProcurementOwner,
    submitCreatedSalesOrder,
} from "../helpers/procurement";
import { registerCustomerReceiptForOrder } from "../helpers/receipts";
import { confirmSupplyAllocation, expandSourcingEditor } from "../helpers/sourcing";
import {
    workspaceTaskId,
    approveCurrentDocument,
    chooseOption,
    expectToast,
    openFulfillmentWorkspaceForm,
    openWorkspaceTask,
    pickCalendarDay,
    readHeaderDocumentNumber,
    expectSourcingCounts,
} from "../helpers/ui";

const UI_TIMEOUT = 20_000;
const FLOW_TIMEOUT = 12 * 60 * 1000;
const CONTRACT_PDF = path.resolve(process.cwd(), "fixtures/sample-contract.pdf");
const SKU_KEYWORD = "龙井";
const SKU_NAME = "狮峰明前龙井礼盒";
const WAREHOUSE_CODE = "BJ-TZ-01";
const SALES_QTY = "2";
const SALES_UNIT_PRICE = "1288.00";

type LoginName =
    | "xiaoshou"
    | "caigou"
    | "cangchu"
    | "caiwu" | "caigoushenpi"
    | "fukuan"
    | "kaipiao"
    | "admin";

function orderTitleRow(page: Page, customerName: string) {
    return workPage(page).getByRole("heading", { name: customerName }).locator("xpath=..");
}

async function confirmFormal(page: Page, title: string | RegExp, confirmName: string | RegExp) {
    const dialog = workPage(page).getByRole("alertdialog", { name: title, exact: true });
    await expect(dialog).toHaveCount(1);
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT });
    await dialog.getByRole("button", { name: confirmName, exact: true }).click();
    await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT });
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
        await ensureWarehouseSalesSourceAccess();

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
        await expect(workPage(page).getByRole("link", { name: shortName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });

        // 2) W04 上传合同 PDF
        await page.goto("/sales/contracts");
        await expect(
            workPage(page).getByRole("heading", { name: "合同", exact: true }),
        ).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await workPage(page).locator("#page-actions-action-upload").click();
        await archiveContractViaUi(page, { contractNo, customerName, pdf: CONTRACT_PDF })
        await expect(workPage(page).getByText(contractNo).first()).toBeVisible({ timeout: UI_TIMEOUT });

        // 3) W05 销售单：实物 SKU + 货到付款，提交后进入采购确认
        await page.goto("/sales/orders");
        await expect(workPage(page).getByRole("heading", { name: "销售单", exact: true })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await workPage(page).locator("#sales-orders-list-header-create").click();
        await selectExistingContractForSalesOrder(page, contractNo);
        await expect(workPage(page).getByRole("heading", { name: "销售明细" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).getByLabel("供应商")).toHaveCount(0);
        await expect(workPage(page).getByLabel("履约责任")).toHaveCount(0);
        await expect(workPage(page).getByLabel("采购成本")).toHaveCount(0);
        await expectSalesOrderContractSummary(page, { customerName, paymentTerms: "货到 15 天", contractNo });
        await chooseOption(
            page,
            workPage(page).locator("#sales-orders-create-header-welfare-scene"),
            "礼包",
        );
        await workPage(page).locator("#sales-orders-create-line-items-add").click();
        const skuDialog = workPage(page).getByRole("dialog", { name: "添加商品" });
        await expect(skuDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await skuDialog
            .locator("#sales-orders-sku-picker-filter-search")
            .fill(SKU_KEYWORD);
        await skuDialog.locator("#sales-orders-sku-picker-filter-search").press("Enter");
        const skuRow = skuDialog.getByRole("checkbox", { name: new RegExp(`选择.*${SKU_NAME}`) });
        await expect(skuRow.first()).toBeVisible({ timeout: UI_TIMEOUT });
        await skuRow.first().check();
        await skuDialog.locator("#sales-orders-sku-picker-confirm").click();
        await expect(skuDialog).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(workPage(page).getByText(SKU_NAME).first()).toBeVisible({ timeout: UI_TIMEOUT });
        await workPage(page).getByLabel("数量").fill(SALES_QTY);
        await fillSalesLineUnitPrice(page, SALES_UNIT_PRICE);
        await setBeijingShippingAddress(page)
        await workPage(page).locator("#sales-orders-create-batch-due-date-open").click()
        await pickCalendarDay(
            page,
            workPage(page).locator("#sales-orders-create-batch-due-date"),
            dueDate,
        );
        await workPage(page).locator("#sales-orders-create-batch-due-date-apply").click();
        await expectToast(page, "已批量设置交期");
        await submitCreatedSalesOrder(page);
        const submitDialog = workPage(page).getByRole("dialog", { name: "提交销售单" });
        await expect(submitDialog.getByText("审批中")).toBeVisible();
        await submitDialog.locator("#sales-orders-submit-confirm-confirm").click();
        await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, { timeout: UI_TIMEOUT });
        salesOrderId = page.url().split("/sales/orders/")[1]?.split("?")[0] ?? "";
        expect(salesOrderId).toBeTruthy();
        await expect(workPage(page).getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("审批中")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        salesOrderNo = await readHeaderDocumentNumber(page)
        expect(salesOrderNo).toBeTruthy();
        await expect(workPage(page).locator("#sales-orders-detail-start-change")).toBeDisabled();
        await expect(workPage(page).locator("#sales-order-follow-up-delivery").locator("..")).toContainText("待采购", {
            timeout: UI_TIMEOUT,
        });

        // 4) W01 采购确认节点：只通过/驳回，不选源、不录入成本/交期
        page = await switchTo("caigou");
        await openWorkspaceTask(page, "销售单审批", salesOrderNo, "approval");
        await expect(workPage(page).getByLabel("供给来源 / 履约责任")).toHaveCount(0);
        await expect(workPage(page).getByLabel("含税成本")).toHaveCount(0);
        await expect(workPage(page).getByLabel("预计交付日")).toHaveCount(0);
        await approveCurrentDocument(page);

        // 5) 销售单生效后才出现供给分配；确认全部走外部采购入仓
        await openWorkspaceTask(page, "待安排采购", salesOrderNo, "procurement");
        await expect(workPage(page).getByRole("heading", { name: "供给分配" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).getByRole("region", { name: "供给商品列表" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expandSourcingEditor(page);
        const sourcingOption = workPage(page)
            .locator('[id^="procurement-orders-create-row-"][id$="-sourcing-option"]')
            .first();
        await expect(sourcingOption).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(page, sourcingOption, /入仓/, "入仓");
        const warehouseInput = workPage(page)
            .locator('[id^="procurement-orders-create-row-"][id$="-warehouse"]')
            .first();
        await expect(warehouseInput).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(page, warehouseInput, new RegExp(WAREHOUSE_CODE), WAREHOUSE_CODE);
        await workPage(page).locator("#sourcing-product-adjust-done").click();
        await expectSourcingCounts(page, 1, 0);
        await workPage(page).locator("#procurement-orders-create-preview").click();
        const previewDialog = workPage(page).getByRole("dialog", { name: "预览供给分配" });
        await expect(previewDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(previewDialog.getByText("现有库存分配")).toHaveCount(0);
        await expect(previewDialog.getByText(/确认提交 1 张采购单/)).toBeVisible();
        const allocation = await confirmSupplyAllocation(page);
        expect(allocation.orders).toHaveLength(1);
        const approvedPurchaseNo = allocation.orders[0]!.purchase_no;

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).locator("#sales-order-follow-up-delivery").locator("..")).toContainText("采购已覆盖", {
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).getByText("草稿")).toHaveCount(0);
        expect(await apiGet<{ purchase_order_count: number }>(
            await apiToken("xiaoshou"), `/admin/sales-orders/${salesOrderId}`,
        )).toMatchObject({ purchase_order_count: 1 });

        // 6) 采购单由供给分配立即提交，采购部门审批后生效并形成应付
        page = await switchTo("caigoushenpi");
        await openWorkspaceTask(page, "采购单审批", approvedPurchaseNo, "approval");
        await approveCurrentDocument(page);

        // 7) 先手工入库、仓发，再按本采购单办理供应商付款。
        const purchaseOrderId = allocation.orders[0]!.purchase_order_id;
        page = await switchTo("cangchu");
        await openFulfillmentTaskForSource(page, { salesOrderId, purchaseOrderId, operation: "RECEIPT" });
        const receiptForm = await openFulfillmentWorkspaceForm(page);
        const receipt = receiptForm.getByRole("region", { name: "入库表单", exact: true });
        await expect(receipt).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(receiptForm.getByRole("button", { name: "过账" })).toHaveCount(0);
        await receipt.getByLabel("到货数量").fill(SALES_QTY);
        await chooseOption(page, receipt.getByLabel("质量结果"), "合格");
        const confirmReceipt = receiptForm.locator("#fulfillment-operations-work-surface-confirm");
        await expect(confirmReceipt).toBeEnabled({ timeout: UI_TIMEOUT });
        await confirmReceipt.click();
        const receiptPosted = page.waitForResponse(response => response.request().method() === "POST" &&
            /^\/admin\/purchase-receipts\/[^/]+\/post$/.test(new URL(response.url()).pathname),
            { timeout: UI_TIMEOUT });
        await confirmFormal(page, "确认入库？", "确认入库");
        const receiptResponse = await receiptPosted;
        expect(receiptResponse.ok(), await receiptResponse.text()).toBe(true);
        expect((await receiptResponse.json()).data).toMatchObject({ purchase_order_id: purchaseOrderId, status: "POSTED" });

        await openFulfillmentTaskForSource(page, { salesOrderId, operation: "WAREHOUSE_SHIP" });
        const shippingDialog = await openFulfillmentWorkspaceForm(page);
        await fillFirstDeliveryParcel(page, `SF${stamp}`, "顺丰速运");
        const workItemId = workspaceTaskId(page);
        expect(workItemId).toBeTruthy();
        const token = await page.evaluate(() => localStorage.getItem("erp.token"));
        expect(token).toBeTruthy();
        const task = await apiGet<{
            id: string; business_object_type: string; business_object_id: string; status: string;
        }>(token!, `/admin/work-items/${workItemId}`);
        expect(task).toMatchObject({ id: workItemId, business_object_type: "delivery", status: "OPEN" });
        const source = await apiGet<{
            total: number;
            items: Array<{ work_item_id: string; operation_id: string; operation_type: string;
                sales_order_id: string }>;
        }>(token!, "/admin/work-items/fulfillment-queue", {
            operation_id: task.business_object_id, operation_types: "WAREHOUSE_SHIP",
            page: 1, page_size: 1, timezone: "Asia/Shanghai",
        });
        expect(source.total).toBe(1);
        expect(source.items).toHaveLength(1);
        expect(source.items[0]).toMatchObject({
            work_item_id: workItemId, operation_id: task.business_object_id,
            operation_type: "WAREHOUSE_SHIP", sales_order_id: salesOrderId,
        });
        const shipped = page.waitForResponse(response => response.request().method() === "POST" &&
            new URL(response.url()).pathname === `/admin/deliveries/${task.business_object_id}/post`,
            { timeout: UI_TIMEOUT });
        await shippingDialog.locator("#fulfillment-operations-work-surface-confirm").click();
        const shippedResponse = await shipped;
        expect(shippedResponse.ok(), await shippedResponse.text()).toBe(true);
        const delivery = (await shippedResponse.json()).data;
        expect(delivery).toMatchObject({
            id: task.business_object_id, status: "SHIPPED", delivery_type: "WAREHOUSE_SHIP",
            sales_order_id: salesOrderId, purchase_order_id: null,
        });
        expect(delivery.tracking_entries).toHaveLength(1);
        expect(delivery.tracking_entries[0]).toMatchObject({ tracking_no: `SF${stamp}`, carrier: "顺丰速运" });
        expect(shippedResponse.request().postDataJSON()).toMatchObject({
            packing: { packages: [{ tracking_no: `SF${stamp}`, carrier: "顺丰速运", items: [{ quantity: SALES_QTY }] }] },
        });
        expect(await apiGet(token!, `/admin/work-items/${workItemId}`)).toMatchObject({
            id: workItemId, business_object_id: task.business_object_id, status: "COMPLETED",
        });
        await expect(shippingDialog).toBeHidden({ timeout: UI_TIMEOUT });

        page = await switchTo("fukuan");
        await payOnlySupplierTask(page, approvedPurchaseNo);

        // 8) 销售登记客户验收。验收后未回款不得关闭；开票未完成不阻塞关闭
        page = await switchTo("xiaoshou");
        await openWorkspaceTask(page, "客户验收登记", salesOrderNo, "fulfillment");
        await workPage(page).locator("#sales-orders-acceptance-register-open").click();
        const acceptanceDialog = workPage(page).getByRole("dialog", { name: "登记客户验收" });
        await expect(acceptanceDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await uploadAcceptanceEvidence(page);
        await acceptanceDialog.locator("#sales-orders-acceptance-register-submit").click();
        await confirmFormal(page, "确认客户验收", "确认本次验收");

        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(workPage(page).getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toHaveCount(0);
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible();
        await expect(workPage(page).getByText("已完成").first()).toBeVisible();
        await expect(workPage(page).getByText("未收").first()).toBeVisible();
        await expect(workPage(page).getByText(/未开/).first()).toBeVisible();
        await expect(workPage(page).getByText("应收结清").locator("xpath=..")).toBeVisible();

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
        await expect(workPage(page).getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).getByText("已结清").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(workPage(page).getByText(/未开|部分开票/).first()).toBeVisible();

        // 10) 销售提交开票申请，财务审批后开票人 W01 登记销项发票；开票不阻塞关闭
        const invoiceRequestId = await submitSalesInvoiceRequest(page, {
            salesOrderId,
            amount: "2576.00",
            taxNumber: creditCode,
            title: customerName,
        });
        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "开票申请审批", customerName, "approval");
        await approveCurrentDocument(page);

        page = await switchTo("kaipiao");
        await completeSalesInvoiceFromWorkspace(page, {
            requestId: invoiceRequestId, orderNo: salesOrderNo, amount: "2576.00", evidenceName: `INV${stamp}`,
        });

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).getByText("已完成").first()).toBeVisible();
        await expect(workPage(page).getByText("已结清").first()).toBeVisible();
        await expect(workPage(page).getByText("已开齐").first()).toBeVisible();
        await expect(workPage(page).getByRole("button", { name: "过账" })).toHaveCount(0);
    } finally {
        await session?.context.close();
    }
});
