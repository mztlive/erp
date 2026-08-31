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
 */
import path from "node:path";
import { test, expect, type Browser, type BrowserContext, type Locator, type Page } from "@playwright/test";

import { ACCOUNTS } from "../helpers/accounts";
import { loginViaUi, newLoggedInContext } from "../helpers/login";

const UI_TIMEOUT = 20_000;
const FLOW_TIMEOUT = 12 * 60 * 1000;
const CONTRACT_PDF = path.resolve(process.cwd(), "fixtures/sample-contract.pdf");
const SKU_KEYWORD = "龙井";
const SKU_NAME = "狮峰明前龙井礼盒";
const WAREHOUSE_NAME = "北京通州仓";
const SALES_QTY = "2";

type LoginName =
    | "xiaoshou"
    | "caigou"
    | "cangchu"
    | "caiwu"
    | "fukuan"
    | "kaipiao"
    | "admin";

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
        kaipiao: ["kaipiao", "invoice"],
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
        await input.fill("");
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
        [
            "Jan",
            "Feb",
            "Mar",
            "Apr",
            "May",
            "Jun",
            "Jul",
            "Aug",
            "Sep",
            "Oct",
            "Nov",
            "Dec",
        ][month]!,
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
    family?: "approval" | "procurement" | "fulfillment" | "finance",
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
    const hinted = hint
        ? page.getByRole("button", {
              name: new RegExp(`${typeLabel}[\\s\\S]*${hint}|${hint}[\\s\\S]*${typeLabel}`),
          })
        : page.getByRole("button", { name: new RegExp(typeLabel) });
    const fallback = page.getByRole("button", { name: new RegExp(typeLabel) }).first();
    const task = hinted.first();
    try {
        await expect(task).toBeVisible({ timeout: hint ? 8_000 : UI_TIMEOUT });
        await task.click();
    } catch {
        await expect(fallback).toBeVisible({ timeout: UI_TIMEOUT });
        await fallback.click();
    }
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

async function confirmFormal(page: Page, title: string | RegExp, confirmName: string | RegExp) {
    const dialog = page.getByRole("alertdialog").or(page.getByRole("dialog")).filter({
        hasText: title,
    });
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
    const raw = `91${stamp.replace(/[^0-9A-Za-z]/g, "").toUpperCase()}E2EWAREHOUSE`;
    return raw.slice(0, 18).padEnd(18, "0");
}

test.describe.configure({ mode: "serial" });

test("flow-01 外部采购入仓后由公司仓库发货", async ({ browser }) => {
    test.setTimeout(FLOW_TIMEOUT);
    const stamp = Date.now().toString(36).toUpperCase();
    const customerName = `E2E仓发客户${stamp}`;
    const contractNo = `HT-E2E-WH-${stamp}`;
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
        // 0) 采购责任默认调度人：销售提交实物单前必须能解析采购负责人
        let page = await switchTo("admin");
        await ensureDefaultProcurementOwner(page);

        // 1) W03 客户创建
        page = await switchTo("xiaoshou");
        await page.goto("/sales/customers");
        await expect(page.getByRole("heading", { name: "客户中心" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#customers-directory-create").click();
        const customerDialog = page.getByRole("dialog", { name: "新建客户" });
        await expect(customerDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await customerDialog.locator("#customers-form-legal-name").fill(customerName);
        await customerDialog.locator("#customers-form-short-name").fill(`仓发${stamp}`);
        await customerDialog.locator("#customers-form-credit-code").fill(uniqueCreditCode(stamp));
        await chooseOption(
            page,
            customerDialog.locator("#customers-form-payment-term"),
            "货到 15 天",
        );
        if (await customerDialog.getByRole("button", { name: "添加联系人" }).count()) {
            await customerDialog.getByRole("button", { name: "添加联系人" }).click();
            await customerDialog.getByLabel("姓名").fill("李测");
            await customerDialog.getByPlaceholder("11 位手机号").fill("13800138001");
        }
        if (await customerDialog.getByRole("button", { name: "添加地址" }).count()) {
            await customerDialog.getByRole("button", { name: "添加地址" }).click();
            await customerDialog.getByLabel("地址", { exact: true }).fill("北京市朝阳区测试路 1 号");
        }
        await customerDialog.locator("#customers-form-submit").click();
        await expectToast(page, "客户已创建");
        await expect(customerDialog).toBeHidden({ timeout: UI_TIMEOUT });
        await page.locator("#customers-directory-search").fill(customerName);
        await page.locator("#customers-directory-search").press("Enter");
        await expect(page.getByText(customerName).first()).toBeVisible({ timeout: UI_TIMEOUT });

        // 2) W04 上传合同 PDF
        await page.goto("/sales/contracts");
        await expect(page.getByRole("heading", { name: "合同" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.getByRole("button", { name: "上传合同 PDF" }).click();
        const contractDialog = page.getByRole("dialog", { name: "上传合同 PDF" });
        await expect(contractDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await contractDialog.locator("#card-contracts-upload-pdf-input").setInputFiles(CONTRACT_PDF);
        await contractDialog.locator("#card-contracts-upload-contract-no").fill(contractNo);
        await chooseOption(
            page,
            contractDialog.locator("#card-contracts-upload-customer"),
            customerName,
        );
        await expect(contractDialog.getByText(customerName).first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });
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
        await expect(page.getByRole("heading", { name: "销售单" })).toBeVisible({
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
        await page.getByRole("button", { name: "选择商品" }).click();
        const skuDialog = page.getByRole("dialog", { name: "选择商品" });
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
        await expect(orderTitleRow(page, customerName).getByText("审批中")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        salesOrderNo = (
            await page.locator("span.num.text-foreground").first().innerText()
        ).trim();
        expect(salesOrderNo).toBeTruthy();
        await expect(page.locator("#sales-orders-detail-start-change")).toBeDisabled();
        await expect(page.getByText(/采购单 0 笔/)).toBeVisible();

        // 4) W01 采购确认节点：只通过/驳回，不选源、不录入成本/交期
        page = await switchTo("caigou");
        await openWorkspaceTask(page, "单据审批", salesOrderNo, "approval");
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
        const sourcingOption = page
            .locator('[id^="procurement-orders-create-row-"][id$="-sourcing-option"]')
            .first();
        await expect(sourcingOption).toBeVisible({ timeout: UI_TIMEOUT });
        await sourcingOption.click();
        const inboundOption = page
            .getByRole("option", { name: /入仓/ })
            .or(page.locator('[data-slot="combobox-item"]').filter({ hasText: "入仓" }));
        await expect(inboundOption.first()).toBeVisible({ timeout: UI_TIMEOUT });
        await inboundOption.first().click();
        const warehouseInput = page
            .locator('[id^="procurement-orders-create-row-"][id$="-warehouse"]')
            .first();
        await expect(warehouseInput).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(page, warehouseInput, WAREHOUSE_NAME);
        await expect(page.getByText("将创建采购单").locator("xpath=..")).toContainText("1 张");
        await expect(page.getByText("将建立库存预留").locator("xpath=..")).toContainText("0 条");
        await page.locator("#procurement-orders-create-preview").click();
        const previewDialog = page.getByRole("dialog", { name: "预览供给分配" });
        await expect(previewDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(previewDialog.getByText("现有库存分配")).toHaveCount(0);
        await expect(previewDialog.getByText(/确认提交 1 张采购单/)).toBeVisible();
        await previewDialog.locator("#procurement-orders-create-preview-confirm").click();
        await expect(page.getByRole("heading", { name: "确认供给分配" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#procurement-orders-create-confirm").click();
        await expectToast(page, /供给分配已完成|已创建 1 张采购单并提交审批/);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText(/采购单 1 笔/)).toBeVisible({ timeout: UI_TIMEOUT });
        await page.getByRole("tab", { name: /采购/ }).click();
        await expect(page.getByText("草稿")).toHaveCount(0);
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText(/审批中|已生效/);

        // 6) 采购单由供给分配立即提交，财务总监审批后生效并形成应付
        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "单据审批", salesOrderNo, "approval");
        await approveCurrentDocument(page);

        // 7) 先履约后付款：仓储入库 → 仓发。若供给是先款条件，则先由出纳确认付款
        page = await switchTo("cangchu");
        await openWorkspaceTask(page, "履约处理", customerName, "fulfillment");
        const gate = page.locator("#prepayment-gate");
        if (
            (await gate.count()) &&
            /暂时不能|先款未到/.test((await gate.innerText()) ?? "")
        ) {
            page = await switchTo("fukuan");
            await openWorkspaceTask(page, "供应商付款处理", customerName, "finance");
            await expect(page.getByLabel("付款金额")).toBeVisible({ timeout: UI_TIMEOUT });
            const payAmount = page.locator("#supplier-payables-allocation-form-amount");
            if (!(await payAmount.inputValue())) {
                await payAmount.fill("1");
            }
            await page.locator("#supplier-payables-allocation-form-bank-reference").fill(`BR${stamp}`);
            await page.locator("#supplier-payables-allocation-form-submit").click();
            await confirmFormal(page, "确认付款", "确认付款");
            page = await switchTo("cangchu");
            await openWorkspaceTask(page, "履约处理", customerName, "fulfillment");
        }

        await expect(page.getByRole("heading", { name: "入库作业" })).toBeVisible({
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
        await page.locator("#fulfillment-operations-work-surface-confirm").click();
        await confirmFormal(page, "确认入库？", "确认入库");

        const shipForm = page.getByLabel("公司仓发表单");
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
        await expect(page.getByLabel("公司仓发表单")).toBeVisible({ timeout: UI_TIMEOUT });
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

        // 9) 出纳登记客户回款并提交，财务总监审批入账
        page = await switchTo("fukuan");
        await page.goto(`/sales/orders/${salesOrderId}?section=receivable`);
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#sales-orders-detail-receivable-register-receipt").click();
        await expect(page.getByRole("heading", { name: /核销/ })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        const receiptAmount = page.locator("#customer-receivables-session-amount");
        await expect(receiptAmount).toBeVisible({ timeout: UI_TIMEOUT });
        const openHint = page.getByText(/开放/).first();
        if (!(await receiptAmount.inputValue())) {
            const openText = (await openHint.innerText().catch(() => "")) || "";
            const matched = openText.match(/[\d.]+/);
            await receiptAmount.fill(matched?.[0] ?? "2576.00");
        }
        const addPool = page.getByRole("button", { name: "加入" }).first();
        if (await addPool.count()) {
            await addPool.click();
            await expect(page.getByText("已加入").first()).toBeVisible({ timeout: UI_TIMEOUT });
        }
        const fillLine = page.getByRole("button", { name: "填满" }).first();
        if (await fillLine.count()) {
            await fillLine.click();
        }
        await page.locator("#customer-receivables-session-bank-reference").fill(`RC${stamp}`);
        await page.locator("#customer-receivables-session-submit").click();
        await confirmFormal(page, /提交回款|确认提交/, "确认提交");
        await expect(page.getByText("回款已提交审批")).toBeVisible({ timeout: UI_TIMEOUT });

        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "单据审批", customerName, "approval");
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

        // 10) 开票人 W01 登记销项发票并核销；开票不阻塞关闭
        page = await switchTo("kaipiao");
        await openWorkspaceTask(page, "销项开票处理", salesOrderNo, "finance");
        await expect(page.getByRole("heading", { name: /核销/ })).toBeVisible({
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
        }
        const fillInvoice = page.getByRole("button", { name: "填满" }).first();
        if (await fillInvoice.count()) {
            await fillInvoice.click();
        }
        await page.locator("#customer-receivables-session-submit").click();
        await confirmFormal(page, "确认登记销项发票并分配", "确认提交");
        await expect(page.getByText("销项发票已登记并分配")).toBeVisible({
            timeout: UI_TIMEOUT,
        });

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
