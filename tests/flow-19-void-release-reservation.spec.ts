/**
 * 流程: [flow-19] 生效后作废释放库存预占
 * 文档: docs/erp-phase-1.md §7.4（预占只能通过变更、作废、采购退货或库存调整释放）
 *       + §10.2（已出库事实不回退；本 spec 停在仓发出库前）
 *
 * 账号: admin（采购责任默认调度人） / cangchu（盘盈、仓发草稿核对） / caiwu（库存调整审批）
 *       xiaoshou（客户/合同/销售单/作废） / caigou（采购确认、供给分配）
 *
 * 文档-代码差异（以代码为准）:
 * 1. 文档允许「销售单作废」释放现有库存预占；销售单状态机只允许 DRAFT→VOIDED，
 *    EFFECTIVE 为终态，POST /admin/sales-orders/{id}/void 对已生效单会冲突。
 *    详情页没有「作废」按钮（与 flow-05 一致），本流程沿用该 HTTP 命令作废。
 * 2. 库存台账「明确不提供释放预占入口」；预占释放目前只在盘亏/损坏过账与仓发消耗。
 *    销售变更生效、销售单作废均未写预占释放。
 * 3. 发货单状态机只有 草稿→已发货（→已签收/已冲正），没有作废/关闭草稿；
 *    文档「仓发草稿关闭或作废」在代码里没有对应迁移。
 * 4. 库存调整 POSTED 徽标是「已过账」；盘盈/仓发按钮分别是「提交审批/确认提交」
 *    「确认发货」，不用「过账」匹配按钮。
 * 5. 盘盈必须挂已有 stock_balance（「请先建立期初或入库」）。空台账只插入数量 0
 *    的维度行，可用量仍由仓储盘盈 + 财务审批产生。
 * 6. 工作台审批类型文案按单据细分：销售单审批 / 库存调整单审批；供给分配为「待供给分配」。
 * 7. 销售单 related.fulfillments 前端写死 0，不能用「交付 N 笔」判断仓发草稿；
 *    仓发草稿以 W01「履约处理」+ GET /admin/deliveries 为准。
 */
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import {
    test,
    expect,
    type Browser,
    type BrowserContext,
    type Locator,
    type Page,
} from "@playwright/test";

import { ACCOUNTS } from "../helpers/accounts";
import { apiGet, apiLogin } from "../helpers/api";
import { loginViaUi, newLoggedInContext } from "../helpers/login";
import "../helpers/ui";

test.use({ viewport: { width: 1440, height: 900 } });
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

const VOID_HTTP_NOTE =
    "销售单详情无「作废」按钮；已生效作废走 POST /admin/sales-orders/{id}/void（文档 §7.4，代码目前只允许草稿）";

type LoginName = "xiaoshou" | "caigou" | "cangchu" | "caiwu" | "admin";
type Session = { context: BrowserContext; page: Page };

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

function accountCred(login: LoginName): { account: string; password: string } {
    const bag = ACCOUNTS as Record<
        string,
        { account?: string; password?: string } | undefined
    >;
    const aliases: Record<LoginName, string[]> = {
        xiaoshou: ["xiaoshou", "sales"],
        caigou: ["caigou", "procurement"],
        cangchu: ["cangchu", "warehouse"],
        caiwu: ["caiwu", "finance"],
        admin: ["admin"],
    };
    for (const key of aliases[login]) {
        const row = bag[key];
        if (row?.password) {
            return { account: row.account ?? login, password: row.password };
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

async function openSession(browser: Browser, login: LoginName): Promise<Session> {
    const cred = accountCred(login);
    let raw: unknown;
    try {
        raw = await newLoggedInContext(browser, cred as never);
    } catch {
        raw = await newLoggedInContext(browser, cred.account as never);
    }
    const session = asSession(raw);
    if (session.page.url().includes("/login")) {
        await loginViaUi(session.page, cred as never);
    }
    await session.page.goto("/workspace");
    await expect(session.page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    return session;
}

function orderTitleRow(page: Page, customerName: string) {
    return page.getByRole("heading", { name: customerName }).locator("xpath=..");
}

async function expectToast(page: Page, title: string | RegExp) {
    const toast = page.locator('[data-slot="toast-title"]').filter({ hasText: title });
    await expect(toast.first()).toBeVisible({ timeout: UI_TIMEOUT });
}

async function chooseOption(page: Page, input: Locator, option: string | RegExp) {
    await input.click();
    if (typeof option === "string") {
        await input.fill(option);
    }
    const listed = page
        .getByRole("option", { name: option })
        .or(page.locator('[data-slot="combobox-item"]').filter({ hasText: option }))
        .first();
    await expect(listed).toBeVisible({ timeout: UI_TIMEOUT });
    await listed.click();
}

async function pickCalendarDay(page: Page, trigger: Locator, isoDate: string) {
    await trigger.click();
    const calendar = page.locator('[data-slot="calendar"]').last();
    await expect(calendar).toBeVisible({ timeout: UI_TIMEOUT });
    const target = new Date(`${isoDate}T00:00:00`);
    const year = target.getFullYear();
    const month = target.getMonth();
    const day = String(target.getDate());
    const monthTokens = [
        `${month + 1}月`,
        [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ][month]!,
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][
            month
        ]!,
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

async function openWorkspaceTask(
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
        await page.locator(`#workspace-family-nav-${family}`).click();
    }
    const search = page.locator("#workspace-queue-toolbar-search-input");
    if (hint && (await search.count())) {
        await search.fill(hint);
        await search.press("Enter");
    }
    const matcher = hint
        ? new RegExp(`${typeLabel}[\\s\\S]*${hint}|${hint}[\\s\\S]*${typeLabel}`)
        : new RegExp(typeLabel);
    const task = page.getByRole("button", { name: matcher }).first();
    await expect(task).toBeVisible({ timeout: UI_TIMEOUT });
    await task.click();
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
        await page.locator(`#workspace-family-nav-${family}`).click();
    }
    const search = page.locator("#workspace-queue-toolbar-search-input");
    if (hint && (await search.count())) {
        await search.fill(hint);
        await search.press("Enter");
    }
    const matcher = hint
        ? new RegExp(`${typeLabel}[\\s\\S]*${hint}|${hint}[\\s\\S]*${typeLabel}`)
        : new RegExp(typeLabel);
    await expect(page.getByRole("button", { name: matcher })).toHaveCount(0, {
        timeout: UI_TIMEOUT,
    });
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

async function confirmFormal(
    page: Page,
    title: string | RegExp,
    confirmName: string | RegExp,
) {
    const dialog = page
        .getByRole("alertdialog")
        .or(page.getByRole("dialog"))
        .filter({ hasText: title });
    await expect(dialog.first()).toBeVisible({ timeout: UI_TIMEOUT });
    await dialog.getByRole("button", { name: confirmName }).click();
    await expect(dialog.first()).toBeHidden({ timeout: UI_TIMEOUT });
}

async function ensureDefaultProcurementOwner(page: Page) {
    await page.goto("/master-data/procurement-responsibilities");
    await expect(page.getByRole("heading", { name: "采购责任规则" })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    if (await page.getByText("默认调度人").count()) {
        return;
    }
    await page.getByRole("button", { name: "新增规则" }).click();
    const dialog = page.getByRole("dialog", { name: "新增采购责任规则" });
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT });
    await chooseOption(
        page,
        dialog.locator("#procurement-responsibility-rules-dialog-rule-type"),
        "默认调度人",
    );
    await chooseOption(
        page,
        dialog.locator("#procurement-responsibility-rules-dialog-owner"),
        /采购/,
    );
    await dialog.getByRole("button", { name: "保存规则" }).click();
    await expectToast(page, /采购责任规则已新增|采购责任规则已更新/);
    await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT });
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

function reservationRow(page: Page, salesOrderNo: string): Locator {
    return page
        .locator("#inventory-ledger-reservation-table")
        .getByRole("row")
        .filter({ hasText: SKU_CODE })
        .filter({ hasText: salesOrderNo });
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

async function tokenOf(login: LoginName): Promise<string> {
    const cred = accountCred(login);
    try {
        return await apiLogin(cred.account as never);
    } catch {
        return await apiLogin(login as never);
    }
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

function mongoSettings(): { uri: string; dbName: string } {
    const configPath = path.join(process.cwd(), "backend", "config.toml");
    const raw = execFileSync(
        "python3",
        [
            "-c",
            "import pathlib, tomllib, json, sys; cfg=tomllib.loads(pathlib.Path(sys.argv[1]).read_bytes().decode()); print(json.dumps({'uri': cfg['database']['uri'], 'dbName': cfg['database']['db_name']}))",
            configPath,
        ],
        { encoding: "utf8" },
    ).trim();
    return JSON.parse(raw) as { uri: string; dbName: string };
}

async function ensureZeroBalanceDimension() {
    const token = await tokenOf("admin");
    const warehouses = await apiGet<
        ApiPage<{ id: string; warehouse_code?: string }>
    >(token, "/admin/warehouses", {
        page: 1,
        page_size: 50,
        sort_by: "warehouse_code",
        sort_dir: "asc",
    } as never);
    const warehouse = (warehouses.items ?? []).find(
        (row) => row.warehouse_code === WAREHOUSE_CODE,
    );
    const skus = await apiGet<ApiPage<{ id: string; sku_no?: string }>>(
        token,
        "/admin/skus",
        { q: SKU_CODE, page: 1, page_size: 20, sort_by: "sku_no", sort_dir: "asc" } as never,
    );
    const sku = (skus.items ?? []).find((row) => row.sku_no === SKU_CODE);
    if (!warehouse || !sku) {
        throw new Error(`主数据缺少仓库 ${WAREHOUSE_CODE} 或 SKU ${SKU_CODE}`);
    }
    const existing = (await listBalances(await tokenOf("cangchu"))).find(
        (row) =>
            (row.sku_code === SKU_CODE || row.sku_id === sku.id) &&
            (row.warehouse_code === WAREHOUSE_CODE || row.warehouse_id === warehouse.id),
    );
    if (existing) return;

    const now = Math.floor(Date.now() / 1000);
    const id = `${now.toString(16)}${"0".repeat(24)}`.slice(0, 24);
    const { uri, dbName } = mongoSettings();
    const script = `
      const dbx = db.getSiblingDB(${JSON.stringify(dbName)});
      const existing = dbx.stock_balances.findOne({
        warehouse_id: ${JSON.stringify(warehouse.id)},
        sku_id: ${JSON.stringify(sku.id)},
        deleted_at: 0
      });
      if (!existing) {
        dbx.stock_balances.insertOne({
          id: ${JSON.stringify(id)},
          version: NumberLong(1),
          created_at: NumberLong(${now}),
          updated_at: NumberLong(${now}),
          deleted_at: NumberLong(0),
          warehouse_id: ${JSON.stringify(warehouse.id)},
          sku_id: ${JSON.stringify(sku.id)},
          on_hand_quantity: NumberDecimal("0"),
          reserved_quantity: NumberDecimal("0"),
          available_quantity: NumberDecimal("0"),
          last_movement_id: null
        });
      }
    `;
    execFileSync("mongosh", ["--norc", "--quiet", uri, "--eval", script], {
        stdio: "pipe",
        timeout: 30_000,
    });
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
 * 详情页没有作废按钮。沿用 flow-05 的 HTTP 命令；文档要求对已生效单释放预占。
 */
async function voidEffectiveSalesOrder(page: Page, salesOrderId: string) {
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
    expect(
        response.ok(),
        `${VOID_HTTP_NOTE}; HTTP ${response.status()} ${bodyText}`,
    ).toBeTruthy();
    await page.reload();
}

test("flow-19 现有库存预占后作废已生效销售单：预占释放、仓发草稿不再履约、零采购单", async ({
    browser,
}) => {
    test.setTimeout(FLOW_TIMEOUT);
    const stamp = Date.now().toString(36).toUpperCase();
    const customerName = `E2E作废释放客户${stamp}`;
    const contractNo = `HT-E2E-VOID-${stamp}`;
    const dueDate = plusDaysIso(21);
    let session: Session | undefined;
    let salesOrderId = "";
    let salesOrderNo = "";

    const switchTo = async (login: LoginName) => {
        await session?.context.close();
        session = await openSession(browser, login);
        return session.page;
    };

    try {
        // 0) 采购责任默认调度人：提交实物销售单前必须能解析采购负责人
        let page = await switchTo("admin");
        await ensureDefaultProcurementOwner(page);
        await ensureZeroBalanceDimension();

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
        await confirmFormal(page, /确认提交库存调整|提交库存调整/, "确认提交");
        await expectToast(page, "调整已提交审批");
        await expect(adjustDialog).toBeHidden({ timeout: UI_TIMEOUT });

        // 2) caiwu 审批库存调整，余额才增加
        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "库存调整单审批|单据审批", undefined, "approval");
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
        await page.goto("/sales/customers");
        await expect(page.getByRole("heading", { name: "客户中心" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#customers-directory-create").click();
        const customerDialog = page.getByRole("dialog", { name: "新建客户" });
        await expect(customerDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await customerDialog.locator("#customers-form-legal-name").fill(customerName);
        await customerDialog.locator("#customers-form-short-name").fill(`作废释放${stamp}`);
        await customerDialog
            .locator("#customers-form-credit-code")
            .fill(uniqueCreditCode(stamp));
        await chooseOption(
            page,
            customerDialog.locator("#customers-form-payment-term"),
            "货到 15 天",
        );
        await customerDialog.locator("#customers-form-submit").click();
        await expectToast(page, "客户已创建");
        await expect(customerDialog).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(page.getByText(customerName).first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });

        await page.goto("/sales/contracts");
        await expect(page.getByRole("heading", { name: "合同" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.getByRole("button", { name: "上传合同 PDF" }).click();
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
        await expect(page.getByRole("heading", { name: "销售单" })).toBeVisible({
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
        await page.getByRole("button", { name: "选择商品" }).click();
        const skuDialog = page.getByRole("dialog", { name: "选择商品" });
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
        await pickCalendarDay(
            page,
            page.locator("#sales-orders-create-batch-due-date"),
            dueDate,
        );
        await page.locator("#sales-orders-create-batch-due-date-apply").click();
        await expectToast(page, "已批量设置交期");
        await expect(page.getByText("暂未确定采购负责人")).toHaveCount(0, {
            timeout: UI_TIMEOUT,
        });
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
        await expect(
            orderTitleRow(page, customerName).getByText(/审批中|审核中/),
        ).toBeVisible({ timeout: UI_TIMEOUT });
        salesOrderNo = (
            await page.locator("span.num.text-foreground").first().innerText()
        ).trim();
        expect(salesOrderNo).toBeTruthy();
        await expect(page.getByText(/采购单 0 笔/)).toBeVisible();
        await expect(page.locator("#sales-orders-detail-start-change")).toBeDisabled();
        await expect(page.getByRole("button", { name: /作废/ })).toHaveCount(0);

        // 4) caigou 采购确认：只通过，不选源
        page = await switchTo("caigou");
        await openWorkspaceTask(page, "销售单审批|单据审批", salesOrderNo, "approval");
        await expect(page.getByRole("button", { name: "预览供给分配" })).toHaveCount(0);
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
        await expect(previewDialog.getByText("本次无需创建采购单")).toBeVisible();
        await expect(
            previewDialog.getByText("本次全部由现有库存满足，不会创建采购单。"),
        ).toBeVisible();
        await expect(previewDialog.getByText(/确认提交 \d+ 张采购单/)).toHaveCount(0);
        await previewDialog.locator("#procurement-orders-create-preview-confirm").click();
        await expect(page.getByRole("heading", { name: "确认供给分配" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(
            page.getByText(/现有库存已满足本次分配，无需创建采购单/),
        ).toBeVisible();
        await page.locator("#procurement-orders-create-confirm").click();
        await expectToast(
            page,
            /供给分配已完成|已从现有库存建立 \d+ 条销售预留并生成仓发草稿，无需采购/,
        );

        // 6) 负向：零张采购单；库存 available 减少、reserved 增加；仓发草稿已形成但尚未出库
        page = await switchTo("caigou");
        await page.goto("/procurement/orders");
        await expect(page.getByRole("heading", { name: "采购单" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("暂无采购单")).toBeVisible({ timeout: UI_TIMEOUT });
        expect(await listPurchaseOrders(await tokenOf("caigou"))).toHaveLength(0);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(page.getByText(/采购单 0 笔/)).toBeVisible({ timeout: UI_TIMEOUT });
        await page.getByRole("tab", { name: /采购/ }).click();
        await expect(page.getByText("本单还没有采购单。")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("改单中")).toHaveCount(0);
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toHaveCount(0);

        page = await switchTo("cangchu");
        await assertBalanceNumbers(page, {
            onHand: GAIN_QTY,
            reserved: SALE_QTY,
            available: AFTER_RESERVE_AVAILABLE,
        });
        await expect(balanceRow(page)).toContainText("有预占");
        await page.locator("#inventory-ledger-view-reservation").click();
        const activeReservation = reservationRow(page, salesOrderNo);
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
        await expect(page.getByLabel("公司仓发表单")).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByRole("button", { name: "通过" })).toHaveCount(0);
        await expect(page.getByRole("button", { name: "过账" })).toHaveCount(0);
        await expect(
            page.locator("#fulfillment-operations-work-surface-confirm"),
        ).toBeVisible({ timeout: UI_TIMEOUT });
        // 本 spec 停在出库前：不点确认发货

        // 7) xiaoshou 作废已生效销售单（无 UI 按钮，走 HTTP）
        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await voidEffectiveSalesOrder(page, salesOrderId);
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("已作废")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("本单已作废，不再进入履约或结案。")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toHaveCount(0);
        await expect(page.locator("#sales-orders-detail-start-change")).toBeDisabled();
        await expect(page.getByText("改单中")).toHaveCount(0);
        await expect(page.getByRole("button", { name: /作废/ })).toHaveCount(0);
        await expect(page.getByText(/采购单 0 笔/)).toBeVisible();
        const voided = await fetchSalesOrder(page, salesOrderId);
        expect(
            String(voided.commercial_status ?? voided.commercialStatus ?? "").toUpperCase(),
        ).toBe("VOIDED");

        // 8) 预占释放、available 恢复、账面现存不变（未出库，事实不回退）
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
            .toBe(`${GAIN_QTY}/0/${GAIN_QTY}`);
        await assertBalanceNumbers(page, {
            onHand: GAIN_QTY,
            reserved: "0",
            available: GAIN_QTY,
        });
        await page.locator("#inventory-ledger-view-reservation").click();
        const released = reservationRow(page, salesOrderNo);
        await expect(released).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(released).toContainText("已释放");
        await expect(released).not.toContainText("有效");
        await expect(released).not.toContainText("已消耗");
        await page.locator("#inventory-ledger-view-movement").click();
        await expect(page.getByText("仓库发出")).toHaveCount(0);
        await expect(page.getByText("仓发出库")).toHaveCount(0);
        await expect(page.getByText("采购入库")).toHaveCount(0);

        const reservations = await listReservations(await tokenOf("cangchu"));
        expect(
            reservations.some((row) => String(row.status).toUpperCase() === "RELEASED"),
        ).toBe(true);
        expect(
            reservations.some((row) => String(row.status).toUpperCase() === "ACTIVE"),
        ).toBe(false);

        // 9) 仓发草稿关闭或作废，不得再履约；已出库事实不存在故无需回退
        const deliveriesAfterVoid = await listDeliveries(
            await tokenOf("cangchu"),
            salesOrderId,
        );
        expect(
            deliveriesAfterVoid.some((row) => row.status === "SHIPPED"),
            "出库前作废不得出现已发货事实",
        ).toBe(false);
        expect(
            deliveriesAfterVoid.filter((row) => row.status === "DRAFT"),
            "仓发草稿应关闭或作废，不得仍为可确认草稿",
        ).toHaveLength(0);
        await expectNoWorkspaceTask(page, "履约处理", customerName, "fulfillment");
        await expectNoWorkspaceTask(page, "履约处理", salesOrderNo, "fulfillment");

        // 10) 全程不得建采购单、不得关闭、不得开变更单、作废本身不得出现审批实例
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
        await expect(orderTitleRow(page, customerName).getByText("已作废")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.locator("#sales-orders-detail-start-change")).toBeDisabled();
        await expect(page.getByText("已关闭")).toHaveCount(0);
        await expect(page.getByRole("button", { name: "过账" })).toHaveCount(0);
        await expectNoWorkspaceTask(page, "客户验收登记", salesOrderNo, "fulfillment");
        await expectNoWorkspaceTask(page, "销售变更单审批", salesOrderNo, "approval");
    } finally {
        await session?.context.close();
    }
});
