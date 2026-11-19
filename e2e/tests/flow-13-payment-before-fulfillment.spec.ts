/**
 * 流程: [flow-13] 预付款提醒和真实付款状态独立于人工履约。
 * 两张采购分别付款，核对各自提示从待预付到已满足，最后完成入库、仓发和直发。
 */
import { workPage } from "../helpers/workspace-page"
import { setBeijingShippingAddress } from "../helpers/sales-orders"
import { archiveContractViaUi } from "../helpers/contracts";
import { fillSalesLineUnitPrice, selectExistingContractForSalesOrder } from "../helpers/sales-orders"
import fs from "node:fs";
import path from "node:path";
import { test, expect, type Browser, type BrowserContext, type Page } from "../helpers/test";

import { createCustomerViaUi } from "../helpers/customers";
import { fillFirstDeliveryParcel } from "../helpers/fulfillment";
import { API_BASE, apiGet, apiToken } from "../helpers/api";
import {
    authorizePurchasePaymentRequest,
    BANK_RECEIPT_FILE,
    payOnlySupplierTask,
    type PurchasePaymentRequestResult,
} from "../helpers/payments";
import { ensureWarehouseSalesSourceAccess } from "../helpers/inventory";
import { openLoggedInWorkspace } from "../helpers/login";
import { confirmSupplyAllocation, expandSourcingEditor } from "../helpers/sourcing";
import {
    ensureDefaultProcurementOwner,
    submitCreatedSalesOrder,
} from "../helpers/procurement";
import {
    workspaceTaskRows,
    expectWorkspaceQueue,
    selectWorkspaceScope,
    workspaceTaskId,
    approveCurrentDocument,
    chooseOption,
    expectToast,
    openFulfillmentWorkspaceForm,
    matchingWorkspaceTaskRows,
    openWorkspaceTask,
    openWorkspaceTaskRow,
    pickCalendarDay,
    readHeaderDocumentNumber,
    selectWorkspaceFamily,
} from "../helpers/ui";

test.describe.configure({ mode: "serial" });

const UI_TIMEOUT = 20_000;
const FLOW_TIMEOUT = 12 * 60 * 1000;
const CONTRACT_PDF = path.resolve(process.cwd(), "fixtures/sample-contract.pdf");
const SKU_INBOUND = "狮峰明前龙井礼盒";
const SKU_DIRECT = "狮峰陈皮普洱礼盒";
const SUPPLIER_SHORT = "狮峰茶叶";
const WAREHOUSE_NAME = "北京通州仓";
const WAREHOUSE_CODE = "BJ-TZ-01";
const SALES_QTY = "1";
const PAYMENT_TERM_CUSTOMER = "货到 15 天";
const PAYMENT_TERM_SUPPLIER = "先款 50%";
const INBOUND_OPTION = /杭州狮峰茶叶有限公司.* · 集采入仓/;
const DIRECT_OPTION = /杭州狮峰茶叶有限公司.* · 代发直发/;
const MINIMAL_PDF = Buffer.from(
    "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R>>endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000068 00000 n \n0000000125 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n210\n%%EOF\n",
);

type LoginName = "xiaoshou" | "caigou" | "cangchu" | "caiwu" | "caigoushenpi" | "fukuan" | "admin";
type Session = { context: BrowserContext; page: Page };
type PurchaseRef = { id: string; no: string; responsibility: "入仓" | "供应商直发" };

function plusDaysIso(days: number): string {
    const date = new Date();
    date.setDate(date.getDate() + days);
    const pad = (value: number) => String(value).padStart(2, "0");
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function uniqueCreditCode(stamp: string): string {
    const raw = `91${stamp.replace(/[^0-9A-Za-z]/g, "").toUpperCase()}E2EPREPAY`;
    return raw.slice(0, 18).padEnd(18, "0");
}

function contractPdf(): string | { name: string; mimeType: string; buffer: Buffer } {
    if (fs.existsSync(CONTRACT_PDF)) return CONTRACT_PDF;
    return { name: "sample-contract.pdf", mimeType: "application/pdf", buffer: MINIMAL_PDF };
}

async function gotoHeading(page: Page, href: string, heading: string | RegExp) {
    await page.goto(href);
    await expect(workPage(page).getByRole("heading", { name: heading })).toBeVisible({ timeout: UI_TIMEOUT });
}

async function approvePurchaseTasks(page: Page, purchaseNos: string[]) {
    for (const purchaseNo of purchaseNos) {
        await openWorkspaceTask(page, "采购单审批", purchaseNo, "approval");
        await approveCurrentDocument(page);
    }
    await page.goto("/workspace/tasks");
    await selectWorkspaceFamily(page, "approval");
    for (const purchaseNo of purchaseNos) {
        await expect(matchingWorkspaceTaskRows(page, "采购单审批", purchaseNo)).toHaveCount(0);
    }
}

async function confirmReceipt(page: Page) {
    const dialog = workPage(page).getByRole("alertdialog", { name: "确认入库？", exact: true });
    await expect(dialog).toHaveCount(1);
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(dialog.getByText(/^合格 1(?:\.0+)? 入库存并留货。确认后不能改。$/)).toBeVisible();
    const confirm = dialog.locator("#fulfillment-operations-workspace-confirm-confirm");
    await expect(confirm).toHaveText("确认入库");
    const posted = page.waitForResponse(response =>
        response.request().method() === "POST" &&
        /^\/admin\/purchase-receipts\/[^/]+\/post$/.test(new URL(response.url()).pathname),
        { timeout: UI_TIMEOUT },
    );
    await confirm.click();
    const response = await posted;
    expect(response.ok(), await response.text()).toBe(true);
    expect((await response.json()).data.status).toBe("POSTED");
    await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT });
}

async function pickSku(page: Page, keyword: string, name: string) {
    await workPage(page).getByRole("button", { name: "添加商品" }).first().click();
    const skuDialog = workPage(page).getByRole("dialog", { name: /添加商品|更换销售商品/ });
    await expect(skuDialog).toBeVisible({ timeout: UI_TIMEOUT });
    const skuSearch = skuDialog.locator("#sales-orders-sku-picker-filter-search");
    await skuSearch.fill(keyword);
    await skuSearch.press("Enter");
    const checkbox = skuDialog.getByRole("checkbox", { name: new RegExp(`选择.*${name}`) });
    await expect(checkbox.first()).toBeVisible({ timeout: UI_TIMEOUT });
    await checkbox.first().check();
    await skuDialog.locator("#sales-orders-sku-picker-confirm").click();
    await expect(skuDialog).toBeHidden({ timeout: UI_TIMEOUT });
    await expect(workPage(page).getByText(name).first()).toBeVisible({ timeout: UI_TIMEOUT });
}

async function fillAllLineQuantities(page: Page, qty: string) {
    const inputs = workPage(page).locator('[id^="sales-orders-create-line-"][id$="-quantity"]');
    const count = await inputs.count();
    expect(count).toBeGreaterThan(0);
    for (let i = 0; i < count; i += 1) {
        await inputs.nth(i).fill(qty);
    }
}

async function chooseSourcing(
    page: Page,
    item: string,
    option: RegExp,
    warehouse?: string,
) {
    const task = workPage(page).getByRole("region", { name: "当前供给分配任务", exact: true });
    const itemName = item.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const row = task.getByRole("region", { name: "供给商品列表", exact: true })
        .getByRole("row").filter({
            has: workPage(page).getByRole("button", { name: new RegExp(`^调整 ${itemName}(?:\\s|$)`) }),
        });
    await expect(row).toHaveCount(1);
    await expandSourcingEditor(page, item);
    const editor = workPage(page).getByRole("dialog", { name: /^供给分配 · / });
    const sourcing = editor.locator('[id$="-sourcing-option"]:visible');
    await expect(sourcing).toHaveCount(1);
    await expect(sourcing).toBeVisible({ timeout: UI_TIMEOUT });
    // 选项很多时第一次点击可能打在重绘前的条目上，下拉仍开着且值是空的。再选一次。
    let attempt = 0;
    await expect(async () => {
        attempt += 1;
        if (attempt > 1) await page.keyboard.press("Escape");
        await chooseOption(page, sourcing, option, option.source.includes("直发") ? "直发" : "入仓");
    }).toPass({ timeout: UI_TIMEOUT * 2 });
    if (warehouse) {
        const warehouseInput = editor.locator('[id$="-warehouse"]:visible');
        await expect(warehouseInput).toHaveCount(1);
        await expect(warehouseInput).toBeVisible({ timeout: UI_TIMEOUT });
        // 仓库列表接口以仓库代码标识选项。
        await chooseOption(page, warehouseInput, WAREHOUSE_CODE, WAREHOUSE_CODE);
    } else {
        await expect(editor.locator('[id$="-warehouse"]:visible')).toHaveCount(0);
    }
    await expect(editor.getByText(`付款 ${PAYMENT_TERM_SUPPLIER}`, { exact: true })).toBeVisible();
    await editor.locator("#sourcing-product-adjust-done").click();
    await expect(editor).toBeHidden();
}

function supplierPaymentTasks(page: Page, taskId: string) {
    expect(taskId).toBeTruthy();
    return workspaceTaskRows(page).and(workPage(page).locator(`tr[data-row-id=${JSON.stringify(taskId)}]`));
}

async function assertNoPaymentApproval(page: Page) {
    await expect(workPage(page).getByText("供应商付款单审批")).toHaveCount(0);
    await expect(workPage(page).getByText("SupplierPayment")).toHaveCount(0);
    await expect(workPage(page).getByRole("button", { name: "提交审批" })).toHaveCount(0);
    await expect(workPage(page).getByText("电子交付单审批")).toHaveCount(0);
    await expect(workPage(page).getByText("服务履约单审批")).toHaveCount(0);
    await expect(workPage(page).getByText("采购收货单审批")).toHaveCount(0);
}

async function assertPurchaseOrderPrepayFacts(page: Page, responsibility: PurchaseRef["responsibility"]) {
    await expect(workPage(page).getByText("付款条件")).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(workPage(page).getByText(PAYMENT_TERM_SUPPLIER).first()).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(workPage(page).getByText(responsibility, { exact: true }).first()).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    await expect(workPage(page).getByRole("button", { name: "过账" })).toHaveCount(0);
    await workPage(page).getByRole("tab", { name: "履约" }).click();
    await expect(workPage(page).getByText("履约进度")).toBeVisible({ timeout: UI_TIMEOUT });
}

async function readPurchaseOrders(page: Page, salesOrderId: string, salesOrderNo: string): Promise<PurchaseRef[]> {
    await gotoHeading(page, "/procurement/orders", "采购单");
    const search = workPage(page).locator("#procurement-orders-list-search");
    await search.fill(salesOrderNo);
    await search.press("Enter");
    await expect(workPage(page).getByText("2 条")).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(workPage(page).getByRole("table").getByText("草稿", { exact: true })).toHaveCount(0);
    await expect(workPage(page).getByText(PAYMENT_TERM_SUPPLIER).first()).toBeVisible({ timeout: UI_TIMEOUT });
    const rows = workPage(page).getByRole("table").getByRole("row").filter({ hasText: salesOrderNo });
    await expect(rows).toHaveCount(2, { timeout: UI_TIMEOUT });
    await expect(rows.getByRole("button", { name: /打开采购单/ })).toHaveCount(2, { timeout: UI_TIMEOUT });
    // 列表的打开按钮没有 href；同一采购账号只读定位本销售单的 ID，业务事实仍逐详情验收。
    const token = await page.evaluate(() => localStorage.getItem("erp.token"));
    expect(token, "采购登录 token 必须存在").toBeTruthy();
    const orders = await apiGet<{
        items: Array<{ id: string; sales_order_id: string; sales_order_no: string }>;
    }>(token!, "/admin/purchase-orders", { sales_order_id: salesOrderId, page: 1, page_size: 10 });
    const ids = orders.items.filter(row => row.sales_order_id === salesOrderId && row.sales_order_no === salesOrderNo)
        .map(row => row.id);
    expect(ids, "本销售单必须对应两张独立采购单").toHaveLength(2);
    expect(new Set(ids).size).toBe(2);
    const refs: PurchaseRef[] = [];
    for (const id of ids) {
        expect(id.length).toBeGreaterThan(0);
        await page.goto(`/procurement/orders/${id}`);
        await expect(workPage(page).getByText("采购单").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(workPage(page).getByText("已生效").first()).toBeVisible({ timeout: UI_TIMEOUT });
        const no = await readHeaderDocumentNumber(page)
        expect(no.length).toBeGreaterThan(0);
        await expect(workPage(page).getByText("入仓", { exact: true }).or(workPage(page).getByText("供应商直发", { exact: true })).filter({ visible: true }).first())
            .toBeVisible({ timeout: UI_TIMEOUT });
        const responsibility: PurchaseRef["responsibility"] =
            (await workPage(page).getByText("供应商直发", { exact: true }).filter({ visible: true }).count()) > 0 ? "供应商直发" : "入仓";
        await assertPurchaseOrderPrepayFacts(page, responsibility);
        refs.push({ id, no, responsibility });
    }
    const inbound = refs.filter((row) => row.responsibility === "入仓");
    const direct = refs.filter((row) => row.responsibility === "供应商直发");
    expect(inbound).toHaveLength(1);
    expect(direct).toHaveLength(1);
    return refs;
}

async function assertUnpaidFulfillmentReminder(page: Page, kind: "入库" | "代发") {
    const surface = await openFulfillmentWorkspaceForm(page);
    if (kind === "入库") await fillReceiptDraft(page);
    else await fillDirectDraft(page, `BLOCK${Date.now().toString().slice(-6)}`);
    await expect(surface.getByText("预付待完成", { exact: true }).first()).toBeVisible({ timeout: UI_TIMEOUT });
    const confirm = surface.locator("#fulfillment-operations-work-surface-confirm");
    await expect(confirm).toHaveText(kind === "入库" ? "确认入库" : "确认发货");
    await expect(confirm).toBeEnabled({ timeout: UI_TIMEOUT });
}

async function fillReceiptDraft(page: Page) {
    const qty = workPage(page).locator('[id^="fulfillment-operations-receipt-form-received-quantity-"]').first();
    if (await qty.count()) {
        const current = await qty.inputValue();
        if (!current || current === "0") await qty.fill(SALES_QTY);
    }
    const quality = workPage(page).locator('[id^="fulfillment-operations-receipt-form-quality-result-"]').first();
    if (await quality.count()) {
        await chooseOption(page, quality, "合格", "合格");
    }
}

async function fillDirectDraft(page: Page, trackingNo: string) {
    await fillFirstDeliveryParcel(page, trackingNo, "顺丰速运");
}

async function payPurchaseOrder(page: Page, purchaseNo: string): Promise<string> {
    const payment = await payOnlySupplierTask(page, purchaseNo);
    return payment.id;
}

async function assertApprovedUnpaidRequest(token: string, request: PurchasePaymentRequestResult) {
    const detail = await apiGet<PurchasePaymentRequestResult & {
        work_item_id: string | null;
        payments: Array<{ payment_id: string }>;
        current_lines: Array<{
            payable_account_id: string;
            current_settled_total: string;
            current_open_total: string;
            current_reserved_total: string;
            current_available_total: string;
        }>;
    }>(token, `/admin/purchase-payment-requests/${request.id}`);
    expect(detail.status, "只有申请批准，尚未登记实际付款").toBe("approved");
    expect(detail.work_item_id, "申请批准已产生独立出纳任务").toBeTruthy();
    expect(detail.payments, "财审不得产生实际付款记录").toHaveLength(0);
    expect(detail.lines).toHaveLength(1);
    expect(detail.current_lines).toHaveLength(detail.lines.length);
    for (const line of detail.lines) {
        expect(line.paid_amount).toMatch(/^0(?:\.0+)?$/);
        expect(line.closed_amount).toMatch(/^0(?:\.0+)?$/);
        const current = detail.current_lines.find((item) => item.payable_account_id === line.payable_account_id);
        expect(current, "批准行必须保留对应的当前应付事实").toBeTruthy();
        expect(current!.current_settled_total, "申请批准不得核销采购应付").toMatch(/^0(?:\.0+)?$/);
        expect(current!.current_open_total).toBe(line.requested_amount);
        expect(current!.current_reserved_total, "批准未付款继续全额占用").toBe(line.requested_amount);
        expect(current!.current_available_total).toMatch(/^0(?:\.0+)?$/);
    }
}

async function assertCurrentDirectSource(
    page: Page,
    purchase: PurchaseRef,
    salesOrderId: string,
) {
    const workItemId = workspaceTaskId(page);
    expect(workItemId, "代发表单必须绑定当前工作台任务").toBeTruthy();
    const dialog = await openFulfillmentWorkspaceForm(page);
    const packing = dialog.getByRole("region", { name: "按地址登记发货包裹", exact: true });
    await expect(packing.getByText(new RegExp(SKU_DIRECT)).first()).toBeVisible();
    const token = await page.evaluate(() => localStorage.getItem("erp.token"));
    expect(token).toBeTruthy();
    const task = await apiGet<{
        id: string; work_item_type: string; business_object_type: string; business_object_id: string;
    }>(token!, `/admin/work-items/${workItemId}`);
    expect(task.id).toBe(workItemId);
    expect(task.work_item_type).toBe("FULFILLMENT_OPERATION");
    expect(task.business_object_type).toBe("delivery");
    const queue = await apiGet<{
        total: number;
        items: Array<{
            work_item_id: string; operation_id: string; operation_type: string;
            purchase_order_id: string;
            sales_order_id: string;
            gate_state: string; gate_effective_paid_amount?: string;
        }>;
    }>(token!, "/admin/work-items/fulfillment-queue", {
        operation_id: task.business_object_id,
        operation_types: "SUPPLIER_DIRECT",
        page: 1,
        page_size: 1,
        timezone: "Asia/Shanghai",
    });
    expect(queue.total).toBe(1);
    expect(queue.items).toHaveLength(1);
    const [operation] = queue.items;
    expect(operation.work_item_id).toBe(workItemId);
    expect(operation.operation_id).toBe(task.business_object_id);
    expect(operation.operation_type).toBe("SUPPLIER_DIRECT");
    expect(operation.purchase_order_id).toBe(purchase.id);
    expect(operation.sales_order_id).toBe(salesOrderId);
    expect(operation.gate_state, "批准申请不得清除本采购的先款提醒").toBe("BLOCKED");
    if (operation.gate_effective_paid_amount != null) expect(operation.gate_effective_paid_amount).toMatch(/^0(?:\.0+)?$/);
}

async function assertPaidFulfillmentReady(page: Page) {
    const surface = await openFulfillmentWorkspaceForm(page);
    await expect(surface.getByText("预付待完成", { exact: true })).toHaveCount(0);
    await expect(surface.locator("#fulfillment-operations-work-surface-confirm")).toBeEnabled({ timeout: UI_TIMEOUT });
}

async function postCurrentDelivery(
    page: Page,
    type: "SUPPLIER_DIRECT" | "WAREHOUSE_SHIP",
    salesOrderId: string,
    purchaseOrderId: string | null,
    trackingNo: string,
) {
    const workItemId = workspaceTaskId(page);
    expect(workItemId).toBeTruthy();
    const token = await page.evaluate(() => localStorage.getItem("erp.token"));
    expect(token).toBeTruthy();
    const task = await apiGet<{ id: string; business_object_type: string; business_object_id: string }>(
        token!, `/admin/work-items/${workItemId}`,
    );
    expect(task.id).toBe(workItemId);
    expect(task.business_object_type).toBe("delivery");
    const dialog = await openFulfillmentWorkspaceForm(page);
    const confirm = dialog.locator("#fulfillment-operations-work-surface-confirm");
    await expect(confirm).toBeEnabled();
    const posted = page.waitForResponse(response =>
        response.request().method() === "POST" &&
        new URL(response.url()).pathname === `/admin/deliveries/${task.business_object_id}/post`,
        { timeout: UI_TIMEOUT },
    );
    await confirm.click();
    const response = await posted;
    expect(response.ok(), await response.text()).toBe(true);
    const delivery = (await response.json()).data;
    expect(delivery).toMatchObject({
        id: task.business_object_id,
        delivery_type: type,
        sales_order_id: salesOrderId,
        purchase_order_id: purchaseOrderId,
        status: "SHIPPED",
    });
    const detail = await apiGet<{ packing: { packages: unknown[] }; delivery: { status: string } }>(
        token!, `/admin/deliveries/${task.business_object_id}`,
    );
    expect(detail.delivery.status).toBe("SHIPPED");
    expect(detail.packing.packages).toEqual([
        expect.objectContaining({ tracking_no: trackingNo, carrier: "顺丰速运" }),
    ]);
    expect(await apiGet(token!, `/admin/work-items/${workItemId}`)).toMatchObject({ status: "COMPLETED" });
}

test("flow-13 预付款提醒随实际付款更新，人工入库与代发保持可办理", async ({ browser }) => {
    test.setTimeout(FLOW_TIMEOUT);
    const stamp = Date.now().toString(36).toUpperCase();
    const customerName = `E2E先款客户${stamp}`;
    const contractNo = `HT-E2E-PP-${stamp}`;
    const dueDate = plusDaysIso(21);
    const trackingNo = `SF${stamp.slice(-8)}`;
    let session: Session | undefined;
    let salesOrderId = "";
    let salesOrderNo = "";
    let inboundPo = "";
    let directPo = "";
    let inboundPaymentId = "";
    let directPaymentId = "";

    const switchTo = async (login: LoginName) => {
        await session?.context.close();
        session = await openLoggedInWorkspace(browser, login);
        return session.page;
    };

    const openFulfillmentTask = async (page: Page, operation: "RECEIPT" | "SUPPLIER_DIRECT" | "WAREHOUSE_SHIP", purchase?: PurchaseRef) => {
        const token = await page.evaluate(() => localStorage.getItem("erp.token"));
        expect(token).toBeTruthy();
        const { total, items } = await apiGet<{
            total: number;
            items: Array<{ work_item_id: string; operation_type: string; sales_order_id: string; purchase_order_id?: string }>;
        }>(token!, "/admin/work-items/fulfillment-queue", {
            sales_order_id: salesOrderId, purchase_order_id: purchase?.id, operation_types: operation, page: 1, page_size: 10,
        });
        expect(total).toBe(1);
        expect(items).toHaveLength(1);
        expect(items[0]).toMatchObject({ sales_order_id: salesOrderId, operation_type: operation });
        if (purchase) expect(items[0].purchase_order_id).toBe(purchase.id);
        const taskId = items[0].work_item_id;
        await page.goto("/workspace/tasks");
        await selectWorkspaceFamily(page, "fulfillment");
        await openWorkspaceTaskRow(page, taskId);
        await openFulfillmentWorkspaceForm(page);
    };

    try {
        // 0) 采购责任默认调度人
        let page = await switchTo("admin");
        await ensureDefaultProcurementOwner(page);
        await ensureWarehouseSalesSourceAccess();

        // 1) 销售：客户（货到付款，对照供应商先款）
        page = await switchTo("xiaoshou");
        await createCustomerViaUi(page, {
            legalName: customerName,
            shortName: `先款${stamp}`,
            creditCode: uniqueCreditCode(stamp),
            paymentTermLabel: PAYMENT_TERM_CUSTOMER,
        });
        await workPage(page).locator("#customers-directory-search").fill(`先款${stamp}`);
        await workPage(page).locator("#customers-directory-search").press("Enter");
        await expect(workPage(page).getByRole("link", { name: `先款${stamp}`, exact: true })).toBeVisible({ timeout: UI_TIMEOUT });

        // 2) 上传合同 PDF
        await gotoHeading(page, "/sales/contracts", /^合同$/);
        await workPage(page).locator("#page-actions-action-upload").click();
        await archiveContractViaUi(page, { contractNo, customerName, pdf: contractPdf() })
        await expect(workPage(page).getByText(contractNo).first()).toBeVisible({ timeout: UI_TIMEOUT });

        // 3) 销售单：龙井入仓 + 普洱直发，客户付款条件仍为货到
        await page.goto("/sales/orders?mode=create");
        await expect(workPage(page).getByRole("heading", { name: /新建销售单|业务信息/ })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await selectExistingContractForSalesOrder(page, contractNo);
        await expect(workPage(page).getByLabel("供应商")).toHaveCount(0);
        await expect(workPage(page).getByLabel("履约责任")).toHaveCount(0);
        await expect(workPage(page).locator("#sales-orders-create-customer")).toContainText(new RegExp(customerName))
        await chooseOption(
            page,
            workPage(page).locator("#sales-orders-create-header-welfare-scene"),
            "礼包",
            "礼包",
        );
        await expect(workPage(page).locator("#sales-orders-create-header-payment-terms")).toContainText(PAYMENT_TERM_CUSTOMER)
        await expect(workPage(page).locator("#sales-orders-create-header-payment-terms").locator("input, button, [role=combobox]")).toHaveCount(0)
        await pickSku(page, "龙井", SKU_INBOUND);
        await pickSku(page, "普洱", SKU_DIRECT);
        await fillAllLineQuantities(page, SALES_QTY);
        await fillSalesLineUnitPrice(page, "1288.00", 0);
        await fillSalesLineUnitPrice(page, "568.00", 1);
        await setBeijingShippingAddress(page)
        await workPage(page).locator("#sales-orders-create-batch-due-date-open").click()
        await pickCalendarDay(page, workPage(page).locator("#sales-orders-create-batch-due-date"), dueDate);
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
        await expect(workPage(page).getByText(/审批中|审核中/).first()).toBeVisible({ timeout: UI_TIMEOUT });
        salesOrderNo = await readHeaderDocumentNumber(page)
        expect(salesOrderNo).toBeTruthy();
        await expect(workPage(page).locator("#sales-order-follow-up-delivery").locator("..")).toContainText("待采购", {
            timeout: UI_TIMEOUT,
        });

        // 4) 负向：销售单未生效不得建采购单、不得履约
        page = await switchTo("caigou");
        await gotoHeading(page, "/procurement/orders", "采购单");
        const poSearch = workPage(page).locator("#procurement-orders-list-search");
        await poSearch.fill(salesOrderNo);
        await poSearch.press("Enter");
        await expect(workPage(page).getByText(/0 条|当前没有/)).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(
            workPage(page).locator("#procurement-orders-list-table").getByText(salesOrderNo),
        ).toHaveCount(0);

        await openWorkspaceTask(page, /销售单审批/, salesOrderNo, "approval");
        await expect(workPage(page).locator("#procurement-orders-create-preview")).toHaveCount(0);
        await expect(workPage(page).getByRole("region", { name: "供给商品列表", exact: true })).toHaveCount(0);
        await expect(workPage(page).getByRole("dialog", { name: /^供给分配 · / })).toHaveCount(0);
        await approveCurrentDocument(page);

        // 5) 供给分配：付款条件先款 50%；龙井入仓、普洱直发；立即提交两张采购单
        await openWorkspaceTask(page, /待安排采购/, salesOrderNo, "procurement");
        const sourcingTask = workPage(page).getByRole("region", { name: "当前供给分配任务", exact: true });
        await expect(sourcingTask.getByRole("region", { name: "供给商品列表", exact: true }))
            .toBeVisible({ timeout: UI_TIMEOUT });
        await chooseSourcing(page, SKU_INBOUND, INBOUND_OPTION, WAREHOUSE_NAME);
        await chooseSourcing(page, SKU_DIRECT, DIRECT_OPTION);
        const sourcingFooter = workPage(page).getByRole("region", { name: "当前工作台任务", exact: true })
            .locator('[data-slot="workspace-task-footer"]:visible');
        await expect(sourcingFooter).toHaveCount(1);
        const visibleSourcingCounts = sourcingFooter.locator("dl:visible").or(
            sourcingFooter.locator("p:visible").filter({ hasText: /^将创建采购单\s/ }),
        );
        await expect(visibleSourcingCounts).toHaveCount(1);
        // 桌面 dl 的 dt/dd textContent 无分隔空白；窄屏摘要保留空格。
        await expect(visibleSourcingCounts).toContainText(/将创建采购单\s*2 张/);
        await expect(visibleSourcingCounts).toContainText(/(?:将建立)?库存预留\s*0 条/);
        await workPage(page).locator("#procurement-orders-create-preview").click();
        const preview = workPage(page).getByRole("dialog", { name: "预览供给分配" });
        await expect(preview).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(
            preview.getByText(/本次不占用现有库存|将为供给缺口创建|张采购单提交审批/),
        ).toBeVisible();
        await expect(preview.getByText("现有库存分配")).toHaveCount(0);
        await expect(preview.getByText(PAYMENT_TERM_SUPPLIER).first()).toBeVisible();
        const previewChoices = preview.getByRole("navigation", { name: "将创建的采购单" }).getByRole("button");
        await expect(previewChoices).toHaveCount(2);
        await previewChoices.filter({ hasText: WAREHOUSE_CODE }).click();
        await expect(preview.getByText("集采入仓", { exact: true })).toBeVisible();
        await previewChoices.filter({ hasNotText: WAREHOUSE_CODE }).click();
        await expect(preview.getByText("代发直发", { exact: true })).toBeVisible();
        const allocation = await confirmSupplyAllocation(page, /供给分配已完成|已将缺口拆成 2 张采购单并提交审批/);
        const purchaseNos = allocation.orders.map((order) => order.purchase_no);
        expect(purchaseNos).toHaveLength(2);
        expect(new Set(purchaseNos).size).toBe(2);
        for (const purchaseNo of purchaseNos) expect(purchaseNo).toBeTruthy();

        await page.goto("/workspace/tasks");
        await selectWorkspaceScope(page, "managed");
        await assertNoPaymentApproval(page);
        await gotoHeading(page, "/workspace/approvals", "我发起的审批");
        await expectWorkspaceQueue(page);
        for (const purchaseNo of purchaseNos) {
            await expect(matchingWorkspaceTaskRows(page, "采购单审批", purchaseNo)).toHaveCount(1);
        }
        await assertNoPaymentApproval(page);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(workPage(page).getByText("已生效").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(workPage(page).locator("#sales-order-follow-up-delivery").locator("..")).toContainText("采购已覆盖", {
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).getByRole("table").getByText("草稿", { exact: true })).toHaveCount(0);

        // 6) 采购部门审批两张采购单生效，形成应付；履约仍被先款拦住
        page = await switchTo("caigoushenpi");
        await approvePurchaseTasks(page, purchaseNos);
        await page.goto("/workspace/tasks");
        await expectWorkspaceQueue(page);
        await expect(matchingWorkspaceTaskRows(page, "待执行采购付款")).toHaveCount(0);
        await assertNoPaymentApproval(page);

        page = await switchTo("caigou");
        const purchases = await readPurchaseOrders(page, salesOrderId, salesOrderNo);
        const inboundPurchase = purchases.find((row) => row.responsibility === "入仓")!;
        const directPurchase = purchases.find((row) => row.responsibility === "供应商直发")!;
        inboundPo = inboundPurchase.no;
        directPo = directPurchase.no;

        // PPR-13：真实批准两张付款申请，仅建立占用和出纳任务，不登记实际付款。
        const inboundRequest = await authorizePurchasePaymentRequest(page, inboundPo);
        const directRequest = await authorizePurchasePaymentRequest(page, directPo);
        const procurementToken = await apiToken("caigou");
        await assertApprovedUnpaidRequest(procurementToken, inboundRequest);
        await assertApprovedUnpaidRequest(procurementToken, directRequest);

        // 7) 两张申请已批准但实际付款为零：入库 / 代发显示预付提醒且允许确认；电子交付与服务任务不出现
        page = await switchTo("cangchu");
        await page.goto("/workspace/tasks");
        await expect(workPage(page).getByRole("heading", { name: "待我处理" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await selectWorkspaceFamily(page, "fulfillment");
        await expect(workPage(page).getByRole("button", { name: /电子交付|线下服务/ })).toHaveCount(0);
        await openFulfillmentTask(page, "RECEIPT", inboundPurchase);
        await expect(workPage(page).getByText(inboundPo).first()).toBeVisible();
        await assertUnpaidFulfillmentReminder(page, "入库");

        page = await switchTo("caigou");
        await page.goto("/workspace/tasks");
        await expect(workPage(page).getByRole("heading", { name: "待我处理" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await selectWorkspaceFamily(page, "fulfillment");
        await expect(workPage(page).getByRole("button", { name: /电子交付|线下服务/ })).toHaveCount(0);
        await openFulfillmentTask(page, "SUPPLIER_DIRECT", directPurchase);
        // 使用同一工作项关联的销售单与采购单 ID 核验发货来源。
        await assertCurrentDirectSource(
            page, purchases.find((row) => row.responsibility === "供应商直发")!, salesOrderId,
        );
        await assertUnpaidFulfillmentReminder(page, "代发");

        // 8) 出纳先付清入仓采购单：仅入库预付提醒消失，代发仍提示
        page = await switchTo("fukuan");
        await page.goto("/workspace/tasks");
        await assertNoPaymentApproval(page);
        inboundPaymentId = await payPurchaseOrder(page, inboundPo);
        await page.goto("/workspace/tasks");
        await expect(workPage(page).getByRole("heading", { name: "待我处理" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await selectWorkspaceFamily(page, "finance");
        await expect(supplierPaymentTasks(page, inboundRequest.work_item_id!)).toHaveCount(0, {
            timeout: UI_TIMEOUT,
        });

        page = await switchTo("cangchu");
        await openFulfillmentTask(page, "RECEIPT", inboundPurchase);
        await expect(workPage(page).locator('[aria-label="入库表单"]')).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await fillReceiptDraft(page);
        await assertPaidFulfillmentReady(page);

        page = await switchTo("caigou");
        await openFulfillmentTask(page, "SUPPLIER_DIRECT", directPurchase);
        await expect(workPage(page).locator('#fulfillment-packing-form')).toBeVisible({ timeout: UI_TIMEOUT });
        await fillDirectDraft(page, trackingNo);
        await assertUnpaidFulfillmentReminder(page, "代发");

        // 9) 付清代发采购单后核对提醒消失再确认直发
        page = await switchTo("fukuan");
        directPaymentId = await payPurchaseOrder(page, directPo);
        await page.goto("/workspace/tasks");
        await expect(workPage(page).getByRole("heading", { name: "待我处理" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await selectWorkspaceFamily(page, "finance");
        // 全量会留下其他单据的付款待办。本流程只要求这两张采购单的付款任务已关闭。
        await expect(supplierPaymentTasks(page, directRequest.work_item_id!)).toHaveCount(0, {
            timeout: UI_TIMEOUT,
        });
        await expect(supplierPaymentTasks(page, inboundRequest.work_item_id!)).toHaveCount(0, {
            timeout: UI_TIMEOUT,
        });
        await assertNoPaymentApproval(page);

        page = await switchTo("caigou");
        await openFulfillmentTask(page, "SUPPLIER_DIRECT", directPurchase);
        await expect(workPage(page).locator('#fulfillment-packing-form')).toBeVisible({ timeout: UI_TIMEOUT });
        const workItemId = workspaceTaskId(page);
        expect(workItemId).toBeTruthy();
        const token = await apiToken("caigou");
        const receiptPath = `/admin/work-items/${workItemId}/payment-receipts`;
        const receipts = await apiGet<Array<{
            document_id: string; document_no: string; file_asset_id: string;
        }>>(token, receiptPath);
        expect(receipts.map(receipt => receipt.document_id)).toEqual([directPaymentId]);
        const wrongPurchaseReceipt = await page.request.get(
            `${API_BASE}${receiptPath}/${inboundPaymentId}/download`,
            { headers: { Authorization: `Bearer ${token}` } },
        );
        expect(wrongPurchaseReceipt.status(), await wrongPurchaseReceipt.text()).toBe(404);

        const payable = workPage(page).locator("#fulfillment-operations-gate-view-payment");
        await expect(payable).toHaveAttribute("href", `/procurement/orders/${directPurchase.id}?section=payable`);
        const [purchasePage] = await Promise.all([page.waitForEvent("popup"), payable.click()]);
        await purchasePage.waitForURL(`**/procurement/orders/${directPurchase.id}?section=payable`);
        try {
            const paymentList = workPage(purchasePage).getByRole("region", { name: "财务付款单", exact: true });
            await expect(paymentList.getByRole("listitem")).toHaveCount(1);
            await expect(paymentList.getByText(receipts[0].document_no, { exact: true })).toBeVisible();
            const [download, response] = await Promise.all([
                purchasePage.waitForEvent("download"),
                purchasePage.waitForResponse(result => new URL(result.url()).pathname ===
                    `/admin/purchase-orders/${directPurchase.id}/payments/${directPaymentId}/receipt`),
                paymentList.getByRole("button", { name: "下载", exact: true }).click(),
            ]);
            expect(response.ok()).toBe(true);
            expect(download.suggestedFilename()).toBe(BANK_RECEIPT_FILE.name);
            expect(await download.failure()).toBeNull();
            expect(fs.readFileSync((await download.path())!)).toEqual(BANK_RECEIPT_FILE.buffer);
        } finally {
            await purchasePage.close();
        }
        expect(workspaceTaskId(page)).toBe(workItemId);
        await fillDirectDraft(page, trackingNo);
        await assertPaidFulfillmentReady(page);
        await postCurrentDelivery(
            page, "SUPPLIER_DIRECT", salesOrderId,
            purchases.find((row) => row.responsibility === "供应商直发")!.id, trackingNo,
        );

        // 10) 入库确认后仓发
        page = await switchTo("cangchu");
        await openFulfillmentTask(page, "RECEIPT", inboundPurchase);
        await expect(workPage(page).locator('[aria-label="入库表单"]')).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await fillReceiptDraft(page);
        await assertPaidFulfillmentReady(page);
        await workPage(page).locator("#fulfillment-operations-work-surface-confirm").click();
        await confirmReceipt(page);

        await openFulfillmentTask(page, "WAREHOUSE_SHIP");
        await fillFirstDeliveryParcel(page, `WH${trackingNo}`, "顺丰速运");
        await postCurrentDelivery(page, "WAREHOUSE_SHIP", salesOrderId, null, `WH${trackingNo}`);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(workPage(page).getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).getByText("已生效").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(workPage(page).getByRole("button", { name: "过账" })).toHaveCount(0);
    } finally {
        await session?.context.close();
    }
});
