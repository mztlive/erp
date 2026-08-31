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
 * 文档-代码差异（测试以代码为准）:
 * - 文档写「先款后货时，付款完成后发货」；种子供应商狮峰茶叶为 PREPAY_50，
 *   门禁按有效已核销付款达到比例门槛即可履约，不必等应付全部结清。
 *   本流程仍由出纳把付款任务待付一次付清，同时满足任务完成与门槛。
 * - 文档 7.3.1 把「创建采购单」和「提交审批」画成两步；代码在供给分配确认同一事务内建单并立即提交。
 * - 客户侧本单用「货到 15 天」，与供应商「先款 50%」对照：客户账期不放开采购履约。
 * - 采购单列表硬编码履约责任为入仓、paymentGate 为 NOT_APPLICABLE；详情履约责任来自对象中心。
 * - 采购单对象中心未投影 prepayment_gate，概览不展示「先款后货门禁」卡片；W01 按单据精确加载时
 *   门禁默认 NOT_APPLICABLE（「无先款要求」）。真正拦截在确认命令：backend ensure_prepay_gate。
 * - W09 /fulfillment 只重定向到 W01；履约确认只在工作台原地处理。
 * - 履约主按钮是「确认入库 / 确认发货」，禁止用「过账」匹配。
 * - 开发目录无 VIRTUAL SKU（电子交付）；卡券 VOUCHER 不能走普通采购单。
 *   线下服务种子供应商安达为 POSTPAY_NET15，不会启用先款门禁。
 *   本流程用狮峰茶叶实物拆「入仓 + 供应商直发」覆盖入库与代发；电子交付/服务与入库/直发共用 ensure_prepay_gate。
 */
import fs from "node:fs";
import path from "node:path";
import { test, expect, type Browser, type BrowserContext, type Locator, type Page } from "@playwright/test";

import { ACCOUNTS } from "../helpers/accounts";
import { loginViaUi, newLoggedInContext } from "../helpers/login";

test.describe.configure({ mode: "serial" });

const UI_TIMEOUT = 20_000;
const FLOW_TIMEOUT = 12 * 60 * 1000;
const CONTRACT_PDF = path.resolve(process.cwd(), "fixtures/sample-contract.pdf");
const SKU_INBOUND = "狮峰明前龙井礼盒";
const SKU_DIRECT = "狮峰陈皮普洱礼盒";
const SUPPLIER_SHORT = "狮峰茶叶";
const WAREHOUSE_NAME = "北京通州仓";
const SALES_QTY = "1";
const PAYMENT_TERM_CUSTOMER = "货到 15 天";
const PAYMENT_TERM_SUPPLIER = "先款 50%";
const INBOUND_OPTION = `${SUPPLIER_SHORT} · 入仓`;
const DIRECT_OPTION = `${SUPPLIER_SHORT} · 供应商直发`;
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

function accountCred(login: LoginName): { account: string; password: string } {
    const bag = ACCOUNTS as Record<string, { account?: string; password?: string } | undefined>;
    const aliases: Record<LoginName, string[]> = {
        xiaoshou: ["xiaoshou", "sales"],
        caigou: ["caigou", "procurement"],
        cangchu: ["cangchu", "warehouse"],
        caiwu: ["caiwu", "finance"],
        fukuan: ["fukuan", "payment"],
        admin: ["admin"],
    };
    for (const key of aliases[login]) {
        const row = bag[key];
        if (row?.password) {
            return { account: row.account ?? login, password: row.password };
        }
    }
    for (const row of Object.values(bag)) {
        if (row?.account === login && row.password) {
            return { account: row.account, password: row.password };
        }
    }
    return { account: login, password: "123456" };
}

function asSession(raw: unknown): Session {
    if (raw && typeof raw === "object" && "page" in raw && "context" in raw) {
        const session = raw as Session;
        if (session.page && session.context) return session;
    }
    if (raw && typeof raw === "object" && "goto" in raw) {
        const page = raw as Page;
        return { context: page.context(), page };
    }
    throw new Error("newLoggedInContext 必须返回 { context, page } 或 Page");
}

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

async function openSession(browser: Browser, login: LoginName): Promise<Session> {
    const cred = accountCred(login);
    const raw = await newLoggedInContext(browser, cred);
    const session = asSession(raw);
    if (session.page.url().includes("/login")) {
        await loginViaUi(session.page, cred);
    }
    await session.page.goto("/workspace");
    await expect(session.page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    return session;
}

async function expectToast(page: Page, title: string | RegExp) {
    const toast = page.locator('[data-slot="toast"]').filter({ hasText: title });
    await expect(toast.first()).toBeVisible({ timeout: UI_TIMEOUT });
}

async function chooseOption(page: Page, input: Locator, option: string | RegExp, typed?: string) {
    await input.click();
    const query = typed ?? (typeof option === "string" ? option : "");
    if (query) {
        await input.fill("");
        await input.fill(query);
    }
    const listed = page.getByRole("option", { name: option }).first();
    if (await listed.count()) {
        await listed.click();
        return;
    }
    await page.locator('[data-slot="combobox-item"]').filter({ hasText: option }).first().click();
}

async function pickCalendarDay(page: Page, trigger: Locator, isoDate: string) {
    await trigger.click();
    const calendar = page.locator('[data-slot="calendar"]').last();
    await expect(calendar).toBeVisible({ timeout: UI_TIMEOUT });
    const byId = calendar.locator(`[id$="-day-${isoDate}"]`);
    if (await byId.count()) {
        await byId.first().click();
        return;
    }
    const target = new Date(`${isoDate}T00:00:00`);
    const year = target.getFullYear();
    const month = target.getMonth();
    const day = String(target.getDate());
    const monthTokens = [
        `${month + 1}月`,
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"][month]!,
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][month]!,
    ];
    for (let i = 0; i < 18; i += 1) {
        const caption = await calendar.innerText();
        const yearOk = caption.includes(String(year));
        const monthOk = monthTokens.some((token) => caption.includes(token));
        if (yearOk && monthOk) break;
        const next = calendar.getByRole("button", {
            name: /next month|go to the next month|下个月|下一月/i,
        });
        if (await next.count()) {
            await next.first().click();
        } else {
            await calendar.locator("button").last().click();
        }
    }
    const dayButtons = calendar.getByRole("button", { name: day, exact: true });
    const total = await dayButtons.count();
    for (let i = 0; i < total; i += 1) {
        const button = dayButtons.nth(i);
        const disabled = await button.getAttribute("aria-disabled");
        const outside = await button.getAttribute("data-outside");
        if (disabled === "true" || outside === "true") continue;
        await button.click();
        return;
    }
    await dayButtons.first().click();
}

async function gotoHeading(page: Page, href: string, heading: string | RegExp) {
    await page.goto(href);
    await expect(page.getByRole("heading", { name: heading })).toBeVisible({ timeout: UI_TIMEOUT });
}

async function searchWorkspace(page: Page, hint?: string) {
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    const refresh = page.locator("#workspace-home-refresh");
    if (await refresh.count()) await refresh.click();
    const search = page.locator("#workspace-queue-toolbar-search-input");
    if (hint && (await search.count())) {
        await search.fill(hint);
        await search.press("Enter");
    }
}

async function openWorkspaceTask(
    page: Page,
    family: "approval" | "procurement" | "fulfillment" | "finance",
    name: RegExp,
    hint?: string,
) {
    await page.goto(`/workspace?family=${family}`);
    await searchWorkspace(page, hint);
    const list = page.getByRole("list", { name: "待办列表" });
    await expect(list).toBeVisible({ timeout: UI_TIMEOUT });
    const task = list.getByRole("button", { name }).first();
    await expect(task).toBeVisible({ timeout: UI_TIMEOUT });
    await task.click();
    await expect(page.getByLabel(/当前/)).toBeVisible({ timeout: UI_TIMEOUT });
}

async function approveCurrentDocument(page: Page) {
    const approve = page.getByRole("button", { name: "通过", exact: true });
    await expect(approve).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(page.getByRole("button", { name: "驳回", exact: true })).toBeVisible();
    await expect(page.getByLabel("供给来源 / 履约责任")).toHaveCount(0);
    await expect(page.getByLabel("含税成本")).toHaveCount(0);
    await expect(page.getByLabel("预计交付日")).toHaveCount(0);
    await approve.click();
    const dialog = page.getByRole("dialog", { name: "确认通过" });
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT });
    await dialog.getByRole("button", { name: "确认通过" }).click();
    await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT });
}

async function approveMatchingTasks(
    page: Page,
    family: "approval" | "procurement" | "fulfillment" | "finance",
    name: RegExp,
    hint: string,
    expected: number,
) {
    for (let i = 0; i < expected; i += 1) {
        await openWorkspaceTask(page, family, name, hint);
        await approveCurrentDocument(page);
    }
    await page.goto(`/workspace?family=${family}`);
    await searchWorkspace(page, hint);
    const list = page.getByRole("list", { name: "待办列表" });
    if (await list.count()) {
        await expect(list.getByRole("button", { name })).toHaveCount(0, { timeout: UI_TIMEOUT });
        return;
    }
    await expect(page.getByText(/当前没有待处理事项|当前筛选没有待办/)).toBeVisible({
        timeout: UI_TIMEOUT,
    });
}

async function confirmFormal(page: Page, title: string | RegExp, confirmName: string | RegExp) {
    const dialog = page.getByRole("alertdialog").or(page.getByRole("dialog")).filter({ hasText: title });
    await expect(dialog.first()).toBeVisible({ timeout: UI_TIMEOUT });
    await dialog.getByRole("button", { name: confirmName }).click();
    await expect(dialog.first()).toBeHidden({ timeout: UI_TIMEOUT });
}

async function ensureDefaultProcurementOwner(page: Page) {
    await gotoHeading(page, "/master-data/procurement-responsibilities", "采购责任规则");
    if (await page.getByText("默认调度人").count()) return;
    const create = page.locator("#procurement-responsibility-rules-create");
    if (await create.count()) {
        await create.click();
    } else {
        await page.getByRole("button", { name: "新增规则" }).click();
    }
    const dialog = page.getByRole("dialog", { name: "新增采购责任规则" });
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT });
    await chooseOption(
        page,
        dialog.locator("#procurement-responsibility-rules-dialog-rule-type"),
        "默认调度人",
        "默认",
    );
    await chooseOption(
        page,
        dialog.locator("#procurement-responsibility-rules-dialog-owner"),
        /采购 · caigou|caigou/,
        "caigou",
    );
    const save = dialog.locator("#procurement-responsibility-rules-dialog-save");
    if (await save.count()) {
        await save.click();
    } else {
        await dialog.getByRole("button", { name: "保存规则" }).click();
    }
    await expectToast(page, /采购责任规则已新增|采购责任规则已更新/);
    await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT });
}

async function pickSku(page: Page, keyword: string, name: string) {
    await page.getByRole("button", { name: "选择商品" }).first().click();
    const skuDialog = page.getByRole("dialog", { name: /选择商品|更换销售商品/ });
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
    return page.locator("tr").filter({ hasText: item });
}

async function chooseSourcing(
    page: Page,
    item: string | RegExp,
    option: string,
    warehouse?: string,
) {
    const row = sourcingRow(page, item);
    await expect(row).toBeVisible({ timeout: UI_TIMEOUT });
    const sourcing = row.locator('[id$="-sourcing-option"]');
    await expect(sourcing).toBeVisible({ timeout: UI_TIMEOUT });
    await chooseOption(page, sourcing, option, option.includes("直发") ? "直发" : "入仓");
    if (warehouse) {
        const warehouseInput = row.locator('[id$="-warehouse"]');
        await expect(warehouseInput).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(page, warehouseInput, warehouse, "通州");
    } else {
        await expect(row.getByText("不适用")).toBeVisible({ timeout: UI_TIMEOUT });
    }
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
    const closed = page.getByRole("button", { name: "履约入口未开放" });
    const goFulfill = page.getByRole("link", { name: "去交付与代发" }).or(
        page.getByRole("button", { name: "去交付与代发" }),
    );
    expect((await closed.count()) + (await goFulfill.count())).toBeGreaterThan(0);
}

async function readPurchaseOrders(page: Page, salesOrderNo: string): Promise<PurchaseRef[]> {
    await gotoHeading(page, "/procurement/orders", "采购单");
    const search = page.locator("#procurement-orders-list-search");
    await search.fill(salesOrderNo);
    await search.press("Enter");
    await expect(page.getByText("2 条")).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(page.getByText("草稿")).toHaveCount(0);
    await expect(page.getByText(PAYMENT_TERM_SUPPLIER).first()).toBeVisible({ timeout: UI_TIMEOUT });
    const links = page.getByRole("link", { name: /打开采购单/ });
    await expect(links).toHaveCount(2, { timeout: UI_TIMEOUT });
    const hrefs = await links.evaluateAll((nodes) =>
        nodes
            .map((node) => (node as HTMLAnchorElement).getAttribute("href") ?? "")
            .filter(Boolean),
    );
    const refs: PurchaseRef[] = [];
    for (const href of hrefs) {
        await page.goto(href);
        await expect(page.getByText("采购单").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByText("已生效").first()).toBeVisible({ timeout: UI_TIMEOUT });
        const no = (await page.locator("span.num.text-foreground").first().innerText()).trim();
        expect(no.length).toBeGreaterThan(0);
        const responsibility: PurchaseRef["responsibility"] =
            (await page.getByText("供应商直发").count()) > 0 ? "供应商直发" : "入仓";
        await assertPurchaseOrderPrepayFacts(page, responsibility);
        refs.push({ no, responsibility });
    }
    const inbound = refs.filter((row) => row.responsibility === "入仓");
    const direct = refs.filter((row) => row.responsibility === "供应商直发");
    expect(inbound).toHaveLength(1);
    expect(direct).toHaveLength(1);
    return refs;
}

async function openFulfillmentTask(page: Page, salesOrderNo: string) {
    await page.goto("/workspace?family=fulfillment");
    await searchWorkspace(page, salesOrderNo);
    const list = page.getByRole("list", { name: "待办列表" });
    await expect(list.getByRole("button", { name: /履约处理/ })).toBeVisible({ timeout: UI_TIMEOUT });
    await list.getByRole("button", { name: /履约处理/ }).first().click();
    await expect(page.getByLabel("当前履约任务")).toBeVisible({ timeout: UI_TIMEOUT });
}

async function assertFulfillmentCannotComplete(page: Page, kind: "入库" | "代发") {
    if (kind === "入库") {
        await expect(page.getByLabel("入库表单").or(page.getByText("入库作业"))).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByLabel("供应商直发表单")).toHaveCount(0);
        await fillReceiptDraft(page);
    } else {
        await expect(page.getByLabel("供应商直发表单")).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByLabel("入库表单")).toHaveCount(0);
        await fillDirectDraft(page, `BLOCK${Date.now().toString().slice(-6)}`);
    }
    await expect(page.getByRole("button", { name: "过账" })).toHaveCount(0);
    const confirm = page.locator("#fulfillment-operations-work-surface-confirm");
    await expect(confirm).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(confirm).toHaveText(kind === "入库" ? "确认入库" : "确认发货");
    const gate = page.locator("#prepayment-gate");
    if (await gate.count()) {
        const allowed = await gate.getAttribute("data-allowed");
        if (allowed === "false") {
            await gate.hover();
            await expect(page.getByText(/先款未到|暂时不能|履约已阻断|作业先决条件尚未满足/)).toBeVisible({
                timeout: UI_TIMEOUT,
            });
        }
    }
    if (await confirm.isDisabled()) {
        await expect(confirm).toBeDisabled();
        return;
    }
    await confirm.click();
    const dialog = page
        .getByRole("alertdialog")
        .or(page.getByRole("dialog"))
        .filter({ hasText: kind === "入库" ? "确认入库" : "确认发货" });
    await expect(dialog.first()).toBeVisible({ timeout: UI_TIMEOUT });
    await dialog.getByRole("button", { name: kind === "入库" ? "确认入库" : "确认发货" }).click();
    await expect(page.getByText(/先款后货|未达.*门槛|请先完成付款|没有生效/)).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    await page.keyboard.press("Escape");
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
    await chooseOption(
        page,
        page.locator("#fulfillment-operations-direct-form-carrier"),
        "顺丰速运",
        "顺丰",
    );
    await page.locator("#fulfillment-operations-direct-form-tracking-no").fill(trackingNo);
}

async function payPurchaseOrder(page: Page, purchaseNo: string) {
    await page.goto("/workspace?family=finance");
    await searchWorkspace(page, purchaseNo);
    const list = page.getByRole("list", { name: "待办列表" });
    await expect(list.getByRole("button", { name: /供应商付款处理/ })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    await list.getByRole("button", { name: /供应商付款处理/ }).first().click();
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
    await payDialog.locator("#supplier-payables-payment-submit-confirm-confirm").click();
    await expectToast(page, /付款已登记/);
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

    const switchTo = async (login: LoginName) => {
        await session?.context.close();
        session = await openSession(browser, login);
        return session.page;
    };

    try {
        // 0) 采购责任默认调度人
        let page = await switchTo("admin");
        await ensureDefaultProcurementOwner(page);

        // 1) 销售：客户（货到付款，对照供应商先款）
        page = await switchTo("xiaoshou");
        await gotoHeading(page, "/sales/customers", "客户中心");
        await page.locator("#customers-directory-create").click();
        const customerDialog = page.getByRole("dialog", { name: "新建客户" });
        await expect(customerDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await customerDialog.locator("#customers-form-legal-name").fill(customerName);
        await customerDialog.locator("#customers-form-short-name").fill(`先款${stamp}`);
        await customerDialog.locator("#customers-form-credit-code").fill(uniqueCreditCode(stamp));
        await chooseOption(
            page,
            customerDialog.locator("#customers-form-payment-term"),
            PAYMENT_TERM_CUSTOMER,
            "货到",
        );
        await customerDialog.locator("#customers-form-submit").click();
        await expectToast(page, "客户已创建");
        await expect(customerDialog).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(page.getByText(customerName).first()).toBeVisible({ timeout: UI_TIMEOUT });

        // 2) 上传合同 PDF
        await gotoHeading(page, "/sales/contracts", "合同");
        await page.getByRole("button", { name: "上传合同 PDF" }).click();
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
        await expect(contractDialog.getByText(customerName).first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });
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
        await expect(page.getByText("单据头")).toBeVisible({ timeout: UI_TIMEOUT });
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
        await pickCalendarDay(page, page.locator("#sales-orders-create-batch-due-date"), dueDate);
        await page.locator("#sales-orders-create-batch-due-date-apply").click();
        await expectToast(page, "已批量设置交期");
        await expect(page.getByText("暂未确定采购负责人")).toHaveCount(0, { timeout: UI_TIMEOUT });
        await page.locator("#sales-orders-create-submit").click();
        const submitDialog = page.getByRole("dialog", { name: "提交销售单" });
        await expect(submitDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(submitDialog.getByText("审批中")).toBeVisible();
        await submitDialog.locator("#sales-orders-submit-confirm-confirm").click();
        await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, { timeout: UI_TIMEOUT });
        salesOrderId = page.url().split("/sales/orders/")[1]?.split("?")[0] ?? "";
        expect(salesOrderId).toBeTruthy();
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText(/审批中|审核中/).first()).toBeVisible({ timeout: UI_TIMEOUT });
        salesOrderNo = (await page.locator("span.num.text-foreground").first().innerText()).trim();
        expect(salesOrderNo).toBeTruthy();
        await expect(page.getByText(/采购单 0 笔/)).toBeVisible();

        // 4) 负向：销售单未生效不得建采购单、不得履约
        page = await switchTo("caigou");
        await gotoHeading(page, "/procurement/orders", "采购单");
        const poSearch = page.locator("#procurement-orders-list-search");
        await poSearch.fill(salesOrderNo);
        await poSearch.press("Enter");
        await expect(page.getByText(/0 条|当前没有/)).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByText(salesOrderNo)).toHaveCount(0);

        await openWorkspaceTask(page, "approval", /销售单审批/, salesOrderNo);
        await expect(page.getByRole("button", { name: "预览供给分配" })).toHaveCount(0);
        await expect(page.getByText("供给来源 / 履约责任")).toHaveCount(0);
        await approveCurrentDocument(page);

        // 5) 供给分配：付款条件先款 50%；龙井入仓、普洱直发；立即提交两张采购单
        await page.goto("/workspace?family=procurement");
        await openWorkspaceTask(page, "procurement", /待供给分配/, salesOrderNo);
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
        await expect(preview.getByText("本次不占用现有库存").or(preview.getByText("将按供应商创建 2 张采购单"))).toBeVisible();
        await expect(preview.getByText("现有库存分配")).toHaveCount(0);
        await expect(preview.getByText(PAYMENT_TERM_SUPPLIER).first()).toBeVisible();
        await expect(preview.getByText("入仓").first()).toBeVisible();
        await expect(preview.getByText("供应商直发").first()).toBeVisible();
        await preview.getByRole("button", { name: /确认提交 2 张采购单/ }).click();
        const confirmAlloc = page.getByRole("alertdialog").filter({ hasText: "确认供给分配" });
        await expect(confirmAlloc).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(confirmAlloc.getByText(/创建 2 张采购单提交审批/)).toBeVisible();
        await confirmAlloc.locator("#procurement-orders-create-confirm").click();
        await expectToast(page, /供给分配已完成|已将缺口拆成 2 张采购单并提交审批/);

        await page.goto("/workspace");
        await page.locator("#workspace-queue-scope-started").click();
        await expect(page.getByText("采购单审批").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await assertNoPaymentApproval(page);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(page.getByText("已生效").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByText(/采购单 2 笔/)).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByText("草稿")).toHaveCount(0);

        // 6) 财务审批两张采购单生效，形成付款任务；履约仍被先款拦住
        page = await switchTo("caiwu");
        await approveMatchingTasks(page, "approval", /采购单审批/, salesOrderNo, 2);
        await page.goto("/workspace?family=finance");
        await expect(page.getByRole("button", { name: /供应商付款处理/ })).toHaveCount(0);
        await assertNoPaymentApproval(page);

        page = await switchTo("caigou");
        const purchases = await readPurchaseOrders(page, salesOrderNo);
        inboundPo = purchases.find((row) => row.responsibility === "入仓")!.no;
        directPo = purchases.find((row) => row.responsibility === "供应商直发")!.no;

        // 7) 付款完成前：入库 / 代发不得确认；电子交付与服务任务不出现
        page = await switchTo("cangchu");
        await page.goto("/workspace?family=fulfillment");
        await searchWorkspace(page, salesOrderNo);
        await expect(page.getByRole("button", { name: /电子交付|线下服务/ })).toHaveCount(0);
        await openFulfillmentTask(page, salesOrderNo);
        await expect(page.getByText(customerName).first()).toBeVisible();
        await assertFulfillmentCannotComplete(page, "入库");

        page = await switchTo("caigou");
        await page.goto("/workspace?family=fulfillment");
        await searchWorkspace(page, salesOrderNo);
        await expect(page.getByRole("button", { name: /电子交付|线下服务/ })).toHaveCount(0);
        await openFulfillmentTask(page, salesOrderNo);
        await expect(page.getByText(customerName).first()).toBeVisible();
        await assertFulfillmentCannotComplete(page, "代发");

        // 8) 出纳先付清入仓采购单：仅入库门禁放开，代发仍阻断
        page = await switchTo("fukuan");
        await page.goto("/workspace?family=approval");
        await assertNoPaymentApproval(page);
        await payPurchaseOrder(page, inboundPo);
        await page.goto("/workspace?family=finance");
        await searchWorkspace(page, inboundPo);
        await expect(page.getByRole("button", { name: /供应商付款处理/ })).toHaveCount(0, {
            timeout: UI_TIMEOUT,
        });

        page = await switchTo("cangchu");
        await openFulfillmentTask(page, salesOrderNo);
        await expect(page.getByLabel("入库表单").or(page.getByText("入库作业"))).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await fillReceiptDraft(page);
        await assertConfirmEnabled(page);

        page = await switchTo("caigou");
        await openFulfillmentTask(page, salesOrderNo);
        await expect(page.getByLabel("供应商直发表单")).toBeVisible({ timeout: UI_TIMEOUT });
        await fillDirectDraft(page, trackingNo);
        await assertFulfillmentCannotComplete(page, "代发");

        // 9) 付清代发采购单后才能确认直发
        page = await switchTo("fukuan");
        await payPurchaseOrder(page, directPo);
        await page.goto("/workspace?family=finance");
        await searchWorkspace(page, salesOrderNo);
        await expect(page.getByRole("button", { name: /供应商付款处理/ })).toHaveCount(0, {
            timeout: UI_TIMEOUT,
        });
        await assertNoPaymentApproval(page);

        page = await switchTo("caigou");
        await openFulfillmentTask(page, salesOrderNo);
        await expect(page.getByLabel("供应商直发表单")).toBeVisible({ timeout: UI_TIMEOUT });
        await fillDirectDraft(page, trackingNo);
        await assertConfirmEnabled(page);
        await page.locator("#fulfillment-operations-work-surface-confirm").click();
        await confirmFormal(page, "确认发货？", "确认发货");

        // 10) 入库确认后仓发
        page = await switchTo("cangchu");
        await openFulfillmentTask(page, salesOrderNo);
        await expect(page.getByLabel("入库表单").or(page.getByText("入库作业"))).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await fillReceiptDraft(page);
        await assertConfirmEnabled(page);
        await page.locator("#fulfillment-operations-work-surface-confirm").click();
        await confirmFormal(page, "确认入库？", "确认入库");

        const shipForm = page.getByLabel("公司仓发表单");
        if (await shipForm.isVisible().catch(() => false)) {
            await chooseOption(
                page,
                page.locator("#fulfillment-operations-ship-form-carrier"),
                "顺丰速运",
                "顺丰",
            );
            await page.locator("#fulfillment-operations-ship-form-tracking-no").fill(`WH${trackingNo}`);
            await page.locator("#fulfillment-operations-work-surface-confirm").click();
            await confirmFormal(page, "确认发货？", "确认发货");
        } else {
            await page.goto("/workspace?family=fulfillment");
            await searchWorkspace(page, salesOrderNo);
            const shipTask = page.getByRole("list", { name: "待办列表" }).getByRole("button", { name: /履约处理/ });
            if (await shipTask.count()) {
                await shipTask.first().click();
                await expect(page.getByLabel("公司仓发表单")).toBeVisible({ timeout: UI_TIMEOUT });
                await chooseOption(
                    page,
                    page.locator("#fulfillment-operations-ship-form-carrier"),
                    "顺丰速运",
                    "顺丰",
                );
                await page
                    .locator("#fulfillment-operations-ship-form-tracking-no")
                    .fill(`WH${trackingNo}`);
                await page.locator("#fulfillment-operations-work-surface-confirm").click();
                await confirmFormal(page, "确认发货？", "确认发货");
            }
        }

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
