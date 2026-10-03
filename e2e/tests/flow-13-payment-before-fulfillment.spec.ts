/**
 * 流程: [flow-13] 先款后货：付款完成前不得履约
 * 文档: docs/erp-phase-1.md §7.2 / §7.4（先款后货时付款完成后发货；账期/货到付款先履约对照 flow-01）
 * 账号: admin（采购责任默认调度人）
 *       xiaoshou（客户 / 合同 / 销售单）
 *       caigou（销售单采购确认、供给分配、代发履约）
 *       caiwu（采购单财务审批）
 *       cangchu（入库 / 仓发）
 *       fukuan（W01 供应商付款任务确认入账；SupplierPayment=NO_APPROVAL）
 *
 * 验收约定：
 * - 文档写「先款后货时，付款完成后发货」；种子供应商狮峰茶叶为 PREPAY_50，
 *   门禁按有效已核销付款达到比例门槛即可履约，不必等应付全部结清。
 *   本流程仍由出纳把付款任务待付一次付清，同时满足任务完成与门槛。
 * - 文档 7.3.1 把「创建采购单」和「提交审批」画成两步；代码在供给分配确认同一事务内建单并立即提交。
 * - 客户侧本单用「货到 15 天」，与供应商「先款 50%」对照：客户账期不放开采购履约。
 * - W01 必须按采购生效版本的冻结比例及有效付款净额展示先款门槛。
 * - W09 /fulfillment 只重定向到 W01；履约确认只在工作台原地处理。
 * - 履约主按钮是「确认入库 / 确认发货」，禁止用「过账」匹配。
 * - 开发目录无 VIRTUAL SKU（电子交付）；卡券 VOUCHER 不能走普通采购单。
 *   线下服务种子供应商安达为 POSTPAY_NET15，不会启用先款门禁。
 *   本流程用狮峰茶叶实物拆「入仓 + 供应商直发」覆盖入库与代发；电子交付/服务与入库/直发共用 ensure_prepay_gate。
 */
import fs from "node:fs";
import path from "node:path";
import { test, expect, type Browser, type BrowserContext, type Page } from "../helpers/test";

import { createCustomerViaUi } from "../helpers/customers";
import { addDeliveryTrackingEntry } from "../helpers/fulfillment";
import { API_BASE, apiGet, apiToken } from "../helpers/api";
import { openLoggedInWorkspace } from "../helpers/login";
import {
    ensureDefaultProcurementOwner,
    submitCreatedSalesOrder,
} from "../helpers/procurement";
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
const CONTRACT_PDF = path.resolve(process.cwd(), "fixtures/sample-contract.pdf");
const SKU_INBOUND = "狮峰明前龙井礼盒";
const SKU_DIRECT = "狮峰陈皮普洱礼盒";
const SUPPLIER_SHORT = "狮峰茶叶";
const WAREHOUSE_NAME = "北京通州仓";
const WAREHOUSE_CODE = "BJ-TZ-01";
const SALES_QTY = "1";
const PAYMENT_TERM_CUSTOMER = "货到 15 天";
const PAYMENT_TERM_SUPPLIER = "先款 50%";
const INBOUND_OPTION = "杭州狮峰茶叶有限公司 · 入仓";
const DIRECT_OPTION = "杭州狮峰茶叶有限公司 · 供应商直发";
const RECEIPT_PNG = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    "base64",
);
const MINIMAL_PDF = Buffer.from(
    "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R>>endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000068 00000 n \n0000000125 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n210\n%%EOF\n",
);

type LoginName = "xiaoshou" | "caigou" | "cangchu" | "caiwu" | "fukuan" | "admin";
type Session = { context: BrowserContext; page: Page };
type PurchaseRef = { no: string; responsibility: "入仓" | "供应商直发" };

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
    await expect(page.getByRole("heading", { name: heading })).toBeVisible({ timeout: UI_TIMEOUT });
}

async function approveMatchingTasks(
    page: Page,
    family: "approval" | "procurement" | "fulfillment" | "finance",
    name: RegExp,
    hint: string,
    expected: number,
) {
    for (let i = 0; i < expected; i += 1) {
        await openWorkspaceTask(page, name, hint, family);
        await approveCurrentDocument(page);
    }
    await page.goto(`/workspace?family=${family}`);
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    const list = page.getByRole("list", { name: "待办列表" });
    const empty = page.getByText(/当前没有待处理事项|当前筛选没有待办|范围内没有待办/);
    await expect(list.or(empty).first()).toBeVisible({ timeout: UI_TIMEOUT });
    if (await list.count()) {
        await expect(list.getByRole("button", { name })).toHaveCount(0, { timeout: UI_TIMEOUT });
    }
}

async function confirmFormal(page: Page, title: string | RegExp, confirmName: string | RegExp) {
    const dialog = page.getByRole("alertdialog").or(page.getByRole("dialog")).filter({ hasText: title });
    await expect(dialog.first()).toBeVisible({ timeout: UI_TIMEOUT });
    await dialog.getByRole("button", { name: confirmName }).click();
    await expect(dialog.first()).toBeHidden({ timeout: UI_TIMEOUT });
}

async function pickSku(page: Page, keyword: string, name: string) {
    await page.getByRole("button", { name: "添加商品" }).first().click();
    const skuDialog = page.getByRole("dialog", { name: /添加商品|更换销售商品/ });
    await expect(skuDialog).toBeVisible({ timeout: UI_TIMEOUT });
    const skuSearch = skuDialog.locator("#master-data-list-sellable-list-toolbar-search-input");
    await skuSearch.fill(keyword);
    await skuSearch.press("Enter");
    const checkbox = skuDialog.getByRole("checkbox", { name: new RegExp(`选择.*${name}`) });
    await expect(checkbox.first()).toBeVisible({ timeout: UI_TIMEOUT });
    await checkbox.first().check();
    await skuDialog.locator("#sales-orders-sku-picker-confirm").click();
    await expect(skuDialog).toBeHidden({ timeout: UI_TIMEOUT });
    await expect(page.getByText(name).first()).toBeVisible({ timeout: UI_TIMEOUT });
}

async function fillAllLineQuantities(page: Page, qty: string) {
    const inputs = page.locator('[id^="sales-orders-create-line-"][id$="-quantity"]');
    const count = await inputs.count();
    expect(count).toBeGreaterThan(0);
    for (let i = 0; i < count; i += 1) {
        await inputs.nth(i).fill(qty);
    }
}

function sourcingRow(page: Page, item: string | RegExp) {
    const name =
        typeof item === "string"
            ? new RegExp(`${item}的供给方案`)
            : new RegExp(`${item.source}.*的供给方案`);
    return page.getByRole("region", { name });
}

async function chooseSourcing(
    page: Page,
    item: string | RegExp,
    option: string,
    warehouse?: string,
) {
    const row = sourcingRow(page, item);
    await expect(row).toBeVisible({ timeout: UI_TIMEOUT });
    const expand = row.getByRole("button", { name: "调整方案" });
    if (await expand.isVisible().catch(() => false)) await expand.click();
    const sourcing = row.locator('[id$="-sourcing-option"]');
    await expect(sourcing).toBeVisible({ timeout: UI_TIMEOUT });
    // 选项很多时第一次点击可能打在重绘前的条目上，下拉仍开着且值是空的。再选一次。
    let attempt = 0;
    await expect(async () => {
        attempt += 1;
        if (attempt > 1) await page.keyboard.press("Escape");
        await chooseOption(page, sourcing, option, option.includes("直发") ? "直发" : "入仓");
    }).toPass({ timeout: UI_TIMEOUT * 2 });
    if (warehouse) {
        const warehouseInput = row.locator('[id$="-warehouse"]');
        await expect(warehouseInput).toBeVisible({ timeout: UI_TIMEOUT });
        // 仓库列表接口以仓库代码标识选项。
        await chooseOption(page, warehouseInput, WAREHOUSE_CODE, WAREHOUSE_CODE);
    } else {
        await expect(row.locator('[id$="-warehouse"]')).toHaveCount(0);
    }
}

function supplierPaymentTasks(page: Page, purchaseNo: string) {
    const list = page.getByRole("list", { name: "待办列表" });
    const escaped = purchaseNo.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const pattern = new RegExp(
        `供应商付款处理[\\s\\S]*${escaped}|${escaped}[\\s\\S]*供应商付款处理`,
    );
    return list
        .getByRole("button", { name: pattern })
        .or(list.getByRole("button").filter({ hasText: pattern }));
}

async function assertNoPaymentApproval(page: Page) {
    await expect(page.getByText("供应商付款单审批")).toHaveCount(0);
    await expect(page.getByText("SupplierPayment")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "提交审批" })).toHaveCount(0);
    await expect(page.getByText("电子交付单审批")).toHaveCount(0);
    await expect(page.getByText("服务履约单审批")).toHaveCount(0);
    await expect(page.getByText("采购收货单审批")).toHaveCount(0);
}

async function assertPurchaseOrderPrepayFacts(page: Page, responsibility: PurchaseRef["responsibility"]) {
    await expect(page.getByText("付款条件")).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(page.getByText(PAYMENT_TERM_SUPPLIER).first()).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(page.getByText(responsibility, { exact: true }).first()).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    await expect(page.getByRole("button", { name: "过账" })).toHaveCount(0);
    await page.getByRole("tab", { name: "履约" }).click();
    await expect(page.getByText("履约进度")).toBeVisible({ timeout: UI_TIMEOUT });
}

async function readPurchaseOrders(page: Page, salesOrderId: string, salesOrderNo: string): Promise<PurchaseRef[]> {
    await gotoHeading(page, "/procurement/orders", "采购单");
    const search = page.locator("#procurement-orders-list-search");
    await search.fill(salesOrderNo);
    await search.press("Enter");
    await expect(page.getByText("2 条")).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(page.getByRole("table").getByText("草稿", { exact: true })).toHaveCount(0);
    await expect(page.getByText(PAYMENT_TERM_SUPPLIER).first()).toBeVisible({ timeout: UI_TIMEOUT });
    const rows = page.getByRole("table").getByRole("row").filter({ hasText: salesOrderNo });
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
        await expect(page.getByText("采购单").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByText("已生效").first()).toBeVisible({ timeout: UI_TIMEOUT });
        const no = await readHeaderDocumentNumber(page)
        expect(no.length).toBeGreaterThan(0);
        await expect(page.getByText("入仓", { exact: true }).or(page.getByText("供应商直发", { exact: true })).filter({ visible: true }).first())
            .toBeVisible({ timeout: UI_TIMEOUT });
        const responsibility: PurchaseRef["responsibility"] =
            (await page.getByText("供应商直发", { exact: true }).filter({ visible: true }).count()) > 0 ? "供应商直发" : "入仓";
        await assertPurchaseOrderPrepayFacts(page, responsibility);
        refs.push({ no, responsibility });
    }
    const inbound = refs.filter((row) => row.responsibility === "入仓");
    const direct = refs.filter((row) => row.responsibility === "供应商直发");
    expect(inbound).toHaveLength(1);
    expect(direct).toHaveLength(1);
    return refs;
}

async function assertFulfillmentCannotComplete(page: Page, kind: "入库" | "代发") {
    if (kind === "入库") {
        await expect(page.locator('[aria-label="入库表单"]')).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.locator('[aria-label="供应商直发表单"]')).toHaveCount(0);
        await fillReceiptDraft(page);
    } else {
        await expect(page.locator('[aria-label="供应商直发表单"]')).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.locator('[aria-label="入库表单"]')).toHaveCount(0);
        await fillDirectDraft(page, `BLOCK${Date.now().toString().slice(-6)}`);
    }
    await expect(page.getByRole("button", { name: "过账" })).toHaveCount(0);
    const confirm = page.locator("#fulfillment-operations-work-surface-confirm");
    await expect(confirm).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(confirm).toHaveText(kind === "入库" ? "确认入库" : "确认发货");
    await expect(confirm).toBeDisabled({ timeout: UI_TIMEOUT });
    const gate = page.locator("#prepayment-gate");
    await expect(gate).toHaveAttribute("data-allowed", "false", { timeout: UI_TIMEOUT });

}

async function fillReceiptDraft(page: Page) {
    const qty = page.locator('[id^="fulfillment-operations-receipt-form-received-quantity-"]').first();
    if (await qty.count()) {
        const current = await qty.inputValue();
        if (!current || current === "0") await qty.fill(SALES_QTY);
    }
    const quality = page.locator('[id^="fulfillment-operations-receipt-form-quality-result-"]').first();
    if (await quality.count()) {
        await chooseOption(page, quality, "合格", "合格");
    }
}

async function fillDirectDraft(page: Page, trackingNo: string) {
    await addDeliveryTrackingEntry(page, {
        kind: "direct",
        lineIndex: 0,
        trackingNo,
        carrier: "顺丰速运",
    });
}

async function payPurchaseOrder(page: Page, purchaseNo: string): Promise<string> {
    await openWorkspaceTask(page, /供应商付款处理/, purchaseNo, "finance");
    await expect(page.getByLabel("当前付款任务")).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(page.getByRole("heading", { name: /向.+付款/ })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    await expect(page.getByText(SUPPLIER_SHORT).first()).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(page.getByText(purchaseNo).first()).toBeVisible({ timeout: UI_TIMEOUT });
    await assertNoPaymentApproval(page);
    await expect(page.getByRole("button", { name: "登记付款并核销" })).toBeVisible();
    const amount = page.locator("#supplier-payables-allocation-form-amount");
    await expect(amount).toHaveValue(/.+/, { timeout: UI_TIMEOUT });
    await page.locator("#supplier-payables-allocation-form-bank-receipt-input").setInputFiles({
        name: `bank-receipt-${purchaseNo}.png`,
        mimeType: "image/png",
        buffer: RECEIPT_PNG,
    });
    await page.locator("#supplier-payables-allocation-form-submit").click();
    const payDialog = page.getByRole("alertdialog").filter({ hasText: "确认付款" });
    await expect(payDialog).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(payDialog.getByText("提交审批")).toHaveCount(0);
    const committed = page.waitForResponse(
        response => response.request().method() === "POST" && response.url().includes("/admin/supplier-payments/commit"),
        { timeout: 60_000 },
    );
    await payDialog.locator("#supplier-payables-payment-submit-confirm-confirm").click();
    const response = await committed;
    expect(response.ok(), await response.text()).toBeTruthy();
    const payment = (await response.json()).data as { id: string; status: string };
    expect(payment.status).toBe("posted");
    expect(payment.id).toBeTruthy();
    await expectToast(page, /付款已登记/);
    return payment.id;
}

async function assertConfirmEnabled(page: Page) {
    const confirm = page.locator("#fulfillment-operations-work-surface-confirm");
    await expect(confirm).toBeEnabled({ timeout: UI_TIMEOUT });
    const gate = page.locator("#prepayment-gate");
    if ((await gate.count()) && (await gate.getAttribute("data-allowed")) === "false") {
        throw new Error("付款完成后先款门禁仍为阻断");
    }
}

test("flow-13 先款后货：付款完成前入库与代发均不可确认", async ({ browser }) => {
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

    const fulfillmentCandidates = (page: Page, hint: string) => {
        const list = page.getByRole("list", { name: "待办列表" });
        const quoted = hint.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
        return list
            .locator("button")
            .filter({ hasText: "履约处理" })
            .filter({ hasText: hint })
            .or(list.locator(`button[aria-label*="履约处理"][aria-label*="${quoted}"]`));
    };

    const closeFulfillmentDialog = async (page: Page) => {
        const dialog = page.getByRole("dialog", { name: "处理履约" });
        if (!(await dialog.isVisible().catch(() => false))) return;
        await page.keyboard.press("Escape");
        const hidden = await dialog.waitFor({ state: "hidden", timeout: 5_000 }).then(() => true).catch(() => false);
        if (hidden) return;
        await dialog.locator('[data-slot="dialog-close"]').click();
        await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT });
    };

    const openListedFulfillment = async (page: Page, purchaseOrderNo: string) => {
        const tasks = salesOrderNo
            ? fulfillmentCandidates(page, purchaseOrderNo).or(fulfillmentCandidates(page, salesOrderNo))
            : fulfillmentCandidates(page, purchaseOrderNo);
        const count = await tasks.count();
        if (count === 0) return false;
        let shipFallback: number | null = null;
        for (let index = 0; index < count; index += 1) {
            await closeFulfillmentDialog(page);
            const card = tasks.nth(index);
            const cardLabel = `${(await card.getAttribute("aria-label")) ?? ""} ${(await card.innerText()) || ""}`;
            await card.click();
            await openFulfillmentWorkspaceForm(page);
            const dialog = page.getByRole("dialog", { name: "处理履约" });
            if (cardLabel.includes(purchaseOrderNo)) return true;
            const inboundForm = dialog.locator('[aria-label="入库表单"]');
            const directForm = dialog.locator('[aria-label="供应商直发表单"]');
            const formReady = await inboundForm
                .or(directForm)
                .first()
                .waitFor({ state: "visible", timeout: UI_TIMEOUT })
                .then(() => true)
                .catch(() => false);
            const formText = formReady
                ? ((await dialog.innerText().catch(() => "")) || "").replace(/\s+/g, " ")
                : "";
            if (formText.includes(purchaseOrderNo)) return true;
            if (
                formReady &&
                salesOrderNo &&
                cardLabel.includes(salesOrderNo) &&
                ((await directForm.isVisible().catch(() => false)) ||
                    (await inboundForm.isVisible().catch(() => false)))
            ) {
                const openedDirect = await directForm.isVisible().catch(() => false);
                const openedInbound = await inboundForm.isVisible().catch(() => false);
                const wantedDirect = cardLabel.includes("直发") || formText.includes("供应商直发");
                const wantedInbound = cardLabel.includes("入库") || formText.includes("采购入库") || formText.includes(purchaseOrderNo);
                if ((openedDirect && wantedDirect) || (openedInbound && wantedInbound)) return true;
            }
            const ship = dialog.locator('[aria-label="公司仓发表单"]');
            // 仓储看不到销售单详情，仓发表单补不出 XS 号。任务卡上已有来源销售单，表单本身仍必须是公司仓发。
            if (
                salesOrderNo &&
                cardLabel.includes(salesOrderNo) &&
                (await ship.isVisible().catch(() => false))
            ) {
                return true;
            }
            const source = dialog.locator('[aria-label="来源单据"]');
            const sourceReady = await source.isVisible().catch(() => false);
            if (!sourceReady) continue;
            const sourceText = (await source.innerText()).replace(/\s+/g, " ");
            if (sourceText.includes(purchaseOrderNo)) return true;
            const otherPurchase = /PO-/.test(sourceText) && !sourceText.includes(purchaseOrderNo);
            if (
                salesOrderNo &&
                (await ship.isVisible().catch(() => false)) &&
                sourceText.includes(salesOrderNo) &&
                !otherPurchase
            ) {
                shipFallback = index;
            }
        }
        if (shipFallback == null) {
            await closeFulfillmentDialog(page);
            return false;
        }
        await closeFulfillmentDialog(page);
        await tasks.nth(shipFallback).click();
        await openFulfillmentWorkspaceForm(page);
        const dialog = page.getByRole("dialog", { name: "处理履约" });
        await expect(dialog.locator('[aria-label="公司仓发表单"]')).toBeVisible({ timeout: UI_TIMEOUT });
        const reopenedLabel = `${(await tasks.nth(shipFallback).getAttribute("aria-label")) ?? ""} ${(await tasks.nth(shipFallback).innerText().catch(() => "")) || ""}`;
        expect(reopenedLabel.includes(salesOrderNo) || (await dialog.innerText()).includes(salesOrderNo)).toBeTruthy();
        return true;
    };

    const openFulfillmentTask = async (page: Page, purchaseOrderNo: string) => {
        await page.goto("/workspace?family=fulfillment");
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await selectWorkspaceFamily(page, "fulfillment");
        if (await openListedFulfillment(page, purchaseOrderNo)) return;
        const managed = page.locator("#workspace-queue-scope-managed");
        if (
            (await managed.isVisible().catch(() => false)) &&
            (await managed.getAttribute("aria-pressed")) !== "true"
        ) {
            await managed.click();
            await expect(managed).toHaveAttribute("aria-pressed", "true", { timeout: UI_TIMEOUT });
            if (await openListedFulfillment(page, purchaseOrderNo)) return;
        }
        const labels = await page
            .getByRole("list", { name: "待办列表" })
            .locator("button")
            .evaluateAll((nodes) => nodes.map((node) => node.getAttribute("aria-label") || node.textContent || ""))
            .catch(() => [] as string[]);
        throw new Error(
            `未找到采购单 ${purchaseOrderNo} 的履约任务（销售单 ${salesOrderNo || "未知"}）\n现有: ${labels.filter(Boolean).join(" | ") || "（空）"}`,
        );
    };

    try {
        // 0) 采购责任默认调度人
        let page = await switchTo("admin");
        await ensureDefaultProcurementOwner(page);

        // 1) 销售：客户（货到付款，对照供应商先款）
        page = await switchTo("xiaoshou");
        await createCustomerViaUi(page, {
            legalName: customerName,
            shortName: `先款${stamp}`,
            creditCode: uniqueCreditCode(stamp),
            paymentTermLabel: PAYMENT_TERM_CUSTOMER,
        });
        await page.locator("#customers-directory-search").fill(`先款${stamp}`);
        await page.locator("#customers-directory-search").press("Enter");
        await expect(page.getByRole("link", { name: `先款${stamp}`, exact: true })).toBeVisible({ timeout: UI_TIMEOUT });

        // 2) 上传合同 PDF
        await gotoHeading(page, "/sales/contracts", /^合同$/);
        await page.locator("#page-actions-action-upload").click();
        const contractDialog = page.getByRole("dialog", { name: "上传合同 PDF" });
        await expect(contractDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await contractDialog.locator("#card-contracts-upload-pdf-input").setInputFiles(contractPdf());
        await contractDialog.locator("#card-contracts-upload-contract-no").fill(contractNo);
        await chooseOption(
            page,
            contractDialog.locator("#card-contracts-upload-customer"),
            customerName,
            customerName,
        );
        await expect(
            contractDialog.locator("#card-contracts-upload-settlement-party"),
        ).not.toHaveValue("", { timeout: UI_TIMEOUT });
        if (await contractDialog.locator("#card-contracts-upload-payment-terms").count()) {
            await chooseOption(
                page,
                contractDialog.locator("#card-contracts-upload-payment-terms"),
                PAYMENT_TERM_CUSTOMER,
                "货到",
            );
        }
        await contractDialog.locator("#card-contracts-upload-submit").click();
        await expectToast(page, "合同 PDF 已归档");
        await expect(contractDialog).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(page.getByText(contractNo).first()).toBeVisible({ timeout: UI_TIMEOUT });

        // 3) 销售单：龙井入仓 + 普洱直发，客户付款条件仍为货到
        await page.goto("/sales/orders?mode=create");
        await expect(page.getByRole("heading", { name: /新建销售单|业务信息/ })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByLabel("供应商")).toHaveCount(0);
        await expect(page.getByLabel("履约责任")).toHaveCount(0);
        await chooseOption(page, page.locator("#sales-orders-create-contract"), contractNo, contractNo);
        await expect(page.getByText(customerName).first()).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(
            page,
            page.locator("#sales-orders-create-header-welfare-scene"),
            "年节礼包",
            "年节",
        );
        await chooseOption(
            page,
            page.locator("#sales-orders-create-header-payment-terms"),
            PAYMENT_TERM_CUSTOMER,
            "货到",
        );
        await pickSku(page, "龙井", SKU_INBOUND);
        await pickSku(page, "普洱", SKU_DIRECT);
        await fillAllLineQuantities(page, SALES_QTY);
        await page.locator("#sales-orders-create-batch-due-date-open").click()
        await pickCalendarDay(page, page.locator("#sales-orders-create-batch-due-date"), dueDate);
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
        await expect(page.getByText(/审批中|审核中/).first()).toBeVisible({ timeout: UI_TIMEOUT });
        salesOrderNo = await readHeaderDocumentNumber(page)
        expect(salesOrderNo).toBeTruthy();
        await page.getByRole("tab", { name: /采购/ }).click();
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("待采购", {
            timeout: UI_TIMEOUT,
        });

        // 4) 负向：销售单未生效不得建采购单、不得履约
        page = await switchTo("caigou");
        await gotoHeading(page, "/procurement/orders", "采购单");
        const poSearch = page.locator("#procurement-orders-list-search");
        await poSearch.fill(salesOrderNo);
        await poSearch.press("Enter");
        await expect(page.getByText(/0 条|当前没有/)).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(
            page.locator("#procurement-orders-list-table").getByText(salesOrderNo),
        ).toHaveCount(0);

        await openWorkspaceTask(page, /销售单审批/, salesOrderNo, "approval");
        await expect(page.getByRole("button", { name: "预览供给分配" })).toHaveCount(0);
        await expect(page.getByText("供给来源 / 履约责任")).toHaveCount(0);
        await approveCurrentDocument(page);

        // 5) 供给分配：付款条件先款 50%；龙井入仓、普洱直发；立即提交两张采购单
        await openWorkspaceTask(page, /待供给分配/, salesOrderNo, "procurement");
        await expect(page.getByRole("heading", { name: "供给分配" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("销售明细与供给方案")).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByText(PAYMENT_TERM_SUPPLIER).first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await chooseSourcing(page, /龙井/, INBOUND_OPTION, WAREHOUSE_NAME);
        await chooseSourcing(page, /普洱/, DIRECT_OPTION);
        await expect(page.getByText("将创建采购单").locator("xpath=..")).toContainText("2 张");
        await expect(page.getByText("将建立库存预留").locator("xpath=..")).toContainText("0 条");
        await page.locator("#procurement-orders-create-preview").click();
        const preview = page.getByRole("dialog", { name: "预览供给分配" });
        await expect(preview).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(
            preview.getByText(/本次不占用现有库存|将为供给缺口创建|张采购单提交审批/),
        ).toBeVisible();
        await expect(preview.getByText("现有库存分配")).toHaveCount(0);
        await expect(preview.getByText(PAYMENT_TERM_SUPPLIER).first()).toBeVisible();
        const previewChoices = preview.getByRole("navigation", { name: "将创建的采购单" }).getByRole("button");
        await expect(previewChoices).toHaveCount(2);
        await previewChoices.filter({ hasText: WAREHOUSE_CODE }).click();
        await expect(preview.getByText("入仓", { exact: true })).toBeVisible();
        await previewChoices.filter({ hasNotText: WAREHOUSE_CODE }).click();
        await expect(preview.getByText("供应商直发", { exact: true })).toBeVisible();
        await preview.getByRole("button", { name: /确认提交 2 张采购单/ }).click();
        await expectToast(page, /供给分配已完成|已将缺口拆成 2 张采购单并提交审批/);

        await page.goto("/workspace");
        await page.locator("#workspace-queue-scope-started").click();
        await expect(page.getByText("采购单审批").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await assertNoPaymentApproval(page);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(page.getByText("已生效").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await page.getByRole("tab", { name: /采购/ }).click();
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("采购已覆盖", {
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByRole("table").getByText("草稿", { exact: true })).toHaveCount(0);

        // 6) 财务审批两张采购单生效，形成付款任务；履约仍被先款拦住
        page = await switchTo("caiwu");
        await approveMatchingTasks(page, "approval", /采购单审批/, salesOrderNo, 2);
        await page.goto("/workspace?family=finance");
        await expect(page.getByRole("button", { name: /供应商付款处理/ })).toHaveCount(0);
        await assertNoPaymentApproval(page);

        page = await switchTo("caigou");
        const purchases = await readPurchaseOrders(page, salesOrderId, salesOrderNo);
        inboundPo = purchases.find((row) => row.responsibility === "入仓")!.no;
        directPo = purchases.find((row) => row.responsibility === "供应商直发")!.no;

        // 7) 付款完成前：入库 / 代发不得确认；电子交付与服务任务不出现
        page = await switchTo("cangchu");
        await page.goto("/workspace?family=fulfillment");
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await selectWorkspaceFamily(page, "fulfillment");
        await expect(page.getByRole("button", { name: /电子交付|线下服务/ })).toHaveCount(0);
        await openFulfillmentTask(page, inboundPo);
        await expect(page.getByText(inboundPo).first()).toBeVisible();
        await assertFulfillmentCannotComplete(page, "入库");

        page = await switchTo("caigou");
        await page.goto("/workspace?family=fulfillment");
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await selectWorkspaceFamily(page, "fulfillment");
        await expect(page.getByRole("button", { name: /电子交付|线下服务/ })).toHaveCount(0);
        await openFulfillmentTask(page, directPo);
        // 采购单号是 PO- 加内部 id。履约来源摘要会丢掉这种单号，改用来源销售单和直发品名确认是这一张。
        await expect(page.locator('[aria-label="供应商直发表单"]')).toBeVisible();
        await expect(page.locator('[aria-label="来源单据"]')).toContainText(salesOrderNo);
        await expect(page.getByText(SKU_DIRECT).first()).toBeVisible();
        await assertFulfillmentCannotComplete(page, "代发");

        // 8) 出纳先付清入仓采购单：仅入库门禁放开，代发仍阻断
        page = await switchTo("fukuan");
        await page.goto("/workspace?family=approval");
        await assertNoPaymentApproval(page);
        inboundPaymentId = await payPurchaseOrder(page, inboundPo);
        await page.goto("/workspace?family=finance");
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await selectWorkspaceFamily(page, "finance");
        await expect(supplierPaymentTasks(page, inboundPo)).toHaveCount(0, {
            timeout: UI_TIMEOUT,
        });

        page = await switchTo("cangchu");
        await openFulfillmentTask(page, inboundPo);
        await expect(page.locator('[aria-label="入库表单"]')).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await fillReceiptDraft(page);
        await assertConfirmEnabled(page);

        page = await switchTo("caigou");
        await openFulfillmentTask(page, directPo);
        await expect(page.locator('[aria-label="供应商直发表单"]')).toBeVisible({ timeout: UI_TIMEOUT });
        await fillDirectDraft(page, trackingNo);
        await assertFulfillmentCannotComplete(page, "代发");

        // 9) 付清代发采购单后才能确认直发
        page = await switchTo("fukuan");
        directPaymentId = await payPurchaseOrder(page, directPo);
        await page.goto("/workspace?family=finance");
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await selectWorkspaceFamily(page, "finance");
        // 全量会留下其他单据的付款待办。本流程只要求这两张采购单的付款任务已关闭。
        await expect(supplierPaymentTasks(page, directPo)).toHaveCount(0, {
            timeout: UI_TIMEOUT,
        });
        await expect(supplierPaymentTasks(page, inboundPo)).toHaveCount(0, {
            timeout: UI_TIMEOUT,
        });
        await assertNoPaymentApproval(page);

        page = await switchTo("caigou");
        await openFulfillmentTask(page, directPo);
        await expect(page.locator('[aria-label="供应商直发表单"]')).toBeVisible({ timeout: UI_TIMEOUT });
        await fillDirectDraft(page, trackingNo);
        await assertConfirmEnabled(page);
        await expect(page.getByRole("heading", { name: "财务付款回单", exact: true })).toBeVisible({ timeout: UI_TIMEOUT });
        const workItemId = new URL(page.url()).searchParams.get("currentWorkItemId");
        expect(workItemId).toBeTruthy();
        const token = await apiToken("caigou");
        const receiptPath = `/admin/work-items/${workItemId}/payment-receipts`;
        const receipts = await apiGet<Array<{ document_id: string }>>(token, receiptPath);
        expect(receipts.map(receipt => receipt.document_id)).toEqual([directPaymentId]);
        expect(receipts.map(receipt => receipt.document_id)).not.toContain(inboundPaymentId);
        const wrongPurchaseReceipt = await page.request.get(
            `${API_BASE}${receiptPath}/${inboundPaymentId}/download`,
            { headers: { Authorization: `Bearer ${token}` } },
        );
        expect(wrongPurchaseReceipt.status(), await wrongPurchaseReceipt.text()).toBe(404);
        await page.locator("#fulfillment-operations-work-surface-confirm").click();
        await confirmFormal(page, "确认发货？", "确认发货");

        // 10) 入库确认后仓发
        page = await switchTo("cangchu");
        await openFulfillmentTask(page, inboundPo);
        await expect(page.locator('[aria-label="入库表单"]')).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await fillReceiptDraft(page);
        await assertConfirmEnabled(page);
        await page.locator("#fulfillment-operations-work-surface-confirm").click();
        await confirmFormal(page, "确认入库？", "确认入库");

        await openFulfillmentTask(page, inboundPo);
        await expect(page.locator('[aria-label="公司仓发表单"]')).toBeVisible({ timeout: UI_TIMEOUT });
        await addDeliveryTrackingEntry(page, {
            kind: "ship",
            lineIndex: 0,
            trackingNo: `WH${trackingNo}`,
            carrier: "顺丰速运",
        });
        await page.locator("#fulfillment-operations-work-surface-confirm").click();
        await confirmFormal(page, "确认发货？", "确认发货");

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("已生效").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByRole("button", { name: "过账" })).toHaveCount(0);
    } finally {
        await session?.context.close();
    }
});
