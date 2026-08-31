/**
 * 流程: [flow-11] 现有库存仓发（零采购单）
 * 文档: docs/erp-phase-1.md §7.2 / §7.4（现有库存直配跳过采购/付款/入库）
 *
 * 账号: admin（采购责任默认调度人） / cangchu（盘盈、仓发） / caiwu（库存调整审批）
 *       xiaoshou（客户/合同/销售单/验收） / caigou（采购确认、供给分配）
 *
 * 文档-代码差异（以代码为准）:
 * 1. 文档允许纯库存覆盖零张采购单；供给分配预览主按钮为「确认库存分配」，
 *    成功 toast「已从现有库存建立 N 条销售预留并生成仓发草稿，无需采购。」
 * 2. 库存调整状态徽标是「已过账」；提交按钮是「提交审批 / 确认提交」，
 *    仓发按钮是「确认发货」，不用「过账」匹配按钮。
 * 3. 盘盈不能作为第一笔流水：创建/过账库存调整都要求已有 stock_balance
 *    （「请先建立期初或入库」）。空台账文案指向「导入与期初」，但导入页空态
 *    没有新建批次入口。本流程仍走盘盈；若台账无余额行会失败（见 risks）。
 * 4. 工作台队列类型：flow-01 用「待供给分配」搜索；后端 WorkItemType label
 *    为「供给分配」。选任务兼容两者。
 * 5. 销售单提交后页头徽标可能是「审批中」或「审核中」；商业状态生效后为「已生效」。
 * 6. 采购确认只通过/驳回，不选供给；选源只在生效后的供给分配。
 */
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
import { loginViaUi, newLoggedInContext } from "../helpers/login";

const UI_TIMEOUT = 20_000;
const FLOW_TIMEOUT = 12 * 60 * 1000;
const SKU_CODE = "TEA-SF-LJ-250";
const SKU_NAME = "狮峰明前龙井礼盒";
const WAREHOUSE_NAME = "北京通州仓";
const GAIN_QTY = "10";
const SALE_QTY = "2";
const AFTER_RESERVE_AVAILABLE = "8";

const MINIMAL_PDF = Buffer.from(
    "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R>>endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000068 00000 n \n0000000125 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n210\n%%EOF\n",
);

type LoginName = "xiaoshou" | "caigou" | "cangchu" | "caiwu" | "fukuan" | "admin";
type Session = { context: BrowserContext; page: Page };

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
        fukuan: ["fukuan", "payment"],
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
    const listed = page.getByRole("option", { name: option }).first();
    if (await listed.count()) {
        await listed.click();
        return;
    }
    await page
        .locator('[data-slot="combobox-item"]')
        .filter({ hasText: option })
        .first()
        .click();
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

async function openWorkspaceTask(page: Page, typeLabel: string, hint?: string) {
    await page.goto("/workspace");
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    const search = page.locator("#workspace-queue-toolbar-search-input");
    if (hint && (await search.count())) {
        await search.fill(hint);
        await search.press("Enter");
    }
    const type = `(?:${typeLabel})`;
    const matcher = hint
        ? new RegExp(`${type}[\\s\\S]*${hint}|${hint}[\\s\\S]*${type}`)
        : new RegExp(type);
    const task = page.getByRole("button", { name: matcher }).first();
    await expect(task).toBeVisible({ timeout: UI_TIMEOUT });
    await task.click();
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
    const raw = `91${stamp.replace(/[^0-9A-Za-z]/g, "").toUpperCase()}E2ESTOCKWH`;
    return raw.slice(0, 18).padEnd(18, "0");
}

function contractPdf(): { name: string; mimeType: string; buffer: Buffer } {
    const fixture = path.resolve(process.cwd(), "fixtures/sample-contract.pdf");
    if (fs.existsSync(fixture)) {
        return {
            name: "sample-contract.pdf",
            mimeType: "application/pdf",
            buffer: fs.readFileSync(fixture),
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
        .filter({ hasText: new RegExp(`${WAREHOUSE_NAME}|BJ-TZ-01`) });
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

test.describe.configure({ mode: "serial" });

test("flow-11 现有库存仓发：盘盈 → 销售生效 → 纯库存供给分配 → 仓发 → 验收，零采购单", async ({
    browser,
}) => {
    test.setTimeout(FLOW_TIMEOUT);
    const stamp = Date.now().toString(36).toUpperCase();
    const customerName = `E2E库存仓发客户${stamp}`;
    const contractNo = `HT-E2E-STK-${stamp}`;
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

        // 1) cangchu 盘盈准备指定仓库 + SKU 可用库存
        page = await switchTo("cangchu");
        await searchInventory(page, SKU_CODE);
        if (await page.getByText("当前仓库尚无 ERP 自有库存记录").count()) {
            throw new Error(
                "库存台账无余额行：后端盘盈必须挂已有 stock_balance（请先建立期初或入库），导入页也没有新建批次入口。无法按文档仅用盘盈准备第一笔可用库存。",
            );
        }
        const row = balanceRow(page);
        await expect(row).toBeVisible({ timeout: UI_TIMEOUT });
        await row.getByRole("button", { name: "库存调整" }).click();
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
            .fill("flow-11 现有库存仓发盘盈");
        await adjustDialog.locator("#inventory-adjustment-dialog-submit").click();
        await confirmFormal(page, /确认提交库存调整|提交库存调整/, "确认提交");
        await expectToast(page, "调整已提交审批");
        await expect(adjustDialog).toBeHidden({ timeout: UI_TIMEOUT });

        // 2) caiwu 审批库存调整，余额才增加
        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "单据审批");
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
        await customerDialog.locator("#customers-form-short-name").fill(`库存仓发${stamp}`);
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

        // 4) caigou 采购确认：只通过，不选源
        page = await switchTo("caigou");
        await openWorkspaceTask(page, "单据审批", salesOrderNo);
        await expect(page.getByRole("button", { name: "预览供给分配" })).toHaveCount(0);
        await approveCurrentDocument(page);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible({
            timeout: UI_TIMEOUT,
        });

        // 5) caigou 供给分配：全部走现有库存，确认后零张采购单
        page = await switchTo("caigou");
        await page.getByRole("button", { name: "刷新" }).click().catch(() => undefined);
        await openWorkspaceTask(page, "待供给分配|供给分配", salesOrderNo);
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
        await expect(page.getByText(/现有库存已满足本次分配，无需创建采购单/)).toBeVisible();
        await page.locator("#procurement-orders-create-confirm").click();
        await expectToast(
            page,
            /供给分配已完成|已从现有库存建立 \d+ 条销售预留并生成仓发草稿，无需采购/,
        );

        // 6) 负向：零张采购单；库存 available 减少、reserved 增加；不得出现入库
        page = await switchTo("caigou");
        await page.goto("/procurement/orders");
        await expect(page.getByRole("heading", { name: "采购单" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("暂无采购单")).toBeVisible({ timeout: UI_TIMEOUT });

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(page.getByText(/采购单 0 笔/)).toBeVisible({ timeout: UI_TIMEOUT });
        await page.getByRole("tab", { name: /采购/ }).click();
        await expect(page.getByText("本单还没有采购单。")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("改单中")).toHaveCount(0);

        page = await switchTo("cangchu");
        await assertBalanceNumbers(page, {
            onHand: GAIN_QTY,
            reserved: SALE_QTY,
            available: AFTER_RESERVE_AVAILABLE,
        });
        await expect(balanceRow(page)).toContainText("有预占");
        await page.locator("#inventory-ledger-view-reservation").click();
        const reservation = page
            .locator("#inventory-ledger-reservation-table")
            .getByRole("row")
            .filter({ hasText: SKU_CODE })
            .filter({ hasText: salesOrderNo });
        await expect(reservation).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(reservation).toContainText("有效");
        await expect(page.getByRole("button", { name: /入库/ })).toHaveCount(0);

        // 7) cangchu 仓发出库消耗预占（Delivery 无审批）
        await openWorkspaceTask(page, "履约处理", customerName);
        await expect(page.getByLabel("公司仓发表单")).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByRole("button", { name: "通过" })).toHaveCount(0);
        await expect(page.getByRole("button", { name: "过账" })).toHaveCount(0);
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
                await shipQty.fill(SALE_QTY);
            }
        }
        await page.locator("#fulfillment-operations-work-surface-confirm").click();
        await confirmFormal(page, "确认发货？", "确认发货");

        await assertBalanceNumbers(page, {
            onHand: AFTER_RESERVE_AVAILABLE,
            reserved: "0",
            available: AFTER_RESERVE_AVAILABLE,
        });
        await page.locator("#inventory-ledger-view-reservation").click();
        const consumed = page
            .locator("#inventory-ledger-reservation-table")
            .getByRole("row")
            .filter({ hasText: SKU_CODE })
            .filter({ hasText: salesOrderNo });
        await expect(consumed).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(consumed).toContainText("已消耗");
        await page.locator("#inventory-ledger-view-movement").click();
        await expect(page.getByText("库存调整").first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("仓库发出").first()).toBeVisible();
        await expect(page.getByText("采购入库")).toHaveCount(0);

        // 8) xiaoshou 登记客户验收（无审批）；未回款不得关闭、不得开变更单
        page = await switchTo("xiaoshou");
        await openWorkspaceTask(page, "客户验收登记", salesOrderNo);
        await page.locator("#sales-orders-acceptance-register-open").click();
        const acceptanceDialog = page.getByRole("dialog", { name: "登记客户验收" });
        await expect(acceptanceDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(acceptanceDialog.getByRole("button", { name: "通过" })).toHaveCount(0);
        await acceptanceDialog.locator("#sales-orders-acceptance-register-submit").click();
        await confirmFormal(page, "确认客户验收", "确认本次验收");

        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toHaveCount(0);
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible();
        await expect(page.locator("#sales-orders-detail-start-change")).toBeDisabled();
        await expect(page.getByText("改单中")).toHaveCount(0);

        // 9) 全程不得出现采购单、供应商付款、采购入库
        page = await switchTo("caigou");
        await page.goto("/procurement/orders");
        await expect(page.getByText("暂无采购单")).toBeVisible({ timeout: UI_TIMEOUT });
        await page.goto("/workspace");
        await expect(page.getByRole("button", { name: /供应商付款/ })).toHaveCount(0);
        await expect(page.getByRole("button", { name: /入库/ })).toHaveCount(0);

        page = await switchTo("fukuan");
        await page.goto("/workspace");
        await expect(page.getByRole("button", { name: /供应商付款处理/ })).toHaveCount(0);
    } finally {
        await session?.context.close();
    }
});
