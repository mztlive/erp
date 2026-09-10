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
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
    test,
    expect,
    type Browser,
    type BrowserContext,
    type Locator,
    type Page,
} from "@playwright/test";

import { ACCOUNTS } from "../helpers/accounts";
import { submitSalesInvoiceRequest } from "../helpers/invoices";
import { loginViaUi, newLoggedInContext } from "../helpers/login";
import {
    expectReceiptPreview,
    registerCustomerReceiptForOrder,
    submitCustomerRefundRequest,
} from "../helpers/receipts";
import { openFulfillmentWorkspaceForm, readHeaderDocumentNumber, selectWorkspaceFamily } from "../helpers/ui";

test.describe.configure({ mode: "serial" });

const UI_TIMEOUT = 20_000;
const FLOW_TIMEOUT = 15 * 60 * 1000;
const SKU_KEYWORD = "龙井";
const SKU_NAME = "狮峰明前龙井礼盒";
const SUPPLIER_SHORT = "狮峰茶叶";
const DIRECT_OPTION = "杭州狮峰茶叶有限公司 · 供应商直发";
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
        lisiyong: ["lisiyong", "salesLeader", "sales_leader"],
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
    if (await session.page.locator("#governance-auth-login-account").isVisible().catch(() => false)) {
        await loginViaUi(session.page, cred);
    }
    await session.page.goto("/workspace");
    await expect(session.page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    return session;
}

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

function escapeRe(value: string): string {
    return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function orderTitleRow(page: Page, customerName: string) {
    return page.getByRole("heading", { name: customerName }).locator("xpath=..");
}

async function expectToast(page: Page, title: string | RegExp) {
    const toast = page.locator('[data-slot="toast-title"]').filter({ hasText: title });
    await expect(toast.first()).toBeVisible({ timeout: UI_TIMEOUT });
    // 关闭已确认的悬浮提示，避免其遮挡后续按钮造成偶发点击失败。
    for (let i = 0; i < 5; i += 1) {
        const dismiss = page
            .locator('[data-slot="toast"]')
            .getByRole("button", { name: "关闭提示", includeHidden: true })
            .first();
        if (!(await dismiss.count())) break;
        await dismiss.click({ timeout: 5_000 }).catch(() => undefined);
    }
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
    const calendar = page.locator('[data-slot="calendar"]:visible');
    await expect(calendar).toBeVisible({ timeout: UI_TIMEOUT });
    const byId = calendar.locator(`[id$="-day-${isoDate}"]`);
    if (await byId.count()) {
        await expect(byId.first()).toBeVisible({ timeout: UI_TIMEOUT });
        await byId.first().click();
        return;
    }
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
    // 日期按钮无障碍名为完整日期，子串匹配后由下方循环跳过禁选日期。
    const dayButtons = calendar.getByRole("button", { name: day });
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
        await selectWorkspaceFamily(page, family);
    }
    // 工作台后端搜索只匹配单据 ID、类型码等字段，不匹配单号与往来方，
    // 在搜索框填写 hint 会把列表滤空；改为在待办列表中匹配任务。
    // 单号可能只出现在无障碍名中，客户名只出现在可见文本中，两处取并集后点选。
    const list = page.getByRole("list", { name: "待办列表" });
    await expect(list).toBeVisible({ timeout: UI_TIMEOUT });
    const label = `(?:${typeLabel})`;
    const union = hint
        ? list
              .getByRole("button", {
                  name: new RegExp(
                      `${label}[\\s\\S]*${escapeRe(hint)}|${escapeRe(hint)}[\\s\\S]*${label}`,
                  ),
              })
              .or(
                  list
                      .getByRole("button", { name: new RegExp(label) })
                      .filter({ hasText: hint }),
              )
              .first()
        : list.getByRole("button", { name: new RegExp(label) }).first();
    try {
        await expect(union).toBeVisible({ timeout: hint ? 8_000 : UI_TIMEOUT });
        await union.click();
        return;
    } catch {
        if (!hint) throw new Error(`工作台未找到任务: ${typeLabel}`);
    }
    // 兜底：履约任务按钮仅显示采购单号，不含销售单号与客户名，hint 无法匹配；
    // 同类型仅有一项时直接点选，多项时显式失败避免点错任务。
    const sameType = list.getByRole("button", { name: new RegExp(label) });
    await expect(sameType).toHaveCount(1, { timeout: UI_TIMEOUT });
    await sameType.first().click();
}

async function approveCurrentDocument(page: Page) {
    const approve = page.getByRole("button", { name: /^(通过|同意审批)$/ });
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

async function factValue(page: Page, label: string) {
    const dt = page.locator('[data-slot="formal-action-result"] dt', { hasText: label });
    await expect(dt.first()).toBeVisible({ timeout: UI_TIMEOUT });
    return (await dt.first().locator("xpath=following-sibling::dd[1]").innerText()).trim();
}

async function ensureDefaultProcurementOwner(page: Page) {
    await page.goto("/master-data/procurement-responsibilities");
    await expect(page.getByRole("heading", { name: "采购责任规则" })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    // 规则列表在标题之后加载，先等列表接口返回再判断是否已存在。
    await page
        .waitForResponse(
            (response) =>
                response.request().method() === "GET" &&
                response.url().includes("procurement-responsibility-rules"),
            { timeout: UI_TIMEOUT },
        )
        .catch(() => undefined);
    if (await page.getByText("默认调度人").count()) {
        return;
    }
    const byId = page.locator("#procurement-responsibility-rules-create");
    if (await byId.count()) {
        await byId.click();
    } else {
        await page.getByRole("button", { name: "新增规则" }).click();
    }
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
    const save = dialog.locator("#procurement-responsibility-rules-dialog-save");
    if (await save.count()) {
        await save.click();
    } else {
        await dialog.getByRole("button", { name: "保存规则" }).click();
    }
    await expectToast(page, /采购责任规则已新增|采购责任规则已更新/);
    await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT });
}

async function assertNoReturnApprovalTasks(page: Page) {
    await page.goto("/workspace");
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    await expect(page.getByRole("button", { name: /销售退货单审批/ })).toHaveCount(0);
    await expect(page.getByRole("button", { name: /采购退货单审批/ })).toHaveCount(0);
    await expect(page.getByRole("button", { name: /发票审批/ })).toHaveCount(0);
    await expect(page.getByRole("button", { name: /客户验收单审批/ })).toHaveCount(0);
}

async function assertNoReturnCreateUi(page: Page) {
    await expect(page.getByRole("button", { name: /创建退货|新建退货|登记退货|退货处理单/ })).toHaveCount(
        0,
    );
    await expect(page.getByRole("button", { name: /创建采购退货|新建采购退货/ })).toHaveCount(0);
    await expect(page.getByRole("button", { name: /确认处理完成|确认退货处理完成/ })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "选择流程" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "过账" })).toHaveCount(0);
}

async function uploadBankReceipt(page: Page, fileName: string) {
    const input = page.locator("#supplier-payables-allocation-form-bank-receipt-input");
    if (await input.count()) {
        await input.setInputFiles({
            name: fileName,
            mimeType: "image/png",
            buffer: PNG_1X1,
        });
        return;
    }
    const fallback = page.locator('input[type="file"]').first();
    if (await fallback.count()) {
        await fallback.setInputFiles({
            name: fileName,
            mimeType: "image/png",
            buffer: PNG_1X1,
        });
    }
}

async function confirmSupplierPaymentIfGated(page: Page): Promise<boolean> {
    const gate = page.locator("#prepayment-gate");
    const confirmBtn = page.locator("#fulfillment-operations-work-surface-confirm");
    const blockedText = page.getByText(/先款未到|暂时不能/);
    return (
        ((await gate.count()) > 0 && /暂时不能|先款未到/.test((await gate.innerText()) ?? "")) ||
        (await blockedText.isVisible().catch(() => false)) ||
        ((await confirmBtn.count()) > 0 && !(await confirmBtn.isEnabled()))
    );
}

async function submitSupplierPayment(page: Page, stamp: string) {
    await openWorkspaceTask(page, "供应商付款处理", undefined, "finance");
    await expect(page.getByLabel("付款金额").or(page.locator("#supplier-payables-allocation-form-amount"))).toBeVisible({
        timeout: UI_TIMEOUT,
    });
    const payAmount = page.locator("#supplier-payables-allocation-form-amount");
    if (await payAmount.count()) {
        const current = await payAmount.inputValue();
        if (!current) {
            await payAmount.fill("1");
        }
    }
    const bankRef = page.locator("#supplier-payables-allocation-form-bank-reference");
    if (await bankRef.count()) {
        await bankRef.fill(`BR${stamp}`);
    }
    await uploadBankReceipt(page, `bank-receipt-${stamp}.png`);
    await page.locator("#supplier-payables-allocation-form-submit").click();
    const payDialog = page.getByRole("alertdialog").filter({ hasText: "确认付款" });
    await expect(payDialog).toBeVisible({ timeout: UI_TIMEOUT });
    await expect(payDialog.getByText("提交审批")).toHaveCount(0);
    const confirm = payDialog.locator("#supplier-payables-payment-submit-confirm-confirm");
    if (await confirm.count()) {
        await confirm.click();
    } else {
        await payDialog.getByRole("button", { name: "确认付款" }).click();
    }
    await expect(payDialog).toBeHidden({ timeout: UI_TIMEOUT });
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
        session = await openSession(browser, login);
        return session.page;
    };

    try {
        // 0) 采购责任默认调度人
        let page = await switchTo("admin");
        await ensureDefaultProcurementOwner(page);

        // 1) W03 客户
        page = await switchTo("xiaoshou");
        await page.goto("/sales/customers");
        await expect(page.getByRole("heading", { name: "客户中心" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#customers-directory-create").click();
        const customerDialog = page.getByRole("dialog", { name: "新建客户" });
        await expect(customerDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await customerDialog.locator("#customers-form-legal-name").fill(customerName);
        await customerDialog.locator("#customers-form-short-name").fill(`拒收${stamp}`);
        await customerDialog.locator("#customers-form-credit-code").fill(creditCode);
        await chooseOption(
            page,
            customerDialog.locator("#customers-form-payment-term"),
            "货到 15 天",
        );
        if (await customerDialog.getByRole("button", { name: "添加联系人" }).count()) {
            await customerDialog.getByRole("button", { name: "添加联系人" }).click();
            await customerDialog.getByLabel("姓名").fill("拒收联系人");
            await customerDialog.getByPlaceholder("11 位手机号").fill("13800138015");
        }
        if (await customerDialog.getByRole("button", { name: "添加地址" }).count()) {
            await customerDialog.getByRole("button", { name: "添加地址" }).click();
            // 地址输入的无障碍名称含必填星号，且“地址类型/地址联系人”都会命中文本匹配，改用 id 前后缀定位该行输入。
            await customerDialog
                .locator('input[id^="customers-form-addresses-"][id$="-address"]')
                .fill("北京市朝阳区拒收路 15 号");
        }
        await customerDialog.locator("#customers-form-submit").click();
        await expectToast(page, "客户已创建");
        await expect(customerDialog).toBeHidden({ timeout: UI_TIMEOUT });

        // 2) W04 合同 PDF
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
        await chooseOption(page, contractDialog.locator("#card-contracts-upload-customer"), customerName);
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

        // 3) W05 销售单：数量 2，便于部分拒收仍能记入验收
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
        await expect(page.getByText(customerName).first()).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(page, page.locator("#sales-orders-create-header-welfare-scene"), "年节礼包");
        await chooseOption(
            page,
            page.locator("#sales-orders-create-header-payment-terms"),
            "货到 15 天",
        );
        await page.locator('[id^="sales-orders-create-line-"][id$="-pick-sku"]').click();
        const skuDialog = page.getByRole("dialog", { name: "更换销售商品" });
        await expect(skuDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await skuDialog.locator("#master-data-list-sellable-list-toolbar-search-input").fill(SKU_KEYWORD);
        await skuDialog.locator("#master-data-list-sellable-list-toolbar-search-input").press("Enter");
        const skuRow = skuDialog.getByRole("checkbox", { name: new RegExp(`选择.*${SKU_NAME}`) });
        await expect(skuRow.first()).toBeVisible({ timeout: UI_TIMEOUT });
        await skuRow.first().check();
        await skuDialog.locator("#sales-orders-sku-picker-confirm").click();
        await expect(skuDialog).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(page.getByText(SKU_NAME).first()).toBeVisible({ timeout: UI_TIMEOUT });
        await page.getByLabel("数量").fill(SALES_QTY);
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
        await expect(orderTitleRow(page, customerName).getByText(/审批中|审核中/)).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        salesOrderNo = await readHeaderDocumentNumber(page)
        expect(salesOrderNo).toBeTruthy();
        await expect(page.locator("#sales-orders-detail-start-change")).toBeDisabled();
        await page.getByRole("tab", { name: /采购/ }).click();
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("待采购", {
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("改单中")).toHaveCount(0);

        // 4) 负向：生效前不得建采购单、不得履约
        page = await switchTo("caigou");
        await page.goto("/procurement/orders");
        await expect(page.getByRole("heading", { name: "采购单", exact: true })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText(salesOrderNo)).toHaveCount(0);
        await expect(page.getByText("供应商直发")).toHaveCount(0);

        // 5) 采购确认：只通过，不选源
        await openWorkspaceTask(page, "销售单审批", salesOrderNo, "approval");
        await expect(page.getByRole("heading", { name: /销售单/ })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByRole("button", { name: "预览供给分配" })).toHaveCount(0);
        await approveCurrentDocument(page);

        // 6) 供给分配：最短路径选供应商直发（对应文档直退供应商、不经公司仓）
        await page.getByRole("button", { name: "刷新" }).click().catch(() => undefined);
        await openWorkspaceTask(page, "待供给分配", salesOrderNo, "procurement");
        await expect(page.getByRole("heading", { name: "供给分配" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        const expandSourcing = page.getByRole("button", { name: "调整方案" }).first();
        if (await expandSourcing.isVisible().catch(() => false)) {
            await expandSourcing.click();
        }
        const sourcingOption = page
            .locator('[id^="procurement-orders-create-row-"][id$="-sourcing-option"]')
            .first();
        await expect(sourcingOption).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(page, sourcingOption, DIRECT_OPTION);
        await expect(page.getByRole("combobox", { name: "仓库", exact: true })).toHaveCount(0);
        await page.locator("#procurement-orders-create-preview").click();
        const previewDialog = page.getByRole("dialog", { name: "预览供给分配" });
        await expect(previewDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(previewDialog.getByText("现有库存分配")).toHaveCount(0);
        await expect(previewDialog.getByText(/确认提交 1 张采购单/)).toBeVisible();
        await previewDialog.locator("#procurement-orders-create-preview-confirm").click();
        await expectToast(page, /供给分配已完成|已创建 1 张采购单并提交审批/);

        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.getByRole("tab", { name: /^采购/ }).click();
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("采购已覆盖", {
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByText("草稿")).toHaveCount(0);
        // 销售账号无采购单明细查看权限，面板仅显示计数提示，不显示审批中/已生效。
        await expect(page.getByTestId("sales-order-purchase-count-only")).toContainText(
            /已创建 1 张采购单/,
            { timeout: UI_TIMEOUT },
        );
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toHaveCount(0);

        // 7) 采购单财务审批
        page = await switchTo("caiwu");
        await openWorkspaceTask(page, "采购单审批", salesOrderNo, "approval");
        await approveCurrentDocument(page);

        page = await switchTo("caigou");
        await page.goto("/procurement/orders");
        await expect(page.getByRole("heading", { name: "采购单", exact: true })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        const poTable = page.locator("#procurement-orders-list-table");
        await expect(poTable.getByText("供应商直发")).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(poTable.getByText("草稿")).toHaveCount(0);
        purchaseOrderNo = (
            (await poTable.getByRole("button", { name: /打开采购单/ }).textContent()) ?? ""
        ).trim();
        expect(purchaseOrderNo.length).toBeGreaterThan(0);

        // 8) 仓储不得出现入库履约（代发不经公司仓）
        page = await switchTo("cangchu");
        await page.goto("/workspace?family=fulfillment");
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByRole("button", { name: /入库/ })).toHaveCount(0);
        await expect(page.getByRole("button", { name: /退货/ })).toHaveCount(0);

        // 9) 若先款门槛拦住直发，出纳先确认付款（同时也形成后续供应商退款的原付款事实）
        page = await switchTo("caigou");
        await openWorkspaceTask(page, "履约处理", customerName, "fulfillment");
        await openFulfillmentWorkspaceForm(page);
        if (await confirmSupplierPaymentIfGated(page)) {
            page = await switchTo("fukuan");
            await submitSupplierPayment(page, stamp);
            page = await switchTo("caigou");
            await openWorkspaceTask(page, "履约处理", customerName, "fulfillment");
            await openFulfillmentWorkspaceForm(page);
        }

        // 10) 采购登记代发并确认发货
        await expect(page.locator('[aria-label="供应商直发表单"]')).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(page, page.locator("#fulfillment-operations-direct-form-carrier"), "顺丰速运");
        await page.locator("#fulfillment-operations-direct-form-tracking-no").fill(trackingNo);
        const shipQty = page.locator('[id^="fulfillment-operations-direct-form-quantity-"]').first();
        if (await shipQty.count()) {
            const current = await shipQty.inputValue();
            if (!current || current === "0") {
                await shipQty.fill(SALES_QTY);
            }
        }
        const confirmShip = page.locator("#fulfillment-operations-work-surface-confirm");
        if (!(await confirmShip.isEnabled().catch(() => false))) {
            page = await switchTo("fukuan");
            await submitSupplierPayment(page, stamp);
            page = await switchTo("caigou");
            await openWorkspaceTask(page, "履约处理", customerName, "fulfillment");
            await openFulfillmentWorkspaceForm(page);
            await chooseOption(page, page.locator("#fulfillment-operations-direct-form-carrier"), "顺丰速运");
            await page.locator("#fulfillment-operations-direct-form-tracking-no").fill(trackingNo);
        }
        await expect(page.locator("#fulfillment-operations-work-surface-confirm")).toBeEnabled({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#fulfillment-operations-work-surface-confirm").click();
        const shipped = page.waitForResponse(response => response.request().method() === "POST" && /\/deliveries\/[^/]+\/post$/.test(response.url()));
        await confirmFormal(page, "确认发货？", "确认发货");
        const shippedResponse = await shipped;
        expect(shippedResponse.ok()).toBeTruthy();
        expect((await shippedResponse.json()).data.status).toBe("SHIPPED");

        // 11) 销售登记部分拒收（1 件拒收 + 1 件通过，整件拒收不能记入验收）
        page = await switchTo("xiaoshou");
        await openWorkspaceTask(page, "客户验收登记", salesOrderNo, "fulfillment");
        await page.locator("#sales-orders-acceptance-register-open").click();
        const acceptanceDialog = page.getByRole("dialog", { name: "登记客户验收" });
        await expect(acceptanceDialog).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(acceptanceDialog.getByText("商品明细可记通过、短少或拒收。")).toBeVisible();
        const rejectToggle = acceptanceDialog
            .getByRole("button", { name: "拒收" })
            .or(acceptanceDialog.getByRole("radio", { name: "拒收" }));
        await expect(rejectToggle.first()).toBeVisible({ timeout: UI_TIMEOUT });
        await rejectToggle.first().click();
        await expect(acceptanceDialog.getByLabel("拒收数量")).toBeVisible({ timeout: UI_TIMEOUT });
        await acceptanceDialog.getByLabel("拒收数量").fill(REJECT_QTY);
        await acceptanceDialog.getByPlaceholder("短少或拒收时必填").fill("外箱破损，客户拒收一件");
        await expect(acceptanceDialog.getByText(FACT_ONLY_NOTICE)).toBeVisible();
        await expect(acceptanceDialog.getByRole("button", { name: /创建退货|另开退货/ })).toHaveCount(0);
        await acceptanceDialog.locator("#sales-orders-acceptance-register-submit").click();
        const acceptConfirm = page.getByRole("alertdialog", { name: "确认客户验收", exact: true });
        await expect(acceptConfirm).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(acceptConfirm).toContainText("拒收");
        const acceptConfirmBtn = acceptConfirm.locator("#sales-orders-acceptance-confirm-confirm");
        if (await acceptConfirmBtn.count()) {
            await acceptConfirmBtn.click();
        } else {
            await acceptConfirm.getByRole("button", { name: "确认验收" }).click();
        }
        await expect(acceptConfirm).toBeHidden({ timeout: UI_TIMEOUT });
        await expect(page.getByText("客户验收已登记").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByText("拒收").first()).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByText(/请另开退货或拒收处理单/).first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await assertNoReturnCreateUi(page);

        // 12) 销售单详情：未关闭、未开变更单；无退货处理单创建入口
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("履约中", { exact: true })).toBeVisible();
        await expect(orderTitleRow(page, customerName).getByText("已关闭")).toHaveCount(0);
        await expect(page.getByText("改单中")).toHaveCount(0);
        await assertNoReturnCreateUi(page);
        await expect(page.getByText("选择流程")).toHaveCount(0);
        await expect(page.getByRole("button", { name: /更新审批流程版本|撤回审批|改派当前审批人/ })).toHaveCount(
            0,
        );

        page = await switchTo("caigou");
        await page.goto("/procurement/orders");
        await expect(page.getByRole("button", { name: /打开采购单/ }).first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.getByRole("button", { name: /打开采购单/ }).first().click();
        await expect(page.getByRole("tab", { name: "变更" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.getByRole("tab", { name: "变更" }).click();
        await expect(page.getByText("暂无采购退货。")).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByRole("button", { name: /创建采购退货|新建采购退货/ })).toHaveCount(0);
        await expect(page.getByRole("button", { name: "选择流程" })).toHaveCount(0);
        await expect(page.getByRole("button", { name: /^(通过|同意审批)$/ })).toHaveCount(0);

        page = await switchTo("cangchu");
        await page.goto("/workspace?family=fulfillment");
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByRole("button", { name: /退货验收|退货入库|退货出库|采购退货/ })).toHaveCount(
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
        salesInvoiceNo = `INV${stamp}`;
        await page.locator("#customer-receivables-session-invoice-no").fill(salesInvoiceNo);
        const gross = page.locator("#customer-receivables-session-gross-amount");
        if (await gross.count()) {
            const current = await gross.inputValue();
            if (!current) {
                await gross.fill("2576.00");
            }
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
        await expect(page.getByRole("heading", { name: "确认登记销项发票并分配" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        const committed = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/admin/invoices/commit"), { timeout: UI_TIMEOUT });
        await page.locator("#customer-receivables-session-invoice-confirm-dialog-confirm").click();
        expect((await committed).ok()).toBeTruthy();
        await page.goto(`/finance/customer-accounts?view=sales_invoice&q=${encodeURIComponent(salesInvoiceNo)}`);
        await expect(page.getByRole("row").filter({ hasText: salesInvoiceNo })).toContainText("已登记", { timeout: UI_TIMEOUT });
        await expect(page.getByRole("button", { name: /选择流程|通过|驳回/ })).toHaveCount(0);

        // 15) 出纳按原回款发起客户退款 → 销售领导 → 财务总监
        page = await switchTo("fukuan");
        await page.goto("/finance/customer-accounts?view=receipt");
        await expect(page.getByRole("heading", { name: "客户往来" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#customer-receivables-view-receipt").click();
        await page.locator("#customer-receivables-toolbar-search").fill(receiptNo);
        await page.locator("#customer-receivables-toolbar-search").press("Enter");
        const receiptRow = page.getByRole("row").filter({ hasText: receiptNo });
        await expect(receiptRow).toBeVisible({ timeout: UI_TIMEOUT });
        await receiptRow.click();
        await expectReceiptPreview(page, receiptNo);
        await page.locator("#customer-receivables-preview-receipt-refund").click();
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
        await page.locator("#customer-receivables-view-receipt").click();
        await page.locator("#customer-receivables-toolbar-search").fill(receiptNo);
        await page.locator("#customer-receivables-toolbar-search").press("Enter");
        await expect(page.getByRole("row").filter({ hasText: receiptNo })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(
            page.getByRole("row").filter({ hasText: receiptNo }).getByText("已过账", { exact: true }),
        ).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(
            page.getByRole("row").filter({ hasText: receiptNo }).getByText("已冲正"),
        ).toHaveCount(0);

        // 16) 开票人登记销项红票：原蓝票保留
        page = await switchTo("kaipiao");
        await page.goto("/finance/customer-accounts?view=sales_invoice");
        await expect(page.getByRole("heading", { name: "客户往来" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#customer-receivables-view-sales_invoice").click();
        await page.locator("#customer-receivables-toolbar-search").fill(salesInvoiceNo);
        await page.locator("#customer-receivables-toolbar-search").press("Enter");
        const invoiceRow = page.getByRole("row").filter({ hasText: salesInvoiceNo });
        await expect(invoiceRow).toBeVisible({ timeout: UI_TIMEOUT });
        await invoiceRow.click();
        await page.locator("#customer-receivables-preview-invoice-red").click();
        const redDialog = page.getByRole("dialog", { name: "发起销项红票" });
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
        await expect(page.getByText(/反向记录已追加|已登记独立红票/).first()).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByRole("button", { name: /选择流程|通过|驳回/ })).toHaveCount(0);
        await page.goto("/finance/customer-accounts?view=sales_invoice");
        await page.locator("#customer-receivables-view-sales_invoice").click();
        await page.locator("#customer-receivables-toolbar-search").fill("");
        await page.locator("#customer-receivables-toolbar-search").press("Enter");
        await expect(page.getByRole("row").filter({ hasText: salesInvoiceNo }).getByText("蓝票")).toBeVisible({ timeout: UI_TIMEOUT });
        await expect(page.getByRole("row").filter({ hasText: salesRedResult.invoice_no })).toContainText("红票", { timeout: UI_TIMEOUT });

        // 17) 出纳完成供应商付款（若履约前未付完），形成原付款事实
        page = await switchTo("fukuan");
        await page.goto("/workspace");
        await selectWorkspaceFamily(page, "finance");
        const payTask = page.getByRole("button", { name: /供应商付款处理/ });
        if (await payTask.count()) {
            await submitSupplierPayment(page, `${stamp}B`);
        }

        // 18) 财务登记进项蓝票（红票依据）。进项登记入口在 W12，flow-07 由 caiwu 操作
        page = await switchTo("caiwu");
        await page.goto("/finance/supplier-accounts");
        await expect(page.getByRole("heading", { name: "供应商往来" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#supplier-payables-header-register-invoice").click();
        const pickSupplier = page.getByRole("dialog", { name: /选择供应商 · 登记进项发票/ });
        await expect(pickSupplier).toBeVisible({ timeout: UI_TIMEOUT });
        await chooseOption(
            page,
            pickSupplier.locator("#supplier-payables-pick-supplier-select"),
            /狮峰/,
        );
        await pickSupplier.locator("#supplier-payables-pick-supplier-confirm").click();
        await expect(page.getByRole("heading", { name: "登记进项发票" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(page.getByRole("button", { name: "提交审批" })).toHaveCount(0);
        const poolSelect = page
            .locator('[id^="supplier-payables-allocation-pool-row-"][id$="-select"]')
            .first();
        await expect(poolSelect).toBeVisible({ timeout: UI_TIMEOUT });
        if (!(await poolSelect.isChecked())) {
            await page.locator("#supplier-payables-allocation-pool-select-all").click();
        }
        await page.locator("#supplier-payables-allocation-pool-fill-all").click();
        const allocatedInput = page
            .locator('[id^="supplier-payables-allocation-pool-row-"][id$="-amount"]')
            .first();
        await expect(allocatedInput).toHaveValue(/.+/, { timeout: UI_TIMEOUT });
        const allocated = await allocatedInput.inputValue();
        const grossCents = Math.round(Number(allocated) * 100);
        expect(grossCents).toBeGreaterThan(0);
        const netCents = Math.round(grossCents / 1.13);
        await page.locator("#supplier-payables-allocation-form-gross-amount").fill(allocated);
        await page.locator("#supplier-payables-allocation-form-net-amount").fill((netCents / 100).toFixed(2));
        await page.locator("#supplier-payables-allocation-form-tax-amount").fill(((grossCents - netCents) / 100).toFixed(2));
        await page.locator("#supplier-payables-allocation-form-invoice-no").fill(purchaseInvoiceNo);
        await page.locator("#supplier-payables-allocation-form-submit").click();
        const invoiceConfirm = page.getByRole("alertdialog").filter({
            hasText: "确认登记进项发票并核销",
        });
        await expect(invoiceConfirm).toBeVisible({ timeout: UI_TIMEOUT });
        await invoiceConfirm.locator("#supplier-payables-invoice-allocate-confirm-confirm").click();
        await expect(page.getByText("进项发票已登记").first()).toBeVisible({ timeout: UI_TIMEOUT });

        // 19) 出纳按原付款发起供应商退款 → 采购 → 财务总监
        page = await switchTo("fukuan");
        await page.goto("/finance/supplier-accounts?view=payment");
        await expect(page.getByRole("heading", { name: "供应商往来" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page.locator("#supplier-payables-view-tabs-trigger-payment").click();
        await page.getByRole("row").filter({ hasText: "已过账" }).first().click();
        const refundBtn = page.getByRole("button", { name: "退款" }).first();
        await expect(refundBtn).toBeVisible({ timeout: UI_TIMEOUT });
        await refundBtn.click();
        await expect(page.getByRole("dialog").getByRole("heading", { name: /供应商退款/ })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await page
            .locator("#supplier-payables-refund-request-reason")
            .fill("客户拒收直退供应商，按原付款追加退款，原付款保留");
        const supplierRefundCommitted = page.waitForResponse(
            (response) =>
                response.request().method() === "POST" &&
                response.url().includes("/admin/supplier-refunds/commit"),
        );
        await page.locator("#supplier-payables-refund-request-submit").click();
        const supplierRefundConfirm = page.getByRole("alertdialog", {
            name: /确认提交退款|提交退款/,
        });
        try {
            await expect(supplierRefundConfirm).toBeVisible({ timeout: 5_000 });
            await page.locator("#supplier-payables-refund-submit-confirm-confirm").click();
        } catch {
            // prepareRefundDraft 已直接提交。
        }
        const supplierRefundResponse = await supplierRefundCommitted;
        expect(supplierRefundResponse.ok()).toBeTruthy();
        const supplierRefundResult = (await supplierRefundResponse.json()).data;
        supplierRefundNo = supplierRefundResult.refund_no ?? supplierRefundResult.refundNo;
        expect(String(supplierRefundNo).length).toBeGreaterThan(2);
        await expect(page.getByText(/退款已提交审批/).first()).toBeVisible({
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
        const supplierHeading = page.getByRole("heading", { name: "供应商往来" });
        await expect(supplierHeading).toBeVisible({ timeout: UI_TIMEOUT });
        {
            await page.locator("#supplier-payables-view-tabs-trigger-purchase-invoice").click();
            await page.locator("#supplier-payables-toolbar-search").fill(purchaseInvoiceNo);
            await page.locator("#supplier-payables-toolbar-search").press("Enter");
            const purchaseInvoiceRow = page
                .getByRole("row")
                .filter({ hasText: purchaseInvoiceNo })
                .filter({ hasText: "蓝票" });
            await expect(purchaseInvoiceRow).toBeVisible({ timeout: UI_TIMEOUT });
            await purchaseInvoiceRow.click();
            const purchaseRed = page.locator("#supplier-payables-preview-record-red-invoice");
            await expect(purchaseRed).toBeVisible({ timeout: UI_TIMEOUT });
            {
                await purchaseRed.click();
                const purchaseRedDlg = page.getByRole("dialog", { name: "进项红票" });
                await expect(purchaseRedDlg).toBeVisible({ timeout: UI_TIMEOUT });
                await purchaseRedDlg
                    .locator("#supplier-payables-reverse-dialog-reason")
                    .fill("客户拒收，进项红冲");
                await purchaseRedDlg
                    .locator("#supplier-payables-reverse-dialog-red-invoice-no")
                    .fill(`R${purchaseInvoiceNo}`);
                await purchaseRedDlg.locator("#supplier-payables-reverse-dialog-confirm").click();
                await expect(page.getByText("红票已登记").first()).toBeVisible({
                    timeout: UI_TIMEOUT,
                });
            }
            await expect(page.getByRole("button", { name: /选择流程|通过|驳回/ })).toHaveCount(0);
        }

        // 21) 销售侧：原出库/回款/发票保留；无确认处理完成入口；销售单不得因开票关闭
        page = await switchTo("xiaoshou");
        await page.goto(`/sales/orders/${salesOrderId}`);
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        await expect(orderTitleRow(page, customerName).getByText("履约中", { exact: true })).toBeVisible();
        await expect(page.getByText("改单中")).toHaveCount(0);
        await assertNoReturnCreateUi(page);
        await page.getByRole("tab", { name: /^票款/ }).click();
        await expect(page.getByRole("heading", { name: "开票申请" })).toBeVisible({
            timeout: UI_TIMEOUT,
        });
        // 全额红冲后有效开票归零；原蓝票和红票已在财务台账核验。
        await page.goto("/workspace");
        await assertNoReturnApprovalTasks(page);
    } finally {
        await session?.context.close();
    }
});
