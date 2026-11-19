/**
 * 流程: [flow-15] 客户拒收/退货全链（含采购退货、退款与红票）
 * 文档: docs/erp-phase-1.md §6.2（验收短少/拒收）+ §6.5.3（退货与拒收时序）+ §6.5.4（资金与发票纠错）
 * 账号: admin / xiaoshou / caigou / cangchu / caiwu / fukuan / kaipiao / lisiyong
 *
 * 文档-代码差异（以代码为准）:
 * 1. 文档 6.5.3：销售创建销售退货/拒收处理单 → 仓储验收退回 → 采购建采购退货并出库 → 销售确认完成。
 *    代码：验收弹窗可记短少/拒收，但「只记客户结果，不会自动退货、退款或改应收」；
 *    整件拒收必须「本次不验」后另开退货。前端无创建 SalesReturnCase 入口
 *    （仅有未挂载的 SalesReturnCaseFacts），后端 POST /admin/sales-return-cases
 *    只写草稿，无 submit/post/complete 命令。
 * 2. 文档：仓储验收退回商品、按采购退货单出库。代码：库存/履约页无退货入库/出库作业面。
 * 3. 文档：采购创建采购退货单。代码：采购单「变更与异常」只读展示关联退货，文案「暂无采购退货」，
 *    无创建/执行按钮；后端 POST /admin/purchase-return-orders 同样只写草稿。
 * 4. 文档：直退供应商由采购登记退回结果、不经公司仓库。代码：无该登记 UI。
 * 5. 文档：销售确认退货、资金和发票处理完成。代码：无「确认处理完成」按钮或命令。
 * 6. 文档 6.5.4：财务经办创建退款。代码：必须 fukuan 提交 CustomerRefund（lisiyong→caiwu）
 *    与 SupplierRefund（caigou→caiwu）；禁止 caiwu 自己提交。
 * 7. 文档：红票走 Invoice 强类型命令、NO_APPROVAL。代码：kaipiao 在客户往来预览点「红票」；
 *    进项红票在供应商往来发票行；均不出现审批实例。
 * 8. 销售单主状态页文案可能是「审核中」而非文档「审批中」；开票完成态是「已开齐」不是「已完成」；
 *    退款入账徽标是「已过账」，按钮不用「过账」。
 * 9. 验收确认层使用当前「确认客户验收」入口。
 * 10. 供给分配确认同一事务创建并立即提交采购单，不得留下未提交草稿。
 */
import { workPage } from "../helpers/workspace-page"
import { setBeijingShippingAddress } from "../helpers/sales-orders"
import { archiveContractViaUi } from "../helpers/contracts";
import { expectSalesOrderContractSummary, fillSalesLineUnitPrice, selectExistingContractForSalesOrder } from "../helpers/sales-orders"
import { payOnlySupplierTask } from "../helpers/payments";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
    test,
    expect,
    type BrowserContext,
    type Page,
} from "../helpers/test";

import { createCustomerViaUi } from "../helpers/customers";
import { apiGet, apiToken } from "../helpers/api";
import { fillFirstDeliveryParcel, openFulfillmentTaskForSource, uploadAcceptanceEvidence } from "../helpers/fulfillment";
import { invoiceEvidenceFiles, registerDetailedSalesInvoice, submitSalesInvoiceRequest } from "../helpers/invoices";
import { openLoggedInWorkspace } from "../helpers/login";
import {
    ensureDefaultProcurementOwner,
    submitCreatedSalesOrder,
} from "../helpers/procurement";
import {
    expectReceiptPreview,
    registerCustomerReceiptForOrder,
    submitCustomerRefundRequest,
} from "../helpers/receipts";
import { confirmSupplyAllocation, expandSourcingEditor } from "../helpers/sourcing";
import {
    workspaceTaskId,
    approveCurrentDocument,
    chooseOption,
    expectToast,
    openFulfillmentWorkspaceForm,
    openWorkspaceTask,
    optionalStepVisible,
    pickCalendarDay,
    readHeaderDocumentNumber,
} from "../helpers/ui";

test.describe.configure({ mode: "serial" });

const UI_TIMEOUT = 20_000;
const FLOW_TIMEOUT = 15 * 60 * 1000;
const SKU_KEYWORD = "龙井";
const SKU_NAME = "狮峰明前龙井礼盒";
const SUPPLIER_SHORT = "狮峰茶叶";
const DIRECT_OPTION = /杭州狮峰茶叶有限公司.* · 代发直发/;
const SALES_QTY = "2";
const REJECT_QTY = "1";
const FACT_ONLY_NOTICE =
    "短少、拒收或服务不通过只记客户结果，不会自动退货、退款或改应收。请另开退货或拒收处理单。";

const PNG_1X1 = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    "base64",
);

const specDir = path.dirname(fileURLToPath(import.meta.url));
const CONTRACT_PDF_CANDIDATES = [
    path.resolve(specDir, "../fixtures/sample-contract.pdf"),
    path.resolve(process.cwd(), "fixtures/sample-contract.pdf"),
];
const CONTRACT_PDF =
    CONTRACT_PDF_CANDIDATES.find((candidate) => existsSync(candidate)) ??
    CONTRACT_PDF_CANDIDATES[0]!;

type LoginName =
    | "xiaoshou"
    | "lisiyong"
    | "caigou"
    | "cangchu"
    | "caiwu" | "caigoushenpi"
    | "fukuan"
    | "kaipiao"
    | "admin";

type Session = { context: BrowserContext; page: Page };

function plusDaysIso(days: number): string {
    const date = new Date();
    date.setDate(date.getDate() + days);
    const pad = (value: number) => String(value).padStart(2, "0");
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function uniqueCreditCode(stamp: string): string {
    const raw = `91${stamp.replace(/[^0-9A-Za-z]/g, "").toUpperCase()}E2ERETURN`;
    return raw.slice(0, 18).padEnd(18, "0");
}

function orderTitleRow(page: Page, customerName: string) {
    return workPage(page).getByRole("heading", { name: customerName }).locator("xpath=..");
}

async function factValue(page: Page, label: string) {
    const dt = workPage(page).locator('[data-slot="formal-action-result"] dt', { hasText: label });
    await expect(dt.first()).toBeVisible({ timeout: UI_TIMEOUT });
    return (await dt.first().locator("xpath=following-sibling::dd[1]").innerText()).trim();
}

async function assertNoReturnApprovalTasks(page: Page) {
    await page.goto("/workspace/tasks");
    await expect(workPage(page).getByRole("heading", { name: "待我处理" })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    await expect(workPage(page).getByRole("button", { name: /销售退货单审批/ })).toHaveCount(0);
    await expect(workPage(page).getByRole("button", { name: /采购退货单审批/ })).toHaveCount(0);
    await expect(workPage(page).getByRole("button", { name: /发票审批/ })).toHaveCount(0);
    await expect(workPage(page).getByRole("button", { name: /客户验收单审批/ })).toHaveCount(0);
}

async function assertNoReturnCreateUi(page: Page) {
    await expect(workPage(page).getByRole("button", { name: /创建退货|新建退货|登记退货|退货处理单/ })).toHaveCount(
        0,
    );
    await expect(workPage(page).getByRole("button", { name: /创建采购退货|新建采购退货/ })).toHaveCount(0);
    await expect(workPage(page).getByRole("button", { name: /确认处理完成|确认退货处理完成/ })).toHaveCount(0);
    await expect(workPage(page).getByRole("button", { name: "选择流程" })).toHaveCount(0);
    await expect(workPage(page).getByRole("button", { name: "过账" })).toHaveCount(0);
}

test("flow-15 客户拒收后走直退供应商、退款与红票纠正", async ({ browser }) => {
    test.setTimeout(FLOW_TIMEOUT);
    const stamp = Date.now().toString(36).toUpperCase();
    const customerName = `E2E拒收客户${stamp}`;
    const creditCode = uniqueCreditCode(stamp);
    const contractNo = `HT-E2E-RT-${stamp}`;
    const dueDate = plusDaysIso(21);
    const trackingNo = `SF${stamp.slice(-8)}`;
    let session: Session | undefined;
    let salesOrderId = "";
    let salesOrderNo = "";
    let purchaseOrderNo = "";
    let receiptNo = "";
    let salesInvoiceNo = "";
    let customerRefundNo = "";
    let supplierRefundNo = "";
    let purchaseInvoiceNo = `PINV${stamp}`;

    const switchTo = async (login: LoginName) => {
        await session?.context.close();
        session = await openLoggedInWorkspace(browser, login);
        return session.page;
    };

    try {
        // 0) 采购责任默认调度人
        let page = await switchTo("admin");
        await ensureDefaultProcurementOwner(page);

        // 1) W03 客户
        page = await switchTo("xiaoshou");
        await createCustomerViaUi(page, {
            legalName: customerName,
            shortName: `拒收${stamp}`,
            creditCode,
            paymentTermLabel: "货到 15 天",
            contact: { name: "拒收联系人", phone: "13800138015" },
            address: "北京市朝阳区拒收路 15 号",
        });

        // 2) W04 合同 PDF
        await page.goto("/sales/contracts");
        await expect(
            workPage(page).getByRole("heading", { name: "合同", exact: true }),
        ).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await workPage(page).locator("#page-actions-action-upload").click();
        await archiveContractViaUi(page, { contractNo, customerName, pdf: CONTRACT_PDF })
        await expect(workPage(page).getByText(contractNo).first()).toBeVisible({ timeout: UI_TIMEOUT });

        // 3) W05 销售单：数量 2，便于部分拒收仍能记入验收
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
        await expectSalesOrderContractSummary(page, { customerName, paymentTerms: "货到 15 天", contractNo });
        await chooseOption(page, workPage(page).locator("#sales-orders-create-header-welfare-scene"), "礼包");
        await workPage(page).locator("#sales-orders-create-line-items-add").click();
        const skuDialog = workPage(page).getByRole("dialog", { name: "添加商品" });
        await expect(skuDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await skuDialog.locator("#sales-orders-sku-picker-filter-search").fill(SKU_KEYWORD);
        await skuDialog.locator("#sales-orders-sku-picker-filter-search").press("Enter");
        const skuRow = skuDialog.getByRole("checkbox", { name: new RegExp(`选择.*${SKU_NAME}`) });
        await expect(skuRow.first()).toBeVisible({ timeout: UI_TIMEOUT });
        await skuRow.first().check();
        await skuDialog.locator("#sales-orders-sku-picker-confirm").click();
        await expect(skuDialog).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(workPage(page).getByText(SKU_NAME).first()).toBeVisible({ timeout: UI_TIMEOUT });
        await workPage(page).getByLabel("数量").fill(SALES_QTY);
        await fillSalesLineUnitPrice(page, "1288.00");
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
        await expect(orderTitleRow(page, customerName).getByText(/审批中|审核中/)).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        salesOrderNo = await readHeaderDocumentNumber(page)
        expect(salesOrderNo).toBeTruthy();
        await expect(workPage(page).locator("#sales-orders-detail-start-change")).toBeDisabled();
        await expect(workPage(page).locator("#sales-order-follow-up-delivery").locator("..")).toContainText("待采购", {
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).getByText("改单中")).toHaveCount(0);

        // 4) 负向：生效前不得建采购单、不得履约
        page = await switchTo("caigou");
        await page.goto("/procurement/orders");
        await expect(workPage(page).getByRole("heading", { name: "采购单", exact: true })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).getByText(/共 \d+ 张采购单/)).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(
            workPage(page)
                .locator("#procurement-orders-list-table")
                .getByRole("row")
                .filter({ hasText: salesOrderNo }),
        ).toHaveCount(0);

        // 5) 采购确认：只通过，不选源
        await openWorkspaceTask(page, "销售单审批", salesOrderNo, "approval");
        await expect(workPage(page).getByRole("heading", { name: /销售单/ })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).locator("#procurement-orders-create-preview")).toHaveCount(0);
        await approveCurrentDocument(page);

        // 6) 供给分配：最短路径选供应商直发（对应文档直退供应商、不经公司仓）
        await workPage(page).getByRole("button", { name: "刷新" }).click().catch(() => undefined);
        await openWorkspaceTask(page, "待安排采购", salesOrderNo, "procurement");
        await expect(workPage(page).getByRole("heading", { name: "供给分配" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expandSourcingEditor(page);
        const sourcingOption = workPage(page)
            .locator('[id^="procurement-orders-create-row-"][id$="-sourcing-option"]')
            .first();
        await expect(sourcingOption).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(page, sourcingOption, DIRECT_OPTION, "代发直发");
        await expect(workPage(page).getByRole("combobox", { name: "仓库", exact: true })).toHaveCount(0);
        await workPage(page).locator("#sourcing-product-adjust-done").click();
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
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toHaveCount(0);

        // 7) 采购部门内部审批
        page = await switchTo("caigoushenpi");
        await openWorkspaceTask(page, "采购单审批", approvedPurchaseNo, "approval");
        await approveCurrentDocument(page);

        page = await switchTo("caigou");
        await page.goto("/procurement/orders");
        await expect(workPage(page).getByRole("heading", { name: "采购单", exact: true })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        const poSearch = workPage(page).locator("#procurement-orders-list-search");
        await poSearch.fill(salesOrderNo);
        await poSearch.press("Enter");
        const poRow = workPage(page)
            .locator("#procurement-orders-list-table")
            .getByRole("row")
            .filter({ hasText: salesOrderNo });
        await expect(poRow.getByText("供应商直发")).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(poRow.getByText("草稿")).toHaveCount(0);
        purchaseOrderNo = (
            (await poRow.getByRole("button", { name: /打开采购单/ }).textContent()) ?? ""
        ).trim();
        expect(purchaseOrderNo.length).toBeGreaterThan(0);

        // 8) 仓储不得出现入库履约（代发不经公司仓）
        page = await switchTo("cangchu");
        await page.goto("/workspace/tasks");
        await expect(workPage(page).getByRole("heading", { name: "待我处理" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).getByRole("button", { name: /入库/ })).toHaveCount(0);
        await expect(workPage(page).getByRole("button", { name: /退货/ })).toHaveCount(0);

        // 9) 采购按冻结的收货地址登记包裹；付款事实在后续退款前独立办理。
        page = await switchTo("caigou");
        await openFulfillmentTaskForSource(page, { salesOrderId, purchaseOrderId: allocation.orders[0]!.purchase_order_id, operation: "SUPPLIER_DIRECT" });
        const dialog = await openFulfillmentWorkspaceForm(page);
        await fillFirstDeliveryParcel(page, trackingNo, "顺丰速运");
        await expect(dialog.locator("#fulfillment-operations-work-surface-confirm")).toBeEnabled({
            timeout: UI_TIMEOUT,
        });
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
                sales_order_id: string; purchase_order_id: string }>;
        }>(token!, "/admin/work-items/fulfillment-queue", {
            operation_id: task.business_object_id, operation_types: "SUPPLIER_DIRECT",
            page: 1, page_size: 1, timezone: "Asia/Shanghai",
        });
        expect(source.total).toBe(1);
        expect(source.items).toHaveLength(1);
        expect(source.items[0]).toMatchObject({
            work_item_id: workItemId, operation_id: task.business_object_id, operation_type: "SUPPLIER_DIRECT",
            sales_order_id: salesOrderId, purchase_order_id: allocation.orders[0]!.purchase_order_id,
        });
        const shipped = page.waitForResponse(response => response.request().method() === "POST" &&
            new URL(response.url()).pathname === `/admin/deliveries/${task.business_object_id}/post`,
            { timeout: UI_TIMEOUT });
        await dialog.locator("#fulfillment-operations-work-surface-confirm").click();
        const shippedResponse = await shipped;
        expect(shippedResponse.ok(), await shippedResponse.text()).toBe(true);
        const delivery = (await shippedResponse.json()).data;
        expect(delivery).toMatchObject({
            id: task.business_object_id, status: "SHIPPED", delivery_type: "SUPPLIER_DIRECT",
            sales_order_id: salesOrderId, purchase_order_id: source.items[0].purchase_order_id,
        });
        expect(delivery.tracking_entries).toHaveLength(1);
        expect(delivery.tracking_entries[0]).toMatchObject({ tracking_no: trackingNo, carrier: "顺丰速运" });
        expect(shippedResponse.request().postDataJSON()).toMatchObject({
            packing: { packages: [{ tracking_no: trackingNo, carrier: "顺丰速运", items: [{ quantity: SALES_QTY }] }] },
        });
        expect(await apiGet(token!, `/admin/work-items/${workItemId}`)).toMatchObject({
            id: workItemId, business_object_id: task.business_object_id, status: "COMPLETED",
        });
        await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT });

        // 11) 销售登记部分拒收（1 件拒收 + 1 件通过，整件拒收不能记入验收）
        page = await switchTo("xiaoshou");
        await openWorkspaceTask(page, "客户验收登记", salesOrderNo, "fulfillment");
        await workPage(page).locator("#sales-orders-acceptance-register-open").click();
        const acceptanceDialog = workPage(page).getByRole("dialog", { name: "登记客户验收" });
        await expect(acceptanceDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(acceptanceDialog.getByRole("checkbox", { name: "纳入本次验收" })).toBeChecked();
        await expect(acceptanceDialog.getByRole("radio", { name: /^全部通过 / })).toBeChecked();
        const rejectToggle = acceptanceDialog.getByRole("radio", { name: /^有拒收 / });
        await expect(rejectToggle).toBeVisible({ timeout: UI_TIMEOUT });
        await rejectToggle.check();
        await expect(acceptanceDialog.getByLabel("拒收数量")).toBeVisible({ timeout: UI_TIMEOUT });
        await acceptanceDialog.getByLabel("拒收数量").fill(REJECT_QTY);
        await acceptanceDialog.getByPlaceholder("短少或拒收时必填").fill("外箱破损，客户拒收一件");
        await expect(acceptanceDialog.getByText(FACT_ONLY_NOTICE)).toBeVisible();
        await expect(acceptanceDialog.getByRole("button", { name: /创建退货|另开退货/ })).toHaveCount(0);
        await uploadAcceptanceEvidence(page);
        await acceptanceDialog.locator("#sales-orders-acceptance-register-submit").click();
        const acceptConfirm = workPage(page).getByRole("alertdialog", { name: "确认客户验收", exact: true });
        await expect(acceptConfirm).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(acceptConfirm).toContainText("拒收");
        const acceptConfirmBtn = acceptConfirm.locator("#sales-orders-acceptance-confirm-confirm");
        if (await acceptConfirmBtn.count()) {
            await acceptConfirmBtn.click();
        } else {
            await acceptConfirm.getByRole("button", { name: "确认验收" }).click();
        }
        await expect(acceptConfirm).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(workPage(page).getByText("客户验收已登记").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(workPage(page).getByText("拒收").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(workPage(page).getByText(/请另开退货或拒收处理单/).first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await assertNoReturnCreateUi(page);

        // 12) 销售单详情：未关闭、未开变更单；无退货处理单创建入口
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(workPage(page).getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("履约中", { exact: true })).toBeVisible();
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toHaveCount(0);
        await expect(workPage(page).getByText("改单中")).toHaveCount(0);
        await assertNoReturnCreateUi(page);
        await expect(workPage(page).getByText("选择流程")).toHaveCount(0);
        await expect(workPage(page).getByRole("button", { name: /更新审批流程版本|撤回审批|改派当前审批人/ })).toHaveCount(
            0,
        );

        page = await switchTo("caigou");
        await page.goto("/procurement/orders");
        await expect(workPage(page).getByRole("button", { name: /打开采购单/ }).first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await workPage(page).getByRole("button", { name: /打开采购单/ }).first().click();
        await expect(workPage(page).getByRole("tab", { name: "变更" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await workPage(page).getByRole("tab", { name: "变更" }).click();
        await expect(workPage(page).getByText("暂无采购退货。")).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(workPage(page).getByRole("button", { name: /创建采购退货|新建采购退货/ })).toHaveCount(0);
        await expect(workPage(page).getByRole("button", { name: "选择流程" })).toHaveCount(0);
        await expect(workPage(page).getByRole("button", { name: /^(通过|同意审批)$/ })).toHaveCount(0);

        page = await switchTo("cangchu");
        await page.goto("/workspace/tasks");
        await expect(workPage(page).getByRole("heading", { name: "待我处理" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).getByRole("button", { name: /退货验收|退货入库|退货出库|采购退货/ })).toHaveCount(
            0,
        );
        await assertNoReturnApprovalTasks(page);

        // 13) 出纳在客户往来登记回款并提交，财务总监审批入账（原回款事实，后续退款纠正）
        page = await switchTo("fukuan");
        const postedReceipt = await registerCustomerReceiptForOrder(page, {
            customerName,
            orderNo: salesOrderNo,
            amount: "2576.00",
            bankReference: `RC${stamp}`,
        });
        receiptNo = postedReceipt.receiptNo;

        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "回款复核", receiptNo, "approval");
        await approveCurrentDocument(page);

        // 14) 销售申请开票，财务审批后开票人 W01 登记销项蓝票（红票依据）
        page = await switchTo("xiaoshou");
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
        salesInvoiceNo = `INV${stamp}`;
        // 红冲必须基于已采集票面税额的蓝票；使用正式任务授权的完整登记接口。
        await registerDetailedSalesInvoice(page, {
            requestId: invoiceRequestId, orderNo: salesOrderNo, invoiceNo: salesInvoiceNo,
            amount: "2576.00", netAmount: "2279.65", taxAmount: "296.35",
        });
        await page.goto(`/finance/customer-accounts?view=sales_invoice&q=${encodeURIComponent(salesInvoiceNo)}`);
        await expect(workPage(page).getByRole("row").filter({ hasText: salesInvoiceNo })).toContainText("已登记", { timeout: UI_TIMEOUT });
        await expect(workPage(page).getByRole("button", { name: /选择流程|通过|驳回/ })).toHaveCount(0);

        // 15) 出纳按原回款发起客户退款 → 销售领导 → 财务总监
        page = await switchTo("fukuan");
        await page.goto("/finance/customer-accounts?view=receipt");
        await expect(workPage(page).getByRole("heading", { name: "客户往来" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await workPage(page).locator("#customer-receivables-view-receipt").click();
        await workPage(page).locator("#customer-receivables-toolbar-search").fill(receiptNo);
        await workPage(page).locator("#customer-receivables-toolbar-search").press("Enter");
        const receiptRow = workPage(page).getByRole("row").filter({ hasText: receiptNo });
        await expect(receiptRow).toBeVisible({ timeout: UI_TIMEOUT });
        await receiptRow.click();
        await expectReceiptPreview(page, receiptNo);
        await workPage(page).locator("#customer-receivables-preview-receipt-refund").click();
        const refundResult = await submitCustomerRefundRequest(
            page,
            "客户拒收，按原回款全额退款，原回款保留",
        );
        customerRefundNo = refundResult.refundNo;
        expect(customerRefundNo.length).toBeGreaterThan(2);

        page = await switchTo("lisiyong");
        await openWorkspaceTask(page, "客户退款审批", customerRefundNo || customerName, "approval");
        await approveCurrentDocument(page);

        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "客户退款审批", customerRefundNo || customerName, "approval");
        await approveCurrentDocument(page);

        page = await switchTo("fukuan");
        await page.goto("/finance/customer-accounts?view=receipt");
        await workPage(page).locator("#customer-receivables-view-receipt").click();
        await workPage(page).locator("#customer-receivables-toolbar-search").fill(receiptNo);
        await workPage(page).locator("#customer-receivables-toolbar-search").press("Enter");
        await expect(workPage(page).getByRole("row").filter({ hasText: receiptNo })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(
            workPage(page).getByRole("row").filter({ hasText: receiptNo }).getByText("已过账", { exact: true }),
        ).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(
            workPage(page).getByRole("row").filter({ hasText: receiptNo }).getByText("已冲正"),
        ).toHaveCount(0);

        // 16) 开票人登记销项红票：原蓝票保留
        page = await switchTo("kaipiao");
        await page.goto("/finance/customer-accounts?view=sales_invoice");
        await expect(workPage(page).getByRole("heading", { name: "客户往来" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await workPage(page).locator("#customer-receivables-view-sales_invoice").click();
        await workPage(page).locator("#customer-receivables-toolbar-search").fill(salesInvoiceNo);
        await workPage(page).locator("#customer-receivables-toolbar-search").press("Enter");
        const invoiceRow = workPage(page).getByRole("row").filter({ hasText: salesInvoiceNo });
        await expect(invoiceRow).toBeVisible({ timeout: UI_TIMEOUT });
        await invoiceRow.click();
        await workPage(page).locator("#customer-receivables-preview-invoice-red").click();
        const redDialog = workPage(page).getByRole("dialog", { name: "发起销项红票" });
        await expect(redDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(redDialog.getByText("红票表示冲减原票的分配。")).toBeVisible();
        const redAmount = redDialog.locator("#customer-receivables-reverse-amount");
        if (await redAmount.count()) {
            const current = await redAmount.inputValue();
            if (!current) {
                await redAmount.fill("1.00");
            }
        }
        await redDialog.locator("#customer-receivables-reverse-reason").fill("客户拒收，开具销项红票");
        const salesRedIssued = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/red-issue"));
        await redDialog.locator("#customer-receivables-reverse-confirm").click();
        const salesRedResponse = await salesRedIssued;
        expect(salesRedResponse.ok()).toBeTruthy();
        const salesRedResult = (await salesRedResponse.json()).data;
        expect(salesRedResult.invoice_kind).toBe("red");
        expect(salesRedResult.unallocated_amount).toMatch(/^0(?:\.0+)?$/);
        expect(salesRedResult.invoice_no).toBeTruthy();
        await expect(redDialog).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(workPage(page).getByRole("button", { name: /选择流程|通过|驳回/ })).toHaveCount(0);
        await page.goto("/finance/customer-accounts?view=sales_invoice");
        await workPage(page).locator("#customer-receivables-view-sales_invoice").click();
        await workPage(page).locator("#customer-receivables-toolbar-search").fill("");
        await workPage(page).locator("#customer-receivables-toolbar-search").press("Enter");
        await expect(workPage(page).getByRole("row").filter({ hasText: salesInvoiceNo }).getByText("蓝票")).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(workPage(page).getByRole("row").filter({ hasText: salesRedResult.invoice_no })).toContainText("红票", { timeout: UI_TIMEOUT });

        // 17) 出纳完成本采购单的供应商付款，形成原付款事实。
        page = await switchTo("fukuan");
        const supplierPayment = await payOnlySupplierTask(page, purchaseOrderNo);

        // 18) 开票岗登记进项蓝票（红票依据），与财审、付款职责分离
        page = await switchTo("kaipiao");
        await page.goto("/finance/supplier-accounts");
        await expect(workPage(page).getByRole("heading", { name: "供应商往来" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await workPage(page).locator("#supplier-payables-header-register-invoice").click();
        const pickSupplier = workPage(page).getByRole("dialog", { name: /选择供应商 · 登记进项发票/ });
        await expect(pickSupplier).toBeVisible({ timeout: UI_TIMEOUT });
        // 下拉回车会把仍打开的弹层交给对话框，确认按钮还没写上供应商就被拆掉。只点选项，再等确认可用。
        const supplierInput = pickSupplier.locator("#supplier-payables-pick-supplier-select");
        await expect(supplierInput).toBeVisible({ timeout: UI_TIMEOUT });
        await supplierInput.click();
        await supplierInput.fill(SUPPLIER_SHORT);
        const supplierOption = workPage(page)
            .locator('[id^="supplier-payables-pick-supplier-select-option-"]')
            .filter({ hasText: "杭州狮峰茶叶有限公司" });
        await expect(supplierOption.first()).toBeVisible({ timeout: UI_TIMEOUT });
        await supplierOption.first().click();
        const confirmSupplier = pickSupplier.locator("#supplier-payables-pick-supplier-confirm");
        await expect(confirmSupplier).toBeEnabled({ timeout: UI_TIMEOUT });
        await confirmSupplier.click();
        await expect(workPage(page).getByRole("heading", { name: "登记进项发票" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(workPage(page).getByRole("button", { name: "提交审批" })).toHaveCount(0);
        const allocationTable = workPage(page).getByRole("region", { name: "核销明细", exact: true });
        const purchaseAllocation = allocationTable.getByRole("row").filter({
            has: workPage(page).getByRole("cell", { name: purchaseOrderNo, exact: true }),
        });
        await expect(purchaseAllocation).toHaveCount(1);
        const allocationCheck = purchaseAllocation.getByRole("checkbox", { name: `选择 ${purchaseOrderNo}`, exact: true });
        if (!(await allocationCheck.isChecked())) await allocationCheck.click();
        await expect(allocationCheck).toBeChecked();
        await expect(allocationTable.locator('tbody [role="checkbox"][aria-checked="true"]')).toHaveCount(1);
        const allocatedInput = purchaseAllocation.getByRole("textbox", { name: `${purchaseOrderNo} 本次核销金额`, exact: true });
        await expect(allocatedInput).toBeEnabled();
        const allocated = await allocatedInput.inputValue();
        const grossCents = Math.round(Number(allocated) * 100);
        expect(grossCents).toBeGreaterThan(0);
        const netCents = Math.round(grossCents / 1.13);
        await workPage(page).locator("#supplier-payables-allocation-form-gross-amount").fill(allocated);
        await workPage(page).locator("#supplier-payables-allocation-form-net-amount").fill((netCents / 100).toFixed(2));
        await workPage(page).locator("#supplier-payables-allocation-form-tax-amount").fill(((grossCents - netCents) / 100).toFixed(2));
        await workPage(page).locator("#supplier-payables-allocation-form-invoice-no").fill(purchaseInvoiceNo);
        await workPage(page).locator("#supplier-payables-invoice-files-input").setInputFiles(
            invoiceEvidenceFiles(purchaseInvoiceNo).slice(0, 1),
        );
        await expect(workPage(page).locator("#supplier-payables-allocation-form-submit")).toBeEnabled({
            timeout: UI_TIMEOUT,
        });
        await workPage(page).locator("#supplier-payables-allocation-form-submit").click();
        const invoiceConfirm = workPage(page).getByRole("alertdialog").filter({
            hasText: "确认登记进项发票并核销",
        });
        await expect(invoiceConfirm).toBeVisible({ timeout: UI_TIMEOUT });
        await invoiceConfirm.locator("#supplier-payables-invoice-allocate-confirm-confirm").click();
        await expect(workPage(page).getByText("进项发票已登记").first()).toBeVisible({ timeout: UI_TIMEOUT });

        // 19) 出纳按原付款发起供应商退款 → 采购 → 财务总监
        page = await switchTo("fukuan");
        await page.goto("/finance/supplier-accounts?view=payment");
        await expect(workPage(page).getByRole("heading", { name: "供应商往来" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await workPage(page).locator("#supplier-payables-view-tabs-trigger-payment").click();
        const paidRow = workPage(page).getByRole("row").filter({ hasText: supplierPayment.payment_no });
        await expect(paidRow).toContainText("已过账", { timeout: UI_TIMEOUT });
        await paidRow.click();
        const refundBtn = workPage(page).getByRole("button", { name: "退款" }).first();
        await expect(refundBtn).toBeVisible({ timeout: UI_TIMEOUT });
        await refundBtn.click();
        await expect(workPage(page).getByRole("dialog").getByRole("heading", { name: /供应商退款/ })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await workPage(page)
            .locator("#supplier-payables-refund-request-reason")
            .fill("客户拒收直退供应商，按原付款追加退款，原付款保留");
        const supplierRefundCommitted = page.waitForResponse(
            (response) =>
                response.request().method() === "POST" &&
                response.url().includes("/admin/supplier-refunds/commit"),
        );
        await workPage(page).locator("#supplier-payables-refund-request-submit").click();
        const supplierRefundConfirm = workPage(page).getByRole("alertdialog", {
            name: /确认提交退款|提交退款/,
        });
        // 确认框与提交响应谁先到走哪条；prepareRefundDraft 直接提交时不弹确认框。
        if (await optionalStepVisible(supplierRefundConfirm, supplierRefundCommitted)) {
            await workPage(page).locator("#supplier-payables-refund-submit-confirm-confirm").click();
        }
        const supplierRefundResponse = await supplierRefundCommitted;
        expect(supplierRefundResponse.ok()).toBeTruthy();
        const supplierRefundResult = (await supplierRefundResponse.json()).data;
        supplierRefundNo = supplierRefundResult.refund_no ?? supplierRefundResult.refundNo;
        expect(String(supplierRefundNo).length).toBeGreaterThan(2);
        await expect(workPage(page).getByText(/退款已提交审批/).first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });

        page = await switchTo("caigou");
        await openWorkspaceTask(page, "供应商退款审批", supplierRefundNo || SUPPLIER_SHORT, "approval");
        await approveCurrentDocument(page);

        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "供应商退款审批", supplierRefundNo || SUPPLIER_SHORT, "approval");
        await approveCurrentDocument(page);

        // 20) 开票人必须完成进项红票登记，页面不得出现审批动作
        page = await switchTo("kaipiao");
        await page.goto("/finance/supplier-accounts?view=purchase_invoice");
        const supplierHeading = workPage(page).getByRole("heading", { name: "供应商往来" });
        await expect(supplierHeading).toBeVisible({ timeout: UI_TIMEOUT });
        {
            await workPage(page).locator("#supplier-payables-view-tabs-trigger-purchase-invoice").click();
            await workPage(page).locator("#supplier-payables-toolbar-search").fill(purchaseInvoiceNo);
            await workPage(page).locator("#supplier-payables-toolbar-search").press("Enter");
            const purchaseInvoiceRow = workPage(page)
                .getByRole("row")
                .filter({ hasText: purchaseInvoiceNo })
                .filter({ hasText: "蓝票" });
            await expect(purchaseInvoiceRow).toBeVisible({ timeout: UI_TIMEOUT });
            await purchaseInvoiceRow.click();
            const purchaseRed = workPage(page).locator("#supplier-payables-preview-record-red-invoice");
            await expect(purchaseRed).toBeVisible({ timeout: UI_TIMEOUT });
            {
                await purchaseRed.click();
                const purchaseRedDlg = workPage(page).getByRole("dialog", { name: "进项红票" });
                await expect(purchaseRedDlg).toBeVisible({ timeout: UI_TIMEOUT });
                await purchaseRedDlg
                    .locator("#supplier-payables-reverse-dialog-reason")
                    .fill("客户拒收，进项红冲");
                await purchaseRedDlg
                    .locator("#supplier-payables-reverse-dialog-red-invoice-no")
                    .fill(`R${purchaseInvoiceNo}`);
                await purchaseRedDlg.locator("#supplier-payables-reverse-dialog-confirm").click();
                await expect(workPage(page).getByText("红票已登记").first()).toBeVisible({
                    timeout: UI_TIMEOUT,
                });
            }
            await expect(workPage(page).getByRole("button", { name: /选择流程|通过|驳回/ })).toHaveCount(0);
        }

        // 21) 销售侧：原出库/回款/发票保留；无确认处理完成入口；销售单不得因开票关闭
        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(workPage(page).getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("履约中", { exact: true })).toBeVisible();
        await expect(workPage(page).getByText("改单中")).toHaveCount(0);
        await assertNoReturnCreateUi(page);
        await workPage(page).locator("#sales-order-follow-up-invoice").click();
        await expect(workPage(page).getByRole("heading", { name: "已开发票", exact: true })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        // 全额红冲后有效开票归零；原蓝票和红票已在财务台账核验。
        await page.goto("/workspace/tasks");
        await assertNoReturnApprovalTasks(page);
    } finally {
        await session?.context.close();
    }
});
