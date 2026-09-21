/**
 * 流程: [flow-19] 已生效销售单禁止直接作废。
 * 验收口径: 按当前销售单状态机，只有草稿允许转为作废；已生效单请求必须返回
 * HTTP 422 / BUSINESS_RULE_BLOCKED，销售单、库存预占及仓发草稿必须保持不变。
 * 场景: 仓储盘盈、财务审批后，销售单全部使用现有库存供给；全程不得生成采购单。
 * 操作: 通过 ERP 页面建立业务事实；详情页无作废按钮，使用 HTTP 验证拒绝契约。
 * 仓发任务必须继续可用，但本用例不得确认发货或消耗库存预占。
 */
import fs from "node:fs";
import path from "node:path";

import {
    test,
    expect,
    type Locator,
    type Page,
} from "@playwright/test";

import { apiGet, apiLogin } from "../helpers/api";
import { createCustomerViaUi } from "../helpers/customers";
import { ensureZeroBalanceDimension } from "../helpers/inventory";
import { openLoggedInWorkspace, type LoggedInSession } from "../helpers/login";
import {
    ensureDefaultProcurementOwner,
    submitCreatedSalesOrder,
} from "../helpers/procurement";
import { confirmSupplyAllocation } from "../helpers/sourcing";
import {
    approveCurrentDocument,
    chooseOption,
    expectToast,
    openFulfillmentWorkspaceForm,
    openWorkspaceTask,
    pickCalendarDay,
    readHeaderDocumentNumber,
    selectWorkspaceFamily,
} from "../helpers/ui";

test.describe.configure({ mode: "serial" });

const UI_TIMEOUT = 20_000;
const FLOW_TIMEOUT = 12 * 60 * 1000;
const API_BASE = process.env.API_BASE ?? "http://127.0.0.1:10001";
const SKU_CODE = "TEA-SF-LJ-250";
const SKU_NAME = "狮峰明前龙井礼盒";
const WAREHOUSE_CODE = "BJ-TZ-01";
const WAREHOUSE_NAME = "北京通州仓";
const GAIN_QTY = "10";
const SALE_QTY = "2";
const AFTER_RESERVE_AVAILABLE = "8";

const CONTRACT_PDF = path.resolve(process.cwd(), "fixtures/sample-contract.pdf");
const MINIMAL_PDF = Buffer.from(
    "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R>>endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000068 00000 n \n0000000125 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n210\n%%EOF\n",
);

type LoginName = "xiaoshou" | "caigou" | "cangchu" | "caiwu" | "admin";

type ApiPage<T> = {
    items?: T[];
    total?: number;
};

type StockBalance = {
    id: string;
    warehouse_id: string;
    warehouse_code?: string;
    warehouse_name?: string;
    sku_id: string;
    sku_code?: string;
    sku_name?: string;
    on_hand_quantity: string;
    reserved_quantity: string;
    available_quantity: string;
};

type StockReservation = {
    id: string;
    status: string;
    reserved_quantity: string;
    consumed_quantity: string;
    released_quantity: string;
    sales_order_line_id?: string;
};

type DeliveryRow = {
    id: string;
    delivery_no?: string;
    sales_order_id: string;
    status: string;
    delivery_type?: string;
};

function orderTitleRow(page: Page, customerName: string) {
    return page.getByRole("heading", { name: customerName }).locator("xpath=..");
}

function escapeRe(value: string): string {
    return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

async function expectNoWorkspaceTask(
    page: Page,
    typeLabel: string,
    hint?: string,
    family?: "approval" | "procurement" | "fulfillment",
) {
    await page.goto("/workspace");
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    if (family) {
        await selectWorkspaceFamily(page, family);
    }
    // 同上：不填搜索框，直接断言无匹配任务。空队列时待办列表可能不渲染，
    // 因此不在列表存在性上断言，只断言匹配按钮数为零。
    const noLabel = `(?:${typeLabel})`;
    const noCandidates = page.getByRole("button", { name: new RegExp(noLabel) });
    const noNamed = hint
        ? page.getByRole("button", {
              name: new RegExp(
                  `${noLabel}[\\s\\S]*${escapeRe(hint)}|${escapeRe(hint)}[\\s\\S]*${noLabel}`,
              ),
          })
        : noCandidates;
    const task = hint ? noNamed.or(noCandidates.filter({ hasText: hint })) : noCandidates;
    await expect(task).toHaveCount(0, {
        timeout: UI_TIMEOUT,
    });
}

function plusDaysIso(days: number): string {
    const date = new Date();
    date.setDate(date.getDate() + days);
    const pad = (value: number) => String(value).padStart(2, "0");
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function uniqueCreditCode(stamp: string): string {
    const raw = `91${stamp.replace(/[^0-9A-Za-z]/g, "").toUpperCase()}E2EVOIDRES`;
    return raw.slice(0, 18).padEnd(18, "0");
}

function contractPdf(): { name: string; mimeType: string; buffer: Buffer } {
    if (fs.existsSync(CONTRACT_PDF)) {
        return {
            name: "sample-contract.pdf",
            mimeType: "application/pdf",
            buffer: fs.readFileSync(CONTRACT_PDF),
        };
    }
    return {
        name: "sample-contract.pdf",
        mimeType: "application/pdf",
        buffer: MINIMAL_PDF,
    };
}

function balanceRow(page: Page): Locator {
    return page
        .locator("#inventory-ledger-balance-table")
        .getByRole("row")
        .filter({ hasText: SKU_CODE })
        .filter({ hasText: new RegExp(`${WAREHOUSE_NAME}|${WAREHOUSE_CODE}`) });
}

function reservationRow(page: Page, salesOrderLineId: string): Locator {
    return page
        .locator("#inventory-ledger-reservation-table")
        .getByRole("row")
        .filter({ hasText: salesOrderLineId });
}

async function searchInventory(page: Page, query: string) {
    await page.goto("/inventory");
    await expect(page.getByRole("heading", { name: "库存台账" })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    await page.locator("#inventory-ledger-view-balance").click();
    const search = page.locator("#inventory-ledger-search");
    await expect(search).toBeVisible({ timeout: UI_TIMEOUT });
    await search.fill(query);
    await search.press("Enter");
}

async function assertBalanceNumbers(
    page: Page,
    expected: { onHand: string; reserved: string; available: string },
) {
    await searchInventory(page, SKU_CODE);
    const row = balanceRow(page);
    await expect(row).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(row).toContainText(expected.onHand);
    await expect(row).toContainText(expected.reserved);
    await expect(row).toContainText(expected.available);
}

const apiTokens = new Map<LoginName, Promise<string>>();

async function tokenOf(login: LoginName): Promise<string> {
    let token = apiTokens.get(login);
    if (!token) {
        token = apiLogin(login);
        apiTokens.set(login, token);
    }
    return token;
}

async function listBalances(token: string): Promise<StockBalance[]> {
    const page = await apiGet<ApiPage<StockBalance>>(token, "/admin/stock-balances", {
        page: 1,
        page_size: 100,
    } as never);
    return page.items ?? [];
}

async function listReservations(token: string): Promise<StockReservation[]> {
    const page = await apiGet<ApiPage<StockReservation>>(
        token,
        "/admin/stock-reservations",
        { page: 1, page_size: 100 } as never,
    );
    return page.items ?? [];
}

async function listDeliveries(
    token: string,
    salesOrderId: string,
): Promise<DeliveryRow[]> {
    const page = await apiGet<ApiPage<DeliveryRow>>(token, "/admin/deliveries", {
        sales_order_id: salesOrderId,
        page: 1,
        page_size: 50,
    } as never);
    return (page.items ?? []).filter((row) => row.sales_order_id === salesOrderId);
}

async function listPurchaseOrders(token: string): Promise<unknown[]> {
    const page = await apiGet<ApiPage<unknown>>(token, "/admin/purchase-orders", {
        page: 1,
        page_size: 20,
    } as never);
    return page.items ?? [];
}

async function bearerToken(page: Page): Promise<string> {
    const token = await page.evaluate(() => localStorage.getItem("erp.token"));
    expect(token, "登录 token 应写入 localStorage erp.token").toBeTruthy();
    return token as string;
}

async function fetchSalesOrder(
    page: Page,
    salesOrderId: string,
): Promise<Record<string, unknown>> {
    const token = await bearerToken(page);
    const response = await page.request.get(
        `${API_BASE}/admin/sales-orders/${salesOrderId}`,
        { headers: { Authorization: `Bearer ${token}` } },
    );
    expect(response.ok(), `读取销售单 ${salesOrderId} 失败`).toBeTruthy();
    const body = (await response.json()) as {
        data?: Record<string, unknown>;
    } & Record<string, unknown>;
    return (body.data ?? body) as Record<string, unknown>;
}

/**
 * 详情页不得暴露作废入口，直接调用命令同样必须拒绝且不改变单据。
 */
async function assertEffectiveVoidRejected(page: Page, salesOrderId: string) {
    await expect(page.getByRole("button", { name: /作废/ })).toHaveCount(0);
    const detail = await fetchSalesOrder(page, salesOrderId);
    const version = Number(detail.version ?? 1);
    const token = await bearerToken(page);
    const response = await page.request.post(
        `${API_BASE}/admin/sales-orders/${salesOrderId}/void`,
        {
            headers: {
                Authorization: `Bearer ${token}`,
                "Content-Type": "application/json",
            },
            data: { version },
        },
    );
    const bodyText = await response.text();
    expect(response.status(), bodyText).toBe(422);
    const body = JSON.parse(bodyText);
    expect(body.code).toBe("BUSINESS_RULE_BLOCKED");
    expect(body.errorMessage).toContain("Effective → Voided");
    expect(body.success).toBe(false);
    expect(await fetchSalesOrder(page, salesOrderId)).toEqual(detail);
    await page.reload();
}

test("flow-19 已生效销售单禁止直接作废：预占和仓发草稿保持、零采购单", async ({
    browser,
}) => {
    test.setTimeout(FLOW_TIMEOUT);
    const stamp = Date.now().toString(36).toUpperCase();
    const customerName = `E2E拒绝作废客户${stamp}`;
    const contractNo = `HT-E2E-VOID-${stamp}`;
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
        // 0) 采购责任默认调度人：提交实物销售单前必须能解析采购负责人
        let page = await switchTo("admin");
        await ensureDefaultProcurementOwner(page);
        await ensureZeroBalanceDimension(WAREHOUSE_CODE, SKU_CODE);

        // 1) cangchu 盘盈准备指定仓库 + SKU 可用库存（不假设期初）
        page = await switchTo("cangchu");
        await searchInventory(page, SKU_CODE);
        const openingRow = balanceRow(page);
        await expect(openingRow).toBeVisible({ timeout: UI_TIMEOUT });
        await openingRow.getByRole("button", { name: "库存调整" }).click();
        const adjustDialog = page.getByRole("dialog", { name: "发起库存调整" });
        await expect(adjustDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(
            page,
            adjustDialog.locator("#inventory-adjustment-dialog-reason-type"),
            "盘盈（增加）",
        );
        await adjustDialog.locator("#inventory-adjustment-dialog-quantity").fill(GAIN_QTY);
        await adjustDialog
            .locator("#inventory-adjustment-dialog-note")
            .fill("flow-19 作废释放预占盘盈");
        await adjustDialog.locator("#inventory-adjustment-dialog-submit").click();
        await expect(page.getByText("调整已提交审批").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(adjustDialog).toBeHidden({ timeout: UI_TIMEOUT });

        // 2) caiwu 审批库存调整，余额才增加
        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "库存调整单审批", undefined, "approval");
        await approveCurrentDocument(page);

        page = await switchTo("cangchu");
        await assertBalanceNumbers(page, {
            onHand: GAIN_QTY,
            reserved: "0",
            available: GAIN_QTY,
        });
        await expect(balanceRow(page)).toContainText("有可用");

        // 3) xiaoshou 建客户 / 归档合同 / 开实物销售单（数量不超过可用库存）
        page = await switchTo("xiaoshou");
        const shortName = `作废释放${stamp}`;
        await createCustomerViaUi(page, {
            legalName: customerName,
            shortName,
            creditCode: uniqueCreditCode(stamp),
            paymentTermLabel: "货到 15 天",
            contact: { name: "李测", phone: "13800138001" },
            address: "北京市朝阳区测试路 1 号",
        });
        await expect(page.getByRole("link", { name: shortName, exact: true })).toBeVisible({
            timeout: UI_TIMEOUT,
        });

        await page.goto("/sales/contracts");
        await expect(
            page.getByRole("heading", { name: "合同", exact: true }),
        ).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#page-actions-action-upload").click();
        const contractDialog = page.getByRole("dialog", { name: "上传合同 PDF" });
        await expect(contractDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await contractDialog
            .locator("#card-contracts-upload-pdf-input")
            .setInputFiles(contractPdf());
        await contractDialog.locator("#card-contracts-upload-contract-no").fill(contractNo);
        await chooseOption(
            page,
            contractDialog.locator("#card-contracts-upload-customer"),
            customerName,
        );
        await expect(
            contractDialog.locator("#card-contracts-upload-settlement-party"),
        ).not.toHaveValue("", { timeout: UI_TIMEOUT });
        await contractDialog.locator("#card-contracts-upload-submit").click();
        await expectToast(page, "合同 PDF 已归档");
        await expect(contractDialog).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(page.getByText(contractNo).first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });

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
        await chooseOption(page, page.locator("#sales-orders-create-contract"), contractNo);
        await expect(page.getByText(customerName).first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });
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
            .fill(SKU_CODE);
        await skuDialog
            .locator("#master-data-list-sellable-list-toolbar-search-input")
            .press("Enter");
        const skuRow = skuDialog.getByRole("checkbox", {
            name: new RegExp(`选择.*${SKU_NAME}`),
        });
        await expect(skuRow.first()).toBeVisible({ timeout: UI_TIMEOUT });
        await skuRow.first().check();
        await skuDialog.locator("#sales-orders-sku-picker-confirm").click();
        await expect(skuDialog).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(page.getByText(SKU_NAME).first()).toBeVisible({ timeout: UI_TIMEOUT });
        await page.getByLabel("数量").fill(SALE_QTY);
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
        await expect(
            orderTitleRow(page, customerName).getByText(/审批中|审核中/),
        ).toBeVisible({ timeout: UI_TIMEOUT });
        salesOrderNo = await readHeaderDocumentNumber(page)
        expect(salesOrderNo).toBeTruthy();
        await page.getByRole("tab", { name: /采购/ }).click();
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("待采购", {
            timeout: UI_TIMEOUT,
        });
        await expect(page.locator("#sales-orders-detail-start-change")).toBeDisabled();
        await expect(page.getByRole("button", { name: /作废/ })).toHaveCount(0);

        // 4) caigou 采购确认：只通过，不选源
        page = await switchTo("caigou");
        await openWorkspaceTask(page, "销售单审批", salesOrderNo, "approval");
        await expect(page.getByRole("button", { name: "预览供给分配" })).toHaveCount(0);
        await expect(page.getByLabel("供给来源 / 履约责任")).toHaveCount(0);
        await approveCurrentDocument(page);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible({
            timeout: UI_TIMEOUT,
        });

        // 5) caigou 供给分配：全部走现有库存，确认后零张采购单 + 仓发草稿
        page = await switchTo("caigou");
        await page.getByRole("button", { name: "刷新" }).click().catch(() => undefined);
        await openWorkspaceTask(page, "待供给分配|供给分配", salesOrderNo, "procurement");
        await expect(
            page.getByRole("heading", { name: /供给分配|销售明细与供给方案/ }).first(),
        ).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByText("将创建采购单").locator("xpath=..")).toContainText(
            "0 张",
        );
        await expect(page.getByText("将建立库存预留").locator("xpath=..")).toContainText(
            "1 条",
        );
        await page.locator("#procurement-orders-create-preview").click();
        const previewDialog = page.getByRole("dialog", { name: "预览供给分配" });
        await expect(previewDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(previewDialog.getByText("现有库存分配")).toBeVisible();
        await expect(previewDialog.getByText(/无需创建采购单/)).toBeVisible();
        await expect(
            previewDialog.getByText("本次全部由现有库存满足，不会创建采购单。"),
        ).toBeVisible();
        await expect(previewDialog.getByText(/确认提交 \d+ 张采购单/)).toHaveCount(0);
        await expect(
            previewDialog.getByText(/现有库存已满足本次分配，无需创建采购单/),
        ).toBeVisible();
        await confirmSupplyAllocation(page, /供给分配已完成|本次供给分配已保存/);

        // 6) 负向：零张采购单；库存 available 减少、reserved 增加；仓发草稿已形成但尚未出库
        page = await switchTo("caigou");
        await page.goto("/procurement/orders");
        await expect(page.getByRole("heading", { name: "采购单", exact: true })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("暂无采购单")).toBeVisible({ timeout: UI_TIMEOUT });
        expect(await listPurchaseOrders(await tokenOf("caigou"))).toHaveLength(0);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await page.getByRole("tab", { name: /采购/ }).click();
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("采购已覆盖", {
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("本单还没有采购单。")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("改单中")).toHaveCount(0);
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toHaveCount(0);

        const effectiveOrder = await fetchSalesOrder(page, salesOrderId);
        const effectiveLines = effectiveOrder.lines as Array<{ id: string }>;
        expect(effectiveLines).toHaveLength(1);
        page = await switchTo("cangchu");
        await assertBalanceNumbers(page, {
            onHand: GAIN_QTY,
            reserved: SALE_QTY,
            available: AFTER_RESERVE_AVAILABLE,
        });
        await expect(balanceRow(page)).toContainText("有预占");
        await page.locator("#inventory-ledger-view-reservation").click();
        const reservationFacts = await listReservations(await tokenOf("cangchu"));
        expect(reservationFacts).toHaveLength(1);
        const reservedLineId = reservationFacts[0].sales_order_line_id;
        expect(reservedLineId).toBe(effectiveLines[0].id);
        const activeReservation = reservationRow(page, reservedLineId!);
        await expect(activeReservation).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(activeReservation).toContainText("有效");
        await expect(activeReservation).toContainText("已释放 0");
        await expect(page.getByRole("button", { name: /释放预占/ })).toHaveCount(0);

        const deliveriesBeforeVoid = await listDeliveries(
            await tokenOf("cangchu"),
            salesOrderId,
        );
        expect(deliveriesBeforeVoid.length).toBeGreaterThan(0);
        expect(
            deliveriesBeforeVoid.every((row) => row.status === "DRAFT"),
            `仓发出库前草稿状态应为 DRAFT，实际 ${deliveriesBeforeVoid
                .map((row) => row.status)
                .join(",")}`,
        ).toBe(true);
        expect(deliveriesBeforeVoid.some((row) => row.status === "SHIPPED")).toBe(false);

        await openWorkspaceTask(page, "履约处理", customerName, "fulfillment");
        await openFulfillmentWorkspaceForm(page);
        await expect(page.locator('[aria-label="公司仓发表单"]')).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByRole("button", { name: /^(通过|同意审批)$/ })).toHaveCount(0);
        await expect(page.getByRole("button", { name: "过账" })).toHaveCount(0);
        await expect(
            page.locator("#fulfillment-operations-work-surface-confirm"),
        ).toBeVisible({ timeout: UI_TIMEOUT });
        // 本 spec 停在出库前：不点确认发货

        // 7) 已生效单直接作废必须被拒绝，销售单保持不变
        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await assertEffectiveVoidRejected(page, salesOrderId);
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("本单已作废，不再进入履约或结案。")).toHaveCount(0);
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toHaveCount(0);
        await expect(page.locator("#sales-orders-detail-start-change")).toBeEnabled();
        await expect(page.getByText("改单中")).toHaveCount(0);
        await expect(page.getByRole("button", { name: /作废/ })).toHaveCount(0);
        await page.getByRole("tab", { name: /采购/ }).click();
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("采购已覆盖", {
            timeout: UI_TIMEOUT,
        });
        const unchanged = await fetchSalesOrder(page, salesOrderId);
        expect(
            String(unchanged.commercial_status ?? unchanged.commercialStatus ?? "").toUpperCase(),
        ).toBe("EFFECTIVE");

        // 8) 拒绝请求不得释放或消耗预占，也不得改变库存余额
        page = await switchTo("cangchu");
        await expect
            .poll(async () => {
                const rows = await listBalances(await tokenOf("cangchu"));
                const row = rows.find(
                    (item) =>
                        item.sku_code === SKU_CODE &&
                        (item.warehouse_code === WAREHOUSE_CODE ||
                            item.warehouse_name === WAREHOUSE_NAME),
                );
                return row
                    ? `${row.on_hand_quantity}/${row.reserved_quantity}/${row.available_quantity}`
                    : "";
            }, { timeout: UI_TIMEOUT })
            .toBe(`${GAIN_QTY}/${SALE_QTY}/${AFTER_RESERVE_AVAILABLE}`);
        await assertBalanceNumbers(page, {
            onHand: GAIN_QTY,
            reserved: SALE_QTY,
            available: AFTER_RESERVE_AVAILABLE,
        });
        await page.locator("#inventory-ledger-view-reservation").click();
        const retained = reservationRow(page, reservedLineId!);
        await expect(retained).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(retained).toContainText("已释放 0");
        await expect(retained).toContainText("有效");
        await expect(retained).toContainText("已消耗 0");
        await page.locator("#inventory-ledger-view-movement").click();
        await expect(page.getByText("仓库发出")).toHaveCount(0);
        await expect(page.getByText("仓发出库")).toHaveCount(0);
        await expect(page.getByText("采购入库")).toHaveCount(0);

        const reservations = await listReservations(await tokenOf("cangchu"));
        expect(reservations).toEqual(reservationFacts);
        expect(reservations.every((row) => String(row.status).toUpperCase() === "ACTIVE"))
            .toBe(true);

        // 9) 原仓发草稿及任务必须保留，仍可办理发货，但本用例不执行发货
        const deliveriesAfterVoid = await listDeliveries(
            await tokenOf("cangchu"),
            salesOrderId,
        );
        expect(deliveriesAfterVoid).toEqual(deliveriesBeforeVoid);
        expect(deliveriesAfterVoid.every((row) => row.status === "DRAFT")).toBe(true);
        await openWorkspaceTask(page, "履约处理", customerName, "fulfillment");
        await openFulfillmentWorkspaceForm(page);
        await expect(page.locator('[aria-label="公司仓发表单"]')).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(page, page.getByLabel("承运方"), "顺丰速运");
        await page.getByLabel("物流单号").fill(`SF19-${stamp}`);
        await expect(page.locator("#fulfillment-operations-work-surface-confirm"))
            .toBeEnabled({ timeout: UI_TIMEOUT });

        // 10) 全程不得建采购单、不得关闭、不得开变更单、被拒绝的作废请求不得出现审批实例
        page = await switchTo("caigou");
        await page.goto("/procurement/orders");
        await expect(page.getByText("暂无采购单")).toBeVisible({ timeout: UI_TIMEOUT });
        expect(await listPurchaseOrders(await tokenOf("caigou"))).toHaveLength(0);
        await expectNoWorkspaceTask(page, "待供给分配|供给分配", salesOrderNo, "procurement");
        await expectNoWorkspaceTask(page, "销售变更单审批", salesOrderNo, "approval");
        await page.goto("/workspace");
        await expect(page.getByRole("button", { name: /供应商付款/ })).toHaveCount(0);
        await expect(page.getByRole("button", { name: /入库/ })).toHaveCount(0);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.locator("#sales-orders-detail-start-change")).toBeEnabled();
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toHaveCount(0);
        await expect(page.getByRole("button", { name: "过账" })).toHaveCount(0);
        await expectNoWorkspaceTask(page, "客户验收登记", salesOrderNo, "fulfillment");
        await expectNoWorkspaceTask(page, "销售变更单审批", salesOrderNo, "approval");
    } finally {
        await session?.context.close();
    }
});
