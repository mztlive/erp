/**
 * 流程: [flow-07] 供应商票款：出纳付款任务、进项发票、核销与冲正
 * 文档: docs/erp-phase-1.md §9.2 + §6.5.4；docs/approval-workflow-contract.md §4.3
 *       （SupplierPayment=NO_APPROVAL）+ docs/workbench-workitem-contract.md §3
 * 账号: xiaoshou（销售建客/合同/销售单）→ caigou（采购确认、供给分配、冲正确认依据）
 *       → caiwu（采购单审批、进项发票、冲正末节点；禁止自己提交冲正）
 *       → fukuan（W01 付款任务分两次确认入账、发起付款冲正）
 *
 * 文档-代码差异（测试以代码为准）:
 * - 文档写「过账」；付款作业按钮是「登记付款并核销」「确认付款」，状态徽标/Toast 仍含「已过账」。
 * - 文档 6.5.4 画「业务部门先确认依据、财务经办再建单」；付款冲正由 fukuan 一次创建并启动审批，
 *   再由 caigou→caiwu 在 W01 审批入账。
 * - W12 仍有「登记付款」按钮，但无付款任务时禁用，文案要求从工作台付款任务进入。
 * - 付款详情无提交审批入口；SupplierPayment 不得出现审批实例/审批任务。
 */
import fs from "node:fs";
import os from "node:os";
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
import { loginViaUi, newLoggedInContext } from "../helpers/login";

test.use({ viewport: { width: 1440, height: 960 } });
test.setTimeout(12 * 60 * 1000);

const SKU_NAME = "狮峰明前龙井礼盒";
const SUPPLIER_SHORT = "狮峰茶叶";
const RECEIPT_PNG = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    "base64",
);

type LoginTarget = string | { account?: string; username?: string; password?: string };

function accountLogin(role: string): LoginTarget {
    const table = (ACCOUNTS ?? {}) as Record<string, LoginTarget>;
    return table[role] ?? role;
}

function isoDate(offsetDays = 0): string {
    const date = new Date();
    date.setDate(date.getDate() + offsetDays);
    const pad = (value: number) => String(value).padStart(2, "0");
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function parseAmount(raw: string): string {
    const match = raw.replace(/,/g, "").match(/-?\d+(?:\.\d+)?/);
    if (!match) throw new Error(`无法解析金额: ${raw}`);
    return Number(match[0]).toFixed(2);
}

function splitHalf(amount: string): { first: string; rest: string } {
    const cents = Math.round(Number(parseAmount(amount)) * 100);
    const first = Math.floor(cents / 2);
    const rest = cents - first;
    if (first <= 0 || rest <= 0) {
        throw new Error(`应付 ${amount} 无法拆成两笔正数付款`);
    }
    return { first: (first / 100).toFixed(2), rest: (rest / 100).toFixed(2) };
}

function splitGross(gross: string, taxRatePercent = "13"): { net: string; tax: string } {
    const grossCents = Math.round(Number(parseAmount(gross)) * 100);
    const rate = Number(taxRatePercent);
    const netCents = Math.round(grossCents / (1 + rate / 100));
    const taxCents = grossCents - netCents;
    return { net: (netCents / 100).toFixed(2), tax: (taxCents / 100).toFixed(2) };
}

function contractPdfPath(): string {
    const here = path.dirname(fileURLToPath(import.meta.url));
    const candidates = [
        path.join(process.cwd(), "fixtures", "sample-contract.pdf"),
        path.join(here, "..", "fixtures", "sample-contract.pdf"),
    ];
    for (const candidate of candidates) {
        if (fs.existsSync(candidate)) return candidate;
    }
    const fallback = path.join(os.tmpdir(), "flow-07-sample-contract.pdf");
    fs.writeFileSync(
        fallback,
        "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n",
    );
    return fallback;
}

async function expectToast(page: Page, title: string | RegExp): Promise<void> {
    await expect(page.getByText(title).first()).toBeVisible({ timeout: 20_000 });
}

async function chooseOption(
    page: Page,
    input: Locator,
    optionName: string | RegExp,
    query?: string,
): Promise<void> {
    await input.click();
    if (query) {
        await input.fill(query);
    }
    await page.getByRole("option", { name: optionName }).first().click({ timeout: 20_000 });
}

async function pickCalendarDay(page: Page, date = isoDate()): Promise<void> {
    await page.locator(`[id$="-day-${date}"]`).first().click();
}

async function gotoWorkspace(page: Page): Promise<void> {
    await page.goto("/workspace");
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: 20_000,
    });
}

async function openWorkspaceTask(page: Page, name: RegExp | string): Promise<void> {
    const list = page.getByRole("list", { name: "待办列表" });
    await expect(list).toBeVisible({ timeout: 20_000 });
    await list.getByRole("button", { name }).first().click();
}

async function approveOpenTask(page: Page, nodeName?: string | RegExp): Promise<void> {
    await expect(page.getByRole("button", { name: "通过" })).toBeVisible({
        timeout: 20_000,
    });
    await page.getByRole("button", { name: "通过" }).click();
    const dialog = page.getByRole("dialog").filter({ hasText: "确认通过" });
    await expect(dialog).toBeVisible({ timeout: 20_000 });
    if (nodeName) {
        await expect(dialog.getByText(nodeName)).toBeVisible();
    }
    await dialog.getByRole("button", { name: "确认通过" }).click();
    await expect(dialog).toBeHidden({ timeout: 20_000 });
}

async function assertNoSupplierPaymentApproval(page: Page): Promise<void> {
    await expect(page.getByText("供应商付款单审批")).toHaveCount(0);
    await expect(page.getByText("SupplierPayment")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "提交审批" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "撤回审批" })).toHaveCount(0);
}

async function uploadReceipt(page: Page, label: string): Promise<void> {
    await page.locator("#supplier-payables-allocation-form-bank-receipt-input").setInputFiles({
        name: label,
        mimeType: "image/png",
        buffer: RECEIPT_PNG,
    });
}

async function waitLoggedIn(page: Page): Promise<void> {
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: 20_000,
    });
}

async function openRole(
    browser: Browser,
    role: string,
): Promise<{ page: Page; close: () => Promise<void> }> {
    const result = await newLoggedInContext(browser, accountLogin(role) as never);
    if (result && typeof result === "object" && "page" in result) {
        const wrapped = result as { page: Page; context?: BrowserContext };
        const context = wrapped.context ?? wrapped.page.context();
        return { page: wrapped.page, close: () => context.close() };
    }
    const page = result as Page;
    return { page, close: () => page.context().close() };
}

async function switchSupplierView(page: Page, view: "payable" | "payment" | "purchase_invoice") {
    await page.locator(`#supplier-payables-view-tabs-trigger-${view}`).click();
}

test("供应商票款：W01 付款任务分次入账、进项发票核销与付款冲正", async ({
    page,
    browser,
}) => {
    const stamp = Date.now().toString().slice(-8);
    const customerLegal = `E2E票款客户${stamp}`;
    const customerShort = `票款${stamp}`;
    const creditCode = `91110105MA0${stamp}X`.slice(0, 18).padEnd(18, "X");
    const contractNo = `HT-E2E-F07-${stamp}`;
    const invoiceNo = `F07${stamp}`;
    const flow = {
        openTotal: "",
        firstAmount: "",
        restAmount: "",
    };

    // ── 1. 销售：客户 + 合同 + 销售单提交 ────────────────────────────────
    await loginViaUi(page, accountLogin("xiaoshou") as never);
    await waitLoggedIn(page);

    await page.goto("/sales/customers");
    await expect(page.getByRole("heading", { name: "客户中心" })).toBeVisible({
        timeout: 20_000,
    });
    await page.locator("#customers-directory-create").click();
    const createCustomer = page.getByRole("dialog", { name: "新建客户" });
    await expect(createCustomer).toBeVisible({ timeout: 20_000 });
    await createCustomer.locator("#customers-form-legal-name").fill(customerLegal);
    await createCustomer.locator("#customers-form-short-name").fill(customerShort);
    await createCustomer.locator("#customers-form-credit-code").fill(creditCode);
    await createCustomer.locator("#customers-form-submit").click();
    await expectToast(page, "客户已创建");
    await expect(createCustomer).toBeHidden({ timeout: 20_000 });
    await expect(page.getByText(customerLegal).first()).toBeVisible({ timeout: 20_000 });

    await page.goto("/sales/orders?mode=create");
    await expect(page.getByText("单据头")).toBeVisible({ timeout: 20_000 });
    await page.locator("#sales-orders-create-contract-upload").click();
    const uploadContract = page.getByRole("dialog", { name: "上传合同 PDF" });
    await expect(uploadContract).toBeVisible({ timeout: 20_000 });
    await uploadContract
        .locator("#card-contracts-upload-pdf-input")
        .setInputFiles(contractPdfPath());
    await uploadContract.locator("#card-contracts-upload-contract-no").fill(contractNo);
    await chooseOption(
        page,
        uploadContract.locator("#card-contracts-upload-customer"),
        new RegExp(customerLegal),
        customerLegal,
    );
    await expect(uploadContract.locator("#card-contracts-upload-submit")).toBeEnabled({
        timeout: 20_000,
    });
    await uploadContract.locator("#card-contracts-upload-submit").click();
    await expect(uploadContract).toBeHidden({ timeout: 20_000 });
    await expect(page.getByText(customerLegal).first()).toBeVisible({ timeout: 20_000 });

    await chooseOption(
        page,
        page.locator("#sales-orders-create-header-welfare-scene"),
        "年节礼包",
    );
    const paymentTerms = page.locator("#sales-orders-create-header-payment-terms");
    const paymentTermsValue = await paymentTerms.inputValue().catch(() => "");
    if (!paymentTermsValue.trim()) {
        await chooseOption(page, paymentTerms, /货到 30 天|按合同约定/);
    }

    await page.getByRole("button", { name: "选择商品" }).first().click();
    const skuDialog = page.getByRole("dialog", { name: "选择商品" });
    await expect(skuDialog).toBeVisible({ timeout: 20_000 });
    const skuSearch = skuDialog.locator("#master-data-list-sellable-list-toolbar-search-input");
    await skuSearch.fill(SKU_NAME);
    await skuSearch.press("Enter");
    await expect(skuDialog.getByText(SKU_NAME).first()).toBeVisible({ timeout: 20_000 });
    await skuDialog.getByRole("checkbox", { name: new RegExp(SKU_NAME) }).click();
    await skuDialog.locator("#sales-orders-sku-picker-confirm").click();
    await expect(skuDialog).toBeHidden({ timeout: 20_000 });
    await expect(page.getByRole("button", { name: new RegExp(SKU_NAME) })).toBeVisible({
        timeout: 20_000,
    });
    await expect(page.getByTestId(/sales-line-procurement-owner-/)).not.toContainText(
        "暂未确定",
        { timeout: 20_000 },
    );

    await page.locator("#sales-orders-create-batch-due-date").click();
    await pickCalendarDay(page);
    await page.locator("#sales-orders-create-batch-due-date-apply").click();
    await expectToast(page, "已批量设置交期");

    await page.locator("#sales-orders-create-submit").click();
    const submitSales = page.getByRole("dialog", { name: "提交销售单" });
    await expect(submitSales).toBeVisible({ timeout: 20_000 });
    await submitSales.locator("#sales-orders-submit-confirm-confirm").click();
    await expect(page.getByRole("heading", { name: customerLegal })).toBeVisible({
        timeout: 20_000,
    });
    await expect(page.getByText("审批中").first()).toBeVisible({ timeout: 20_000 });
    // ── 2. 采购：销售单通过 → 供给分配创建采购单并立即提交审批 ──────────
    const caigou = await openRole(browser, "caigou");
    const caigouPage = caigou.page;
    await waitLoggedIn(caigouPage);
    await openWorkspaceTask(caigouPage, /销售单审批/);
    await expect(caigouPage.getByRole("button", { name: "通过" })).toBeVisible({
        timeout: 20_000,
    });
    await approveOpenTask(caigouPage);

    await gotoWorkspace(caigouPage);
    await caigouPage.locator("#workspace-family-nav-procurement").click();
    await openWorkspaceTask(caigouPage, /待供给分配/);
    await expect(caigouPage.getByRole("heading", { name: "供给分配" })).toBeVisible({
        timeout: 20_000,
    });
    await expect(caigouPage.getByText("将创建采购单")).toBeVisible({ timeout: 20_000 });
    await expect(caigouPage.getByText(/1 张/)).toBeVisible({ timeout: 20_000 });
    await caigouPage.locator("#procurement-orders-create-preview").click();
    const preview = caigouPage.getByRole("dialog", { name: "预览供给分配" });
    await expect(preview).toBeVisible({ timeout: 20_000 });
    await expect(preview.getByText("本次全部由现有库存满足")).toHaveCount(0);
    await preview.locator("#procurement-orders-create-preview-confirm").click();
    const confirmAlloc = caigouPage.getByRole("alertdialog").filter({ hasText: "确认供给分配" });
    await expect(confirmAlloc).toBeVisible({ timeout: 20_000 });
    await confirmAlloc.locator("#procurement-orders-create-confirm").click();
    await expectToast(caigouPage, /已创建 1 张采购单并提交审批|已将缺口拆成/);

    await gotoWorkspace(caigouPage);
    await caigouPage.getByRole("button", { name: /^我发起的/ }).click();
    await expect(caigouPage.getByText("采购单审批").first()).toBeVisible({ timeout: 20_000 });
    await assertNoSupplierPaymentApproval(caigouPage);

    // ── 3. 财务总监：采购单审批通过，形成应付；不得出现付款审批 ────────
    const caiwu = await openRole(browser, "caiwu");
    const caiwuPage = caiwu.page;
    await waitLoggedIn(caiwuPage);
    await caiwuPage.locator("#workspace-family-nav-approval").click();
    await openWorkspaceTask(caiwuPage, /采购单审批/);
    await approveOpenTask(caiwuPage);

    await gotoWorkspace(caiwuPage);
    await caiwuPage.locator("#workspace-family-nav-finance").click();
    await expect(caiwuPage.getByRole("button", { name: /供应商付款处理/ })).toHaveCount(0);
    await caiwuPage.locator("#workspace-family-nav-approval").click();
    await assertNoSupplierPaymentApproval(caiwuPage);
    await expect(caiwuPage.getByRole("button", { name: /采购单审批/ })).toHaveCount(0);

    await caiwuPage.goto("/finance/supplier-accounts");
    await expect(caiwuPage.getByRole("heading", { name: "供应商往来" })).toBeVisible({
        timeout: 20_000,
    });
    await expect(caiwuPage.getByText(/狮峰/).first()).toBeVisible({ timeout: 20_000 });
    await expect(caiwuPage.getByText("未结").first()).toBeVisible({ timeout: 20_000 });
    await expect(caiwuPage.locator("#supplier-payables-header-register-payment")).toBeDisabled();

    // ── 4. 出纳：W01 付款任务核对收款账户，分两次确认入账 ──────────────
    const fukuan = await openRole(browser, "fukuan");
    const fukuanPage = fukuan.page;
    await waitLoggedIn(fukuanPage);
    await fukuanPage.locator("#workspace-family-nav-approval").click();
    await assertNoSupplierPaymentApproval(fukuanPage);
    await expect(fukuanPage.getByRole("button", { name: /单据审批|付款冲正审批/ })).toHaveCount(0);

    await fukuanPage.locator("#workspace-family-nav-finance").click();
    await openWorkspaceTask(fukuanPage, /供应商付款处理/);
    await expect(fukuanPage.getByRole("heading", { name: /向.+付款/ })).toBeVisible({
        timeout: 20_000,
    });
    await expect(fukuanPage.getByText("收款户名")).toBeVisible({ timeout: 20_000 });
    await expect(fukuanPage.getByText("开户行")).toBeVisible();
    await expect(fukuanPage.getByText("收款账号")).toBeVisible();
    await expect(fukuanPage.getByText(/招商银行杭州西湖支行/).first()).toBeVisible({
        timeout: 20_000,
    });
    await expect(fukuanPage.getByRole("button", { name: "显示收款账号" })).toBeVisible();
    await fukuanPage.getByRole("button", { name: "显示收款账号" }).click();
    await expect(fukuanPage.getByText(/5719|8012/).first()).toBeVisible({ timeout: 20_000 });
    await assertNoSupplierPaymentApproval(fukuanPage);
    await expect(fukuanPage.getByRole("button", { name: "登记付款并核销" })).toBeVisible();

    const pendingPay = fukuanPage
        .locator('[aria-label="当前付款任务"]')
        .getByText(/待付/)
        .locator("xpath=..");
    await expect(pendingPay).toBeVisible({ timeout: 20_000 });
    flow.openTotal = parseAmount(await pendingPay.innerText());
    const split = splitHalf(flow.openTotal);
    flow.firstAmount = split.first;
    flow.restAmount = split.rest;

    const amountInput = fukuanPage.locator("#supplier-payables-allocation-form-amount");
    await expect(amountInput).toHaveValue(/.+/, { timeout: 20_000 });
    await amountInput.fill(flow.firstAmount);
    await uploadReceipt(fukuanPage, "bank-receipt-1.png");
    await fukuanPage.locator("#supplier-payables-allocation-form-submit").click();
    const payConfirm = fukuanPage.getByRole("alertdialog").filter({ hasText: "确认付款" });
    await expect(payConfirm).toBeVisible({ timeout: 20_000 });
    await expect(payConfirm.getByText(flow.firstAmount)).toBeVisible();
    await expect(payConfirm.getByText("提交审批")).toHaveCount(0);
    await payConfirm.locator("#supplier-payables-payment-submit-confirm-confirm").click();
    await expectToast(fukuanPage, "付款已登记");
    await expect(fukuanPage.getByText(/已过账并核销/).first()).toBeVisible({ timeout: 20_000 });

    await expect(fukuanPage.getByRole("heading", { name: /向.+付款/ })).toBeVisible({
        timeout: 20_000,
    });
    await expect(fukuanPage.getByRole("button", { name: "登记付款并核销" })).toBeVisible({
        timeout: 20_000,
    });
    await expect(fukuanPage.locator('[aria-label="当前付款任务"]')).toContainText(
        flow.restAmount,
        { timeout: 20_000 },
    );

    const amountInput2 = fukuanPage.locator("#supplier-payables-allocation-form-amount");
    await expect(amountInput2).toBeVisible({ timeout: 20_000 });
    await amountInput2.fill(flow.restAmount);
    await uploadReceipt(fukuanPage, "bank-receipt-2.png");
    await fukuanPage.locator("#supplier-payables-allocation-form-submit").click();
    const payConfirm2 = fukuanPage.getByRole("alertdialog").filter({ hasText: "确认付款" });
    await expect(payConfirm2).toBeVisible({ timeout: 20_000 });
    await payConfirm2.locator("#supplier-payables-payment-submit-confirm-confirm").click();
    await expectToast(fukuanPage, "付款已登记");

    await gotoWorkspace(fukuanPage);
    await fukuanPage.locator("#workspace-family-nav-finance").click();
    await expect(fukuanPage.getByRole("button", { name: /供应商付款处理/ })).toHaveCount(0, {
        timeout: 20_000,
    });
    await assertNoSupplierPaymentApproval(fukuanPage);

    await fukuanPage.goto("/finance/supplier-accounts?view=payment");
    await switchSupplierView(fukuanPage, "payment");
    await expect(fukuanPage.getByText("已过账").first()).toBeVisible({ timeout: 20_000 });
    await expect(fukuanPage.getByRole("button", { name: "冲正" }).first()).toBeVisible({
        timeout: 20_000,
    });
    await switchSupplierView(fukuanPage, "payable");
    await expect(fukuanPage.getByText("已结清").first()).toBeVisible({ timeout: 20_000 });

    // ── 5. 财务：登记进项发票并核销（与付款分轨） ──────────────────────
    await caiwuPage.goto("/finance/supplier-accounts");
    await expect(caiwuPage.getByRole("heading", { name: "供应商往来" })).toBeVisible({
        timeout: 20_000,
    });
    await caiwuPage.locator("#supplier-payables-header-register-invoice").click();
    const pickSupplier = caiwuPage.getByRole("dialog", { name: /选择供应商 · 登记进项发票/ });
    await expect(pickSupplier).toBeVisible({ timeout: 20_000 });
    await chooseOption(
        caiwuPage,
        pickSupplier.locator("#supplier-payables-pick-supplier-select"),
        /狮峰/,
        SUPPLIER_SHORT,
    );
    await pickSupplier.locator("#supplier-payables-pick-supplier-confirm").click();
    await expect(caiwuPage.getByRole("heading", { name: "登记进项发票" })).toBeVisible({
        timeout: 20_000,
    });
    await expect(caiwuPage.getByRole("button", { name: "提交审批" })).toHaveCount(0);
    const poolSelect = caiwuPage
        .locator('[id^="supplier-payables-allocation-pool-row-"][id$="-select"]')
        .first();
    await expect(poolSelect).toBeVisible({ timeout: 20_000 });
    if (!(await poolSelect.isChecked())) {
        await caiwuPage.locator("#supplier-payables-allocation-pool-select-all").click();
    }
    await caiwuPage.locator("#supplier-payables-allocation-pool-fill-all").click();
    const allocatedInput = caiwuPage.locator(
        '[id^="supplier-payables-allocation-pool-row-"][id$="-amount"]',
    ).first();
    await expect(allocatedInput).toHaveValue(/.+/, { timeout: 20_000 });
    const gross = parseAmount(await allocatedInput.inputValue());
    const { net, tax } = splitGross(gross);
    await caiwuPage.locator("#supplier-payables-allocation-form-gross-amount").fill(gross);
    await caiwuPage.locator("#supplier-payables-allocation-form-invoice-no").fill(invoiceNo);
    await caiwuPage.locator("#supplier-payables-allocation-form-net-amount").fill(net);
    await caiwuPage.locator("#supplier-payables-allocation-form-tax-amount").fill(tax);
    await caiwuPage.locator("#supplier-payables-allocation-form-submit").click();
    const invoiceConfirm = caiwuPage.getByRole("alertdialog").filter({
        hasText: "确认登记进项发票并核销",
    });
    await expect(invoiceConfirm).toBeVisible({ timeout: 20_000 });
    await invoiceConfirm.locator("#supplier-payables-invoice-allocate-confirm-confirm").click();
    await expect(caiwuPage.getByText("进项发票已登记").first()).toBeVisible({ timeout: 20_000 });
    await caiwuPage.locator("#supplier-payables-allocation-result-close").click();
    await switchSupplierView(caiwuPage, "purchase_invoice");
    await expect(caiwuPage.getByText(invoiceNo).first()).toBeVisible({ timeout: 20_000 });
    await expect(caiwuPage.getByText("已登记").first()).toBeVisible({ timeout: 20_000 });

    // ── 6. 负向：caiwu 不得自己提交付款冲正 ──────────────────────────
    await caiwuPage.locator("#supplier-payables-view-tabs-trigger-payment").click();
    const caiwuReverse = caiwuPage.getByRole("button", { name: "冲正" }).first();
    if (await caiwuReverse.isVisible()) {
        await caiwuReverse.click();
        const reverseDlg = caiwuPage.getByRole("dialog", { name: "发起付款冲正" });
        await expect(reverseDlg).toBeVisible({ timeout: 20_000 });
        await reverseDlg
            .locator("#supplier-payables-reversal-request-reason")
            .fill("caiwu不得提交冲正");
        await reverseDlg.locator("#supplier-payables-reversal-request-submit").click();
        const reverseConfirm = caiwuPage.getByRole("alertdialog").filter({ hasText: "提交冲正" });
        try {
            await expect(reverseConfirm).toBeVisible({ timeout: 8_000 });
            await reverseConfirm
                .locator("#supplier-payables-reversal-submit-confirm-confirm")
                .click();
        } catch {
            // 确认层未打开时，失败提示应已出现在请求弹窗或页面横幅。
        }
        await expect(
            caiwuPage.getByText(/冲正失败|岗位分离|不得|不能提交|禁止|提交人/).first(),
        ).toBeVisible({ timeout: 20_000 });
        await caiwuPage.keyboard.press("Escape");
    }

    // ── 7. 出纳提交付款冲正 → 采购确认依据 → 财务审批入账 ────────────
    await fukuanPage.goto("/finance/supplier-accounts?view=payment");
    await fukuanPage.locator("#supplier-payables-view-tabs-trigger-payment").click();
    await expect(fukuanPage.getByRole("button", { name: "冲正" }).first()).toBeVisible({
        timeout: 20_000,
    });
    await fukuanPage.getByRole("button", { name: "冲正" }).first().click();
    const reversalRequest = fukuanPage.getByRole("dialog", { name: "发起付款冲正" });
    await expect(reversalRequest).toBeVisible({ timeout: 20_000 });
    await reversalRequest
        .locator("#supplier-payables-reversal-request-reason")
        .fill("E2E 付款冲正：错付核对");
    await reversalRequest.locator("#supplier-payables-reversal-request-submit").click();
    const reversalSubmit = fukuanPage.getByRole("alertdialog").filter({ hasText: "提交冲正" });
    await expect(reversalSubmit).toBeVisible({ timeout: 20_000 });
    await reversalSubmit.locator("#supplier-payables-reversal-submit-confirm-confirm").click();
    await expect(fukuanPage.getByText(/冲正已提交审批/).first()).toBeVisible({
        timeout: 20_000,
    });

    await gotoWorkspace(caigouPage);
    await caigouPage.locator("#workspace-family-nav-approval").click();
    await openWorkspaceTask(caigouPage, /付款冲正审批/);
    await approveOpenTask(caigouPage, "采购确认冲正依据");

    await gotoWorkspace(caiwuPage);
    await caiwuPage.locator("#workspace-family-nav-approval").click();
    await openWorkspaceTask(caiwuPage, /付款冲正审批/);
    await approveOpenTask(caiwuPage, "财务总监审批");

    await fukuanPage.goto("/finance/supplier-accounts?view=payment");
    await switchSupplierView(fukuanPage, "payment");
    await expect(fukuanPage.getByText("已冲正").first()).toBeVisible({ timeout: 20_000 });
    await switchSupplierView(fukuanPage, "payable");
    await expect(fukuanPage.getByText(/未结|部分结清/).first()).toBeVisible({ timeout: 20_000 });

    await gotoWorkspace(fukuanPage);
    await fukuanPage.locator("#workspace-family-nav-finance").click();
    await expect(fukuanPage.getByRole("button", { name: /供应商付款处理/ })).toBeVisible({
        timeout: 20_000,
    });
    await assertNoSupplierPaymentApproval(fukuanPage);

    await caigou.close();
    await caiwu.close();
    await fukuan.close();
});
