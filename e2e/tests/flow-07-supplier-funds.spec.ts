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
    type Page,
} from "../helpers/test";

import { createCustomerViaUi } from "../helpers/customers";
import { headedAwareViewport } from "../helpers/headed";
import { loginViaUi, openLoggedInWorkspace } from "../helpers/login";
import { confirmSupplyAllocation, expandSourcingEditor } from "../helpers/sourcing"
import {
    approveCurrentDocument,
    chooseOption,
    expectToast,
    openWorkspaceTask,
    optionalStepVisible,
    pickCalendarDay,
    readHeaderDocumentNumber,
    selectWorkspaceFamily,
} from "../helpers/ui"

test.use(headedAwareViewport({ width: 1440, height: 960 }));
test.setTimeout(12 * 60 * 1000);

const SKU_NAME = "狮峰明前龙井礼盒";
const SUPPLIER_SHORT = "狮峰茶叶";
const RECEIPT_PNG = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    "base64",
);

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

async function gotoWorkspace(page: Page): Promise<void> {
    await page.goto("/workspace");
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: 20_000,
    });
}

async function approveOpenTask(page: Page, nodeName?: string | RegExp): Promise<void> {
    if (nodeName) {
        await expect(page.getByText(nodeName).first()).toBeVisible({ timeout: 20_000 });
    }
    await approveCurrentDocument(page);
}

function taskNameWithHint(label: string, hint: string): RegExp {
    const escaped = hint.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return new RegExp(`${label}[\\s\\S]*${escaped}|${escaped}[\\s\\S]*${label}`);
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

async function openRole(
    browser: Browser,
    role: string,
): Promise<{ page: Page; close: () => Promise<void> }> {
    const session = await openLoggedInWorkspace(browser, role);
    return { page: session.page, close: () => session.context.close() };
}

async function switchSupplierView(page: Page, view: "payable" | "payment" | "purchase_invoice") {
    // 前端 id 经 toAutomationIdSegment 归一：下划线转连字符。
    await page.locator(`#supplier-payables-view-tabs-trigger-${view.replace(/_/g, "-")}`).click();
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
    let salesOrderNo = "";
    let purchaseNo = "";

    // ── 1. 销售：客户 + 合同 + 销售单提交 ────────────────────────────────
    await loginViaUi(page, "xiaoshou");
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: 20_000,
    });

    await createCustomerViaUi(page, {
        legalName: customerLegal,
        shortName: customerShort,
        creditCode,
        paymentTermLabel: "货到 30 天",
    });
    await page.locator("#customers-directory-search").fill(customerShort);
    await page.locator("#customers-directory-search").press("Enter");
    // 客户目录行内仅展示简称与编号（法定名称只在详情页标题展示）。
    await expect(page.getByText(customerShort).first()).toBeVisible({ timeout: 20_000 });

    await page.goto("/sales/orders?mode=create");
    await expect(page.getByRole("heading", { name: /新建销售单|业务信息/ })).toBeVisible({
        timeout: 20_000,
    });
    // 上传合同按钮与占位 div 重复 id：按角色点击避开严格模式。
    await page.getByRole("button", { name: "上传合同 PDF", exact: true }).click();
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

    await page.getByRole("button", { name: "添加商品" }).first().click();
    const skuDialog = page.getByRole("dialog", { name: "添加商品" });
    await expect(skuDialog).toBeVisible({ timeout: 20_000 });
    const skuSearch = skuDialog.locator("#master-data-list-sellable-list-toolbar-search-input");
    await skuSearch.fill(SKU_NAME);
    await skuSearch.press("Enter");
    await expect(skuDialog.getByText(SKU_NAME).first()).toBeVisible({ timeout: 20_000 });
    await skuDialog.getByRole("checkbox", { name: new RegExp(SKU_NAME) }).click();
    await skuDialog.locator("#sales-orders-sku-picker-confirm").click();
    await expect(skuDialog).toBeHidden({ timeout: 20_000 });
    // 同名多处出现（搜索 chip、已选 chip）：用行内更换按钮精确命中已选行。
    await expect(
        page.getByRole("button", { name: new RegExp(`更换销售项目[\\s\\S]*${SKU_NAME}`) }).first(),
    ).toBeVisible({
        timeout: 20_000,
    });
    await expect(page.getByTestId(/sales-line-procurement-owner-/)).not.toContainText(
        "暂未确定",
        { timeout: 20_000 },
    );

    await page.locator("#sales-orders-create-batch-due-date-open").click()
    await pickCalendarDay(page, page.locator("#sales-orders-create-batch-due-date"), isoDate());
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
    salesOrderNo = await readHeaderDocumentNumber(page);
    expect(salesOrderNo.length).toBeGreaterThan(0);
    // ── 2. 采购：销售单通过 → 供给分配创建采购单并立即提交审批 ──────────
    const caigou = await openRole(browser, "caigou");
    const caigouPage = caigou.page;
    await openWorkspaceTask(caigouPage, /销售单审批/, salesOrderNo, "approval");
    await expect(caigouPage.getByRole("button", { name: /^(通过|同意审批)$/ })).toBeVisible({
        timeout: 20_000,
    });
    await approveOpenTask(caigouPage);

    await openWorkspaceTask(caigouPage, /待供给分配/, salesOrderNo, "procurement");
    await expect(caigouPage.getByRole("heading", { name: "供给分配" })).toBeVisible({
        timeout: 20_000,
    });
    await expect(caigouPage.getByText("将创建采购单")).toBeVisible({ timeout: 20_000 });
    await expect(caigouPage.getByText(/1 张/)).toBeVisible({ timeout: 20_000 });
    // 供给行缺入库目标仓时预览被校验拦截（只弹 toast 不开框）：
    // 落定方案 = 一键匹配推荐 + 应用到选中行（幂等），缺仓横幅消失才可预览。
    await expect(caigouPage.getByText("销售明细与供给方案")).toBeVisible({ timeout: 20_000 })
    const missingWarehouse = caigouPage.getByText(/请选择采购入库目标仓/)
    await expandSourcingEditor(caigouPage)
    const warehouseInput = caigouPage
        .locator('[id^="procurement-orders-create-row-"][id$="-warehouse"]')
        .first()
    await expect(warehouseInput).toBeVisible({ timeout: 20_000 })
    await chooseOption(caigouPage, warehouseInput, /BJ-TZ-01|北京通州/, "BJ-TZ-01")
    await expect(missingWarehouse).toHaveCount(0, { timeout: 20_000 })
    await confirmSupplyAllocation(caigouPage, /已创建 1 张采购单并提交审批|已将缺口拆成/)

    await caigouPage.goto("/procurement/orders");
    await expect(caigouPage.getByRole("heading", { name: "采购单", exact: true })).toBeVisible({
        timeout: 20_000,
    });
    const poSearch = caigouPage.locator("#procurement-orders-list-search");
    await poSearch.fill(salesOrderNo);
    await poSearch.press("Enter");
    const poRow = caigouPage
        .locator("#procurement-orders-list-table")
        .getByRole("row")
        .filter({ hasText: salesOrderNo });
    await expect(poRow).toHaveCount(1, { timeout: 20_000 });
    purchaseNo = ((await poRow.getByRole("button", { name: /打开采购单/ }).textContent()) ?? "").trim();
    expect(purchaseNo.length).toBeGreaterThan(0);

    await gotoWorkspace(caigouPage);
    await caigouPage.getByRole("button", { name: /^我发起的/ }).click();
    await expect(
        caigouPage.getByRole("button", { name: taskNameWithHint("采购单审批", purchaseNo) }),
    ).toBeVisible({ timeout: 20_000 });
    await assertNoSupplierPaymentApproval(caigouPage);

    // ── 3. 财务总监：采购单审批通过，形成应付；不得出现付款审批 ────────
    const caiwu = await openRole(browser, "caiwu");
    const caiwuPage = caiwu.page;
    await openWorkspaceTask(caiwuPage, /采购单审批/, purchaseNo, "approval");
    await approveOpenTask(caiwuPage);

    await gotoWorkspace(caiwuPage);
    await selectWorkspaceFamily(caiwuPage, "finance");
    await expect(
        caiwuPage.getByRole("button", { name: taskNameWithHint("供应商付款处理", purchaseNo) }),
    ).toHaveCount(0);
    await selectWorkspaceFamily(caiwuPage, "approval");
    await assertNoSupplierPaymentApproval(caiwuPage);
    await expect(
        caiwuPage.getByRole("button", { name: taskNameWithHint("采购单审批", purchaseNo) }),
    ).toHaveCount(0);

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
    await gotoWorkspace(fukuanPage);
    await selectWorkspaceFamily(fukuanPage, "approval");
    await assertNoSupplierPaymentApproval(fukuanPage);
    await expect(fukuanPage.getByRole("button", { name: /单据审批|付款冲正审批/ })).toHaveCount(0);

    await openWorkspaceTask(fukuanPage, /供应商付款处理/, purchaseNo, "finance");
    await expect(fukuanPage.getByRole("heading", { name: /向.+付款/ })).toBeVisible({
        timeout: 20_000,
    });
    await expect(fukuanPage.getByText("收款户名")).toBeVisible({ timeout: 20_000 });
    await expect(fukuanPage.getByText("开户行")).toBeVisible();
    await expect(fukuanPage.getByText("收款账号", { exact: true })).toBeVisible();
    await expect(fukuanPage.getByText(/招商银行杭州西湖支行/).first()).toBeVisible({
        timeout: 20_000,
    });
    await expect(fukuanPage.getByRole("button", { name: "显示收款账号" })).toBeVisible();
    await fukuanPage.getByRole("button", { name: "显示收款账号" }).click();
    await expect(fukuanPage.getByText(/5719|8012/).first()).toBeVisible({ timeout: 20_000 });
    await assertNoSupplierPaymentApproval(fukuanPage);
    await expect(fukuanPage.getByRole("button", { name: "登记付款并核销" })).toBeVisible();

    // “待付款”纯标签也含“待付”二字：限定带金额的行。
    const pendingPay = fukuanPage
        .locator('[aria-label="当前付款任务"]')
        .getByText(/待付\s*¥/)
        .first();
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
    // 提交是慢事务：先挂响应等待再点确认，以提交落定为准（残留 toast 会造成假通过，
    // 随后的页面跳转还会取消在途请求导致静默丢失）。
    const commit1 = fukuanPage
        .waitForResponse(
            (res) =>
                res.request().method() === "POST" &&
                res.url().includes("/admin/supplier-payments/commit"),
            { timeout: 60_000 },
        );
    await payConfirm.locator("#supplier-payables-payment-submit-confirm-confirm").click();
    const firstResponse = await commit1;
    expect(firstResponse.ok()).toBeTruthy();
    expect((await firstResponse.json()).data).toMatchObject({ status: "posted", amount: flow.firstAmount, allocated_total: flow.firstAmount, unallocated_amount: "0.00" });
    await expectToast(fukuanPage, /付款已登记/);

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
    const commit2 = fukuanPage
        .waitForResponse(
            (res) =>
                res.request().method() === "POST" &&
                res.url().includes("/admin/supplier-payments/commit"),
            { timeout: 60_000 },
        );
    await payConfirm2.locator("#supplier-payables-payment-submit-confirm-confirm").click();
    const secondResponse = await commit2;
    expect(secondResponse.ok()).toBeTruthy();
    expect((await secondResponse.json()).data).toMatchObject({ status: "posted", amount: flow.restAmount, allocated_total: flow.restAmount, unallocated_amount: "0.00" });
    await expectToast(fukuanPage, /付款已登记/);

    await gotoWorkspace(fukuanPage);
    await selectWorkspaceFamily(fukuanPage, "finance");
    await expect(
        fukuanPage.getByRole("button", { name: taskNameWithHint("供应商付款处理", purchaseNo) }),
    ).toHaveCount(0, {
        timeout: 20_000,
    });
    await assertNoSupplierPaymentApproval(fukuanPage);

    await fukuanPage.goto("/finance/supplier-accounts?view=payment");
    await switchSupplierView(fukuanPage, "payment");
    await expect(fukuanPage.getByText("已过账").first()).toBeVisible({ timeout: 20_000 });
    await fukuanPage.getByRole("row").filter({ hasText: "已过账" }).first().click();
    await expect(fukuanPage.getByRole("button", { name: "冲正" }).first()).toBeVisible({
        timeout: 20_000,
    });
    await fukuanPage.keyboard.press("Escape");
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
    // 下拉回车会在供应商 id 写入前关掉对话框，确认按钮一直禁用然后被拆掉。只点选项，再等确认可用。
    const supplierInput = pickSupplier.locator("#supplier-payables-pick-supplier-select");
    await expect(supplierInput).toBeVisible({ timeout: 20_000 });
    await supplierInput.click();
    await supplierInput.fill(SUPPLIER_SHORT);
    const supplierOption = caiwuPage
        .locator('[id^="supplier-payables-pick-supplier-select-option-"]')
        .filter({ hasText: "杭州狮峰茶叶有限公司" });
    await expect(supplierOption.first()).toBeVisible({ timeout: 20_000 });
    await supplierOption.first().click();
    const confirmSupplier = pickSupplier.locator("#supplier-payables-pick-supplier-confirm");
    await expect(confirmSupplier).toBeEnabled({ timeout: 20_000 });
    await confirmSupplier.click();
    await expect(caiwuPage.getByRole("heading", { name: "登记进项发票" })).toBeVisible({
        timeout: 20_000,
    });
    await expect(caiwuPage.getByRole("button", { name: "提交审批" })).toHaveCount(0);
    // 池内会混入其他采购单和零余额目标。多行一起核销时必须逐笔填税额，
    // 只填表头税额会让提交一直禁用。只保留本采购单的正余额行。
    // Base UI 把 id 放在隐藏 input 上，aria-label「选择 采购单号」在可见复选框上。
    const poolSection = caiwuPage.locator('section[aria-label="同供应商待核销池"]');
    const poolChecks = poolSection.locator('[role="checkbox"][aria-label^="选择"]');
    await expect(poolChecks.first()).toBeVisible({ timeout: 20_000 });
    const checkCount = await poolChecks.count();
    for (let i = 0; i < checkCount; i += 1) {
        if ((await poolChecks.nth(i).getAttribute("aria-checked")) !== "true") {
            await poolChecks.nth(i).click();
        }
    }
    await caiwuPage.locator("#supplier-payables-allocation-pool-fill-all").click();
    const amountCells = await poolSection
        .locator('input[id$="-amount"]')
        .evaluateAll((els) =>
            els.map((el) => ({
                id: el.id,
                value: (el as HTMLInputElement).value,
            })),
        );
    let grossCents = 0;
    for (const cell of amountCells) {
        const input = poolSection.locator(`[id="${cell.id}"]`);
        const card = input.locator("xpath=ancestor::div[.//*[@role='checkbox']][1]");
        const choiceLabel =
            (await card.locator('[role="checkbox"]').first().getAttribute("aria-label")) ?? "";
        const cents = Math.round(Number(parseAmount(cell.value || "0")) * 100);
        const belongsToOrder = choiceLabel.includes(purchaseNo);
        if (!belongsToOrder || cents <= 0) {
            const box = card.locator('[role="checkbox"]').first();
            if ((await box.getAttribute("aria-checked")) === "true") {
                await box.click();
            }
            continue;
        }
        grossCents += cents;
    }
    if (grossCents <= 0) throw new Error("进项发票池内无正余额目标可核销");
    const gross = (grossCents / 100).toFixed(2);
    const { net, tax } = splitGross(gross);
    await caiwuPage.locator("#supplier-payables-allocation-form-gross-amount").fill(gross);
    await caiwuPage.locator("#supplier-payables-allocation-form-invoice-no").fill(invoiceNo);
    await caiwuPage.locator("#supplier-payables-allocation-form-net-amount").fill(net);
    await caiwuPage.locator("#supplier-payables-allocation-form-tax-amount").fill(tax);
    await expect(
        caiwuPage.locator("#supplier-payables-allocation-form-submit"),
    ).toBeEnabled({ timeout: 20_000 });
    await caiwuPage.locator("#supplier-payables-allocation-form-submit").click();
    const invoiceConfirm = caiwuPage.getByRole("alertdialog").filter({
        hasText: "确认登记进项发票并核销",
    });
    await expect(invoiceConfirm).toBeVisible({ timeout: 20_000 });
    // 进项发票提交同样是慢事务：先挂响应等待再点确认，以落定为准再断言 toast。
    const invoiceCommit = caiwuPage
        .waitForResponse(
            (res) =>
                res.request().method() === "POST" &&
                res.url().includes("/admin/purchase-invoice-allocations"),
            { timeout: 60_000 },
        );
    await invoiceConfirm.locator("#supplier-payables-invoice-allocate-confirm-confirm").click();
    expect((await invoiceCommit).ok()).toBeTruthy();
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
        const reverseDlg = caiwuPage.getByRole("dialog", { name: /付款冲正/ });
        await expect(reverseDlg).toBeVisible({ timeout: 20_000 });
        await reverseDlg
            .locator("#supplier-payables-reversal-request-reason")
            .fill("caiwu不得提交冲正");
        await reverseDlg.locator("#supplier-payables-reversal-request-submit").click();
        const reverseConfirm = caiwuPage.getByRole("dialog").filter({ hasText: /提交冲正|确认提交/ });
        const deniedText = /冲正失败|岗位分离|不得|不能提交|禁止|提交人/;
        // 确认层与失败提示谁先出现走哪条：确认层未打开时，失败提示已在请求弹窗或页面横幅。
        // 竞速只认请求弹窗和横幅里的提示，避免页面上本来就有的「提交人」等字样让确认层被跳过。
        const deniedInPlace = reverseDlg
            .getByText(deniedText)
            .or(caiwuPage.getByRole("alert").filter({ hasText: deniedText }));
        if (await optionalStepVisible(reverseConfirm, deniedInPlace, 8_000)) {
            await reverseConfirm
                .locator("#supplier-payables-reversal-submit-confirm-confirm")
                .click();
        }
        await expect(caiwuPage.getByText(deniedText).first()).toBeVisible({ timeout: 20_000 });
        await caiwuPage.keyboard.press("Escape");
    }

    // ── 7. 出纳提交付款冲正 → 采购确认依据 → 财务审批入账 ────────────
    await fukuanPage.goto("/finance/supplier-accounts?view=payment");
    await fukuanPage.locator("#supplier-payables-view-tabs-trigger-payment").click();
    await fukuanPage.getByRole("row").filter({ hasText: "已过账" }).first().click();
    await expect(fukuanPage.getByRole("button", { name: "冲正" }).first()).toBeVisible({
        timeout: 20_000,
    });
    await fukuanPage.getByRole("button", { name: "冲正" }).first().click();
    const reversalRequest = fukuanPage.getByRole("dialog", { name: /付款冲正/ });
    await expect(reversalRequest).toBeVisible({ timeout: 20_000 });
    await reversalRequest
        .locator("#supplier-payables-reversal-request-reason")
        .fill("E2E 付款冲正：错付核对");
    await reversalRequest.locator("#supplier-payables-reversal-request-submit").click();
    const reversalSubmit = fukuanPage.getByRole("alertdialog", {
        name: /确认提交冲正|提交冲正/,
    });
    const reversalSubmitted = fukuanPage.getByText(/冲正已提交审批/);
    if (await optionalStepVisible(reversalSubmit, reversalSubmitted)) {
        await reversalSubmit
            .locator("#supplier-payables-reversal-submit-confirm-confirm")
            .click();
    }
    await expect(fukuanPage.getByText(/冲正已提交审批/).first()).toBeVisible({
        timeout: 20_000,
    });

    await openWorkspaceTask(caigouPage, /付款冲正审批/, undefined, "approval");
    await approveOpenTask(caigouPage, "采购确认冲正依据");

    await openWorkspaceTask(caiwuPage, /付款冲正审批/, undefined, "approval");
    await approveOpenTask(caiwuPage, "财务总监审批");

    await fukuanPage.goto("/finance/supplier-accounts?view=payment");
    await switchSupplierView(fukuanPage, "payment");
    await expect(fukuanPage.getByText("已冲正").first()).toBeVisible({ timeout: 20_000 });
    await switchSupplierView(fukuanPage, "payable");
    await expect(fukuanPage.getByText(/未结|部分结清/).first()).toBeVisible({ timeout: 20_000 });

    await gotoWorkspace(fukuanPage);
    await selectWorkspaceFamily(fukuanPage, "finance");
    await expect(
        fukuanPage.getByRole("button", { name: taskNameWithHint("供应商付款处理", purchaseNo) }),
    ).toBeVisible({
        timeout: 20_000,
    });
    await assertNoSupplierPaymentApproval(fukuanPage);

    await caigou.close();
    await caiwu.close();
    await fukuan.close();
});
