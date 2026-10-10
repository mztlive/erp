/**
 * 流程: [flow-07] 供应商票款：出纳付款任务、进项发票、核销与冲正
 * 文档: docs/erp-phase-1.md §9.2 + §6.5.4；docs/approval-workflow-contract.md §4.3
 *       （SupplierPayment=NO_APPROVAL）+ docs/workbench-workitem-contract.md §3
 *       + docs/purchase-payment-request-contract.md PPR-13（月结、按月对账统一申请入口）
 * 账号: xiaoshou（销售建客/合同/销售单）→ caigou（采购确认、供给分配、冲正确认依据）
 *       → caigoushenpi（采购单审批）；caiwu（付款申请财审、进项发票、冲正末节点；禁止自己提交冲正）
 *       → fukuan（W01 付款任务分两次确认入账、发起付款冲正）
 *
 * 文档-代码差异（测试以代码为准）:
 * - 付款申请审核通过后，指定出纳通过「登记付款」「确认登记付款」完成实际付款，付款单为「已过账」。
 * - 文档 6.5.4 画「业务部门先确认依据、财务经办再建单」；付款冲正由 fukuan 一次创建并启动审批，
 *   再由 caigou→caiwu 在 W01 审批入账。
 * - W12 不保留旧登记付款入口；财务审核人不得调用新申请付款或旧应付直付命令。
 * - 付款详情无提交审批入口；SupplierPayment 不得出现审批实例/审批任务。
 */
import { archiveContractViaUi } from "../helpers/contracts";
import { BANK_RECEIPT_FILE, openAuthorizedSupplierPaymentTask, purchasePaymentRequestCommand, supplierPaymentTaskPattern, type PurchasePaymentRequestResult } from "../helpers/payments";
import { API_BASE, apiGet, apiToken } from "../helpers/api";
import { randomUUID } from "node:crypto";
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
import { rejectFinancialOriginal, reviseFinancialOriginal } from "../helpers/financial-draft-edit"
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
    expectSourcingCounts,
} from "../helpers/ui"

test.use(headedAwareViewport({ width: 1440, height: 960 }));
test.setTimeout(12 * 60 * 1000);

const SKU_NAME = "狮峰明前龙井礼盒";
const SUPPLIER_SHORT = "狮峰茶叶";
const RECEIPT_PNG = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    "base64",
);

type SupplierFixtureDetail = {
    id: string;
    version: number;
    party_version: number;
    legal_name: string;
    short_name: string | null;
    unified_credit_code: string | null;
    maintainer_user_id: string;
    capability_codes: string[];
    capabilities: Array<{ id: string; capability_code: string; owner_user_id: string; status: string }>;
    qualifications: Array<{
        qualification_type: string; certificate_no: string; issuer: string | null;
        valid_from: string | null; valid_to: string | null; attachment_id: string | null;
        capability_ids: string[]; status: string;
    }>;
    bank_accounts: Array<{ id: string; version: number; status: string; is_default: boolean }>;
    current_profile: {
        id: string; settlement_mode: string; reconciliation_cycle: string; payment_term_snapshot: string;
        business_category: string | null; invoice_type: string; invoice_tax_rate: string | null;
        invoice_tax_rates: string[]; signing_entity_party_id: string; payment_entity_party_id: string;
    };
};

/** 正式修订供应商为月结、按月对账，再通过真实销售与供给分配冻结采购条款。 */
async function prepareMonthlySupplier(page: Page) {
    const token = await apiToken("admin");
    const list = await apiGet<{ items: Array<{ id: string; supplier_no: string }> }>(token, "/admin/suppliers", {
        keyword: "SUP-HZSF", status: "active", page: 1, page_size: 100,
    });
    const suppliers = list.items.filter((item) => item.supplier_no === "SUP-HZSF");
    expect(suppliers, "付款用例必须准备真实狮峰供应商商业资料").toHaveLength(1);
    const source = await apiGet<SupplierFixtureDetail>(token, `/admin/suppliers/${suppliers[0]!.id}`);
    expect(source.short_name).toBe(SUPPLIER_SHORT);
    expect(source.current_profile).toBeTruthy();
    const profile = source.current_profile;
    const qualifications = source.qualifications.filter((item) => item.status === "active").map((item) => ({
        qualification_type: item.qualification_type, certificate_no: item.certificate_no, issuer: item.issuer,
        valid_from: item.valid_from, valid_to: item.valid_to, attachment_id: item.attachment_id,
        capability_codes: item.capability_ids.map((id) => {
            const capability = source.capabilities.find((candidate) => candidate.id === id);
            expect(capability, "资质必须保留真实供应商能力关联").toBeTruthy();
            return capability!.capability_code;
        }),
    }));
    const saved = await purchasePaymentRequestCommand<{ supplier_id: string; revision_id: string }>(
        page, "admin", "PUT", `/admin/supplier-profiles/${source.id}`, {
            idempotency_key: randomUUID(), party_no: null, supplier_no: null,
            expected_party_version: source.party_version, expected_supplier_version: source.version,
            legal_name: source.legal_name, short_name: source.short_name, unified_credit_code: source.unified_credit_code,
            contact: null, clear_contact: false, address: null, clear_address: false,
            tax_no: null, clear_tax_profile: false, bank_account: null, clear_bank_account: false,
            settlement_mode: "monthly", reconciliation_cycle: "monthly", payment_term_snapshot: "PERIOD_MONTH_15",
            business_category: profile.business_category, invoice_type: profile.invoice_type,
            invoice_tax_rate: profile.invoice_tax_rate, invoice_tax_rates: profile.invoice_tax_rates,
            signing_entity_party_id: profile.signing_entity_party_id, payment_entity_party_id: profile.payment_entity_party_id,
            maintainer_user_id: source.maintainer_user_id, capability_codes: source.capability_codes,
            capability_owners: source.capabilities.filter((item) => source.capability_codes.includes(item.capability_code) && item.status === "active")
                .map((item) => ({ capability_code: item.capability_code, owner_user_id: item.owner_user_id })),
            qualifications, rating: null, effective_from: isoDate(), change_reason: "E2E 供应商月结及按月对账付款验证",
        },
    );
    const updated = await apiGet<SupplierFixtureDetail>(token, `/admin/suppliers/${source.id}`);
    expect(saved.supplier_id).toBe(source.id);
    expect(updated.current_profile).toMatchObject({
        id: saved.revision_id, settlement_mode: "monthly", reconciliation_cycle: "monthly", payment_term_snapshot: "PERIOD_MONTH_15",
        signing_entity_party_id: profile.signing_entity_party_id, payment_entity_party_id: profile.payment_entity_party_id,
    });
    expect(updated.bank_accounts).toEqual(source.bank_accounts);
    expect(updated.capability_codes).toEqual(source.capability_codes);
    return updated;
}

type PaymentRequestDetail = Omit<PurchasePaymentRequestResult, "lines"> & {
    supplier_id: string;
    payee_account: { account_id: string; version: number } | null;
    lines: Array<PurchasePaymentRequestResult["lines"][number] & {
        payment_terms: { payment_term_code: string; prepay_gate: boolean };
    }>;
    current_lines: Array<{
        payable_account_id: string;
        current_gross_total: string;
        current_settled_total: string;
        current_open_total: string;
        current_reserved_total: string;
        current_available_total: string;
        source_changed: boolean;
        payable_entries: Array<{ id: string; open_total: string; settled_total: string; offset_total: string }>;
    }>;
    payments: Array<{ payment_id: string; amount: string; effective_amount: string }>;
};

type PaymentTask = {
    id: string;
    business_object_type: string;
    business_object_id: string;
    work_item_type: string;
    owner_user_id: string | null;
    status: string;
    task_version: string;
    subject_version: string;
    allowed_actions: string[];
};

function amountCents(value: string): bigint {
    const [whole, fraction = ""] = value.split(".");
    return BigInt(whole!) * 100n + BigInt(fraction.padEnd(2, "0"));
}

/** 使用合法申请、账户、任务版本、应付分配及真实回单证明财审没有付款权限。 */
async function assertFinanceCannotPayApprovedRequest(page: Page, approved: PurchasePaymentRequestResult) {
    const financeToken = await apiToken("caiwu");
    const readerToken = await apiToken("caigou");
    const cashierToken = await apiToken("fukuan");
    const profile = await apiGet<{ account: string; permissions: string[] }>(financeToken, "/account/profile");
    expect(profile.account).toBe("caiwu");
    expect(profile.permissions.filter((permission) =>
        ["*", "*:*", "*:pay", "purchase_payment_request:*", "purchase_payment_request:pay"].includes(permission),
    ), "财务审核岗位不得取得采购付款申请执行权限").toEqual([]);

    const requestPath = `/admin/purchase-payment-requests/${approved.id}`;
    const request = await apiGet<PaymentRequestDetail>(readerToken, requestPath);
    expect(request.status).toBe("approved");
    for (const line of request.lines) {
        expect(line.payment_terms, "月结付款申请必须冻结真实采购月结条件").toMatchObject({
            payment_term_code: "PERIOD_MONTH_15", prepay_gate: false,
        });
    }
    expect(request.work_item_id).toBeTruthy();
    expect(request.payee_account).not.toBeNull();
    const taskPath = `/admin/work-items/${request.work_item_id}`;
    const task = await apiGet<PaymentTask>(cashierToken, taskPath);
    const cashier = await apiGet<{ userid: string }>(cashierToken, "/account/profile");
    expect(task).toMatchObject({
        id: request.work_item_id, business_object_type: "purchase_payment_request",
        business_object_id: request.id, work_item_type: "SUPPLIER_PAYMENT_EXECUTION",
        owner_user_id: cashier.userid, status: "OPEN", subject_version: String(request.submission_version),
    });
    expect(task.task_version).toMatch(/^\d+$/);
    expect(task.allowed_actions).toContain("PROCESS");
    const line = request.lines.find((candidate) =>
        amountCents(candidate.requested_amount) - amountCents(candidate.paid_amount) - amountCents(candidate.closed_amount) >= 100n,
    );
    expect(line, "负向命令仍须具有至少1.00元未付批准额度").toBeTruthy();
    const current = request.current_lines.find((candidate) => candidate.payable_account_id === line!.payable_account_id);
    expect(current).toBeTruthy();
    expect(current!.source_changed).toBe(false);
    let remaining = 100n;
    const allocations: Array<{ payable_entry_id: string; allocated_amount: string }> = [];
    for (const entry of current!.payable_entries) {
        const open = amountCents(entry.open_total);
        const amount = open < remaining ? open : remaining;
        if (amount > 0n) {
            allocations.push({ payable_entry_id: entry.id, allocated_amount: `${amount / 100n}.${String(amount % 100n).padStart(2, "0")}` });
            remaining -= amount;
        }
        if (remaining === 0n) break;
    }
    expect(remaining, "负向付款分配必须来自真实应付分录，合计恰好1.00元").toBe(0n);

    const monetaryFacts = async () => {
        const [detail, accounts, payments, currentTask] = await Promise.all([
            apiGet<PaymentRequestDetail>(readerToken, requestPath),
            Promise.all(request.lines.map((item) =>
                apiGet<{ data: unknown }>(financeToken, `/admin/payable-accounts/${item.payable_account_id}`).then((result) => result.data),
            )),
            apiGet<{ items: unknown[]; total: number }>(financeToken, "/admin/supplier-payments", {
                supplier_id: request.supplier_id, page: 1, page_size: 100,
            }),
            apiGet<PaymentTask>(cashierToken, taskPath),
        ]);
        return {
            request: {
                version: detail.version, status: detail.status, total_amount: detail.total_amount,
                submission_version: detail.submission_version, work_item_id: detail.work_item_id,
                lines: detail.lines, current_lines: detail.current_lines, payments: detail.payments,
            },
            accounts,
            payments: { items: payments.items, total: payments.total },
            task: {
                id: currentTask.id, task_version: currentTask.task_version, subject_version: currentTask.subject_version,
                status: currentTask.status, owner_user_id: currentTask.owner_user_id,
            },
        };
    };
    for (const endpoint of [`${requestPath}/payments/commit`, "/admin/supplier-payments/commit"]) {
        const before = await monetaryFacts();
        const reference = "pending-file:flow-07-finance-denied-receipt";
        const command = {
            payment_request_id: request.id, expected_request_version: request.version,
            work_item_id: task.id, expected_task_version: task.task_version, additional_work_items: [],
            expected_payee_bank_account_id: request.payee_account!.account_id,
            expected_payee_bank_account_version: request.payee_account!.version,
            payment: {
                payment_no: `F07-DENIED-${randomUUID()}`, supplier_id: request.supplier_id,
                paid_at: Math.floor(Date.now() / 1000), amount: "1.00",
                bank_reference: "财审完整付款命令负向验证", bank_receipt_asset_id: reference,
            },
            allocations, idempotency_key: randomUUID(),
        };
        const response = await page.request.post(`${API_BASE}${endpoint}`, {
            headers: { Authorization: `Bearer ${financeToken}` },
            multipart: { command: JSON.stringify(command), [reference]: BANK_RECEIPT_FILE },
            timeout: 60_000,
        });
        const envelope = await response.json() as { success: boolean; code?: string; errorMessage?: string };
        expect(response.ok(), envelope.errorMessage).toBe(false);
        expect(envelope.success, envelope.errorMessage).toBe(false);
        if (endpoint === `${requestPath}/payments/commit`) {
            expect(response.status(), "新申请付款命令须由岗位权限明确拒绝").toBe(403);
            expect(envelope.code).toBe("PERMISSION_DENIED");
        } else {
            expect([403, 409], "旧直付须因岗位权限或入口停用拒绝，不得以schema/415错误通过").toContain(response.status());
        }
        expect(envelope.errorMessage).toMatch(/权限|无权|不允许|已停止|停用|付款申请/);
        expect(await monetaryFacts(), "拒绝财审付款必须保持申请、应付、占用、实际付款和任务版本不变").toEqual(before);
    }
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
    await page.locator("#ppr-pay-bank-receipt").setInputFiles({
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
    const monthlySupplier = await prepareMonthlySupplier(page);

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
    await page.locator("#sales-orders-create-contract-upload").click();
    await archiveContractViaUi(page, { contractNo, customerName: customerLegal, pdf: contractPdfPath() })
    await expect(page.locator("#sales-orders-create-customer")).toHaveValue(new RegExp(customerLegal))

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
    await expectSourcingCounts(caigouPage, 1);
    // 本流程后续验证入库；显式选择同一供应商的入仓方案，并填写目标仓。
    await expect(caigouPage.getByRole("region", { name: "供给商品列表" })).toBeVisible({ timeout: 20_000 })
    const missingWarehouse = caigouPage.getByText(/请选择采购入库目标仓/)
    await expandSourcingEditor(caigouPage)
    const purchaseSource = caigouPage.getByRole("region", { name: "当前商品供给编辑" })
        .locator('[id$="-sourcing-option"]')
    await expect(purchaseSource).toHaveCount(1)
    await chooseOption(caigouPage, purchaseSource, /杭州狮峰.*入仓/, "入仓")
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

    // ── 3. 采购部门：采购单审批通过，形成应付；不得出现付款审批 ────────
    const purchaseApprover = await openRole(browser, "caigoushenpi");
    await openWorkspaceTask(purchaseApprover.page, /采购单审批/, purchaseNo, "approval");
    await approveOpenTask(purchaseApprover.page);
    await purchaseApprover.close();
    const purchaseRows = await apiGet<{ items: Array<{ id: string; purchase_no: string }> }>(
        await apiToken("caigou"), "/admin/purchase-orders", { q: purchaseNo, page: 1, page_size: 100 },
    );
    const monthlyPurchases = purchaseRows.items.filter((item) => item.purchase_no === purchaseNo);
    expect(monthlyPurchases).toHaveLength(1);
    const monthlyPurchase = await apiGet<{ supplier_id: string; payment_term_code: string; current_revision_id: string | null }>(
        await apiToken("caigou"), `/admin/purchase-orders/${monthlyPurchases[0]!.id}`,
    );
    expect(monthlyPurchase).toMatchObject({ supplier_id: monthlySupplier.id, payment_term_code: "PERIOD_MONTH_15" });
    expect(monthlyPurchase.current_revision_id).toBeTruthy();
    const caiwu = await openRole(browser, "caiwu");
    const caiwuPage = caiwu.page;

    await gotoWorkspace(caiwuPage);
    await selectWorkspaceFamily(caiwuPage, "finance");
    await expect(
        caiwuPage.getByRole("button", { name: supplierPaymentTaskPattern(purchaseNo) }),
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
    await expect(caiwuPage.locator("button#supplier-payables-header-register-payment")).toHaveCount(0);
    const supplierWorkspace = caiwuPage.locator('[data-slot="page-scaffold"]').filter({
        has: caiwuPage.getByRole("heading", { name: "供应商往来", exact: true }),
    });
    await expect(supplierWorkspace).toHaveCount(1);
    // Base UI 的导航按钮为带 role="button" 的真实 <a>；按业务区域与 href 验证唯一导航入口。
    const paymentRequestLink = supplierWorkspace.getByRole("group", { name: "页面操作", exact: true })
        .locator('a[href="/procurement/payment-requests"]');
    await expect(paymentRequestLink).toHaveCount(1);
    await expect(paymentRequestLink).toHaveText("采购付款申请");
    await expect(paymentRequestLink).toBeVisible({ timeout: 20_000 });
    await expect(paymentRequestLink).toHaveAttribute("href", "/procurement/payment-requests");
    await expect(caiwuPage.getByRole("button", { name: "登记付款", exact: true })).toHaveCount(0);
    await expect(caiwuPage.locator("#ppr-pay-submit")).toHaveCount(0);

    // ── 4. 出纳：W01 付款任务核对收款账户，分两次确认入账 ──────────────
    const fukuan = await openRole(browser, "fukuan");
    const fukuanPage = fukuan.page;
    await gotoWorkspace(fukuanPage);
    await selectWorkspaceFamily(fukuanPage, "approval");
    await assertNoSupplierPaymentApproval(fukuanPage);
    await expect(fukuanPage.getByRole("button", { name: /单据审批|付款冲正审批/ })).toHaveCount(0);

    const approvedRequest = await openAuthorizedSupplierPaymentTask(fukuanPage, purchaseNo);
    await assertFinanceCannotPayApprovedRequest(caiwuPage, approvedRequest);
    await expect(fukuanPage.getByRole("heading", { name: /向.+付款/ })).toBeVisible({
        timeout: 20_000,
    });
    const paymentTask = fukuanPage.getByLabel("当前付款任务");
    await expect(paymentTask.getByText("收款户名")).toBeVisible({ timeout: 20_000 });
    await expect(paymentTask.getByText("开户行")).toBeVisible();
    await expect(paymentTask.getByText("收款账号", { exact: true })).toBeVisible();
    await expect(paymentTask.getByText(/招商银行/).first()).toBeVisible({ timeout: 20_000 });
    await expect(paymentTask.getByRole("button", { name: "显示收款账号" })).toBeVisible();
    await paymentTask.getByRole("button", { name: "显示收款账号" }).click();
    await expect(fukuanPage.getByText(/5719|8012/).first()).toBeVisible({ timeout: 20_000 });
    await assertNoSupplierPaymentApproval(fukuanPage);
    await expect(fukuanPage.locator("#ppr-pay-submit")).toBeVisible();

    const pendingAmount = fukuanPage.locator('[id^="ppr-pay-line-"]').first();
    await expect(pendingAmount).toHaveValue("", { timeout: 20_000 });
    flow.openTotal = approvedRequest.lines[0]!.requested_amount;
    const split = splitHalf(flow.openTotal);
    flow.firstAmount = split.first;
    flow.restAmount = split.rest;

    const amountInput = fukuanPage.locator('[id^="ppr-pay-line-"]').first();
    await amountInput.fill(flow.firstAmount);
    await uploadReceipt(fukuanPage, "bank-receipt-1.png");
    await fukuanPage.locator("#ppr-pay-submit").click();
    const payConfirm = fukuanPage.getByRole("alertdialog").filter({ hasText: "确认登记付款" });
    await expect(payConfirm).toBeVisible({ timeout: 20_000 });
    await expect(payConfirm.getByText(flow.firstAmount)).toBeVisible();
    await expect(payConfirm.getByText("提交审批")).toHaveCount(0);
    // 提交是慢事务：先挂响应等待再点确认，以提交落定为准（残留 toast 会造成假通过，
    // 随后的页面跳转还会取消在途请求导致静默丢失）。
    const commit1 = fukuanPage
        .waitForResponse(
            (res) =>
                res.request().method() === "POST" &&
                /\/admin\/purchase-payment-requests\/[^/]+\/payments\/commit/.test(res.url()),
            { timeout: 60_000 },
        );
    await payConfirm.locator("#ppr-pay-confirm-confirm").click();
    const firstResponse = await commit1;
    expect(firstResponse.ok()).toBeTruthy();
    expect((await firstResponse.json()).data).toMatchObject({ status: "posted", amount: flow.firstAmount, allocated_total: flow.firstAmount, unallocated_amount: "0.00" });

    await expect(fukuanPage.getByRole("heading", { name: /向.+付款/ })).toBeVisible({
        timeout: 20_000,
    });
    await expect(fukuanPage.locator("#ppr-pay-submit")).toBeVisible({
        timeout: 20_000,
    });
    await expect(fukuanPage.locator('[aria-label="当前付款任务"]')).toContainText(
        flow.restAmount,
        { timeout: 20_000 },
    );

    const amountInput2 = fukuanPage.locator('[id^="ppr-pay-line-"]').first();
    await expect(amountInput2).toBeVisible({ timeout: 20_000 });
    await amountInput2.fill(flow.restAmount);
    await uploadReceipt(fukuanPage, "bank-receipt-2.png");
    await fukuanPage.locator("#ppr-pay-submit").click();
    const payConfirm2 = fukuanPage.getByRole("alertdialog").filter({ hasText: "确认登记付款" });
    await expect(payConfirm2).toBeVisible({ timeout: 20_000 });
    const commit2 = fukuanPage
        .waitForResponse(
            (res) =>
                res.request().method() === "POST" &&
                /\/admin\/purchase-payment-requests\/[^/]+\/payments\/commit/.test(res.url()),
            { timeout: 60_000 },
        );
    await payConfirm2.locator("#ppr-pay-confirm-confirm").click();
    const secondResponse = await commit2;
    expect(secondResponse.ok()).toBeTruthy();
    expect((await secondResponse.json()).data).toMatchObject({ status: "posted", amount: flow.restAmount, allocated_total: flow.restAmount, unallocated_amount: "0.00" });

    await gotoWorkspace(fukuanPage);
    await selectWorkspaceFamily(fukuanPage, "finance");
    await expect(
        fukuanPage.getByRole("button", { name: supplierPaymentTaskPattern(purchaseNo) }),
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

    // ── 5. 开票岗：登记进项发票并核销（与付款分轨） ──────────────────────
    const invoice = await openRole(browser, "kaipiao");
    const invoicePage = invoice.page;
    await invoicePage.goto("/finance/supplier-accounts");
    await expect(invoicePage.getByRole("heading", { name: "供应商往来" })).toBeVisible({
        timeout: 20_000,
    });
    await invoicePage.locator("#supplier-payables-header-register-invoice").click();
    const pickSupplier = invoicePage.getByRole("dialog", { name: /选择供应商 · 登记进项发票/ });
    await expect(pickSupplier).toBeVisible({ timeout: 20_000 });
    // 下拉回车会在供应商 id 写入前关掉对话框，确认按钮一直禁用然后被拆掉。只点选项，再等确认可用。
    const supplierInput = pickSupplier.locator("#supplier-payables-pick-supplier-select");
    await expect(supplierInput).toBeVisible({ timeout: 20_000 });
    await supplierInput.click();
    await supplierInput.fill(SUPPLIER_SHORT);
    const supplierOption = invoicePage
        .locator('[id^="supplier-payables-pick-supplier-select-option-"]')
        .filter({ hasText: "杭州狮峰茶叶有限公司" });
    await expect(supplierOption.first()).toBeVisible({ timeout: 20_000 });
    await supplierOption.first().click();
    const confirmSupplier = pickSupplier.locator("#supplier-payables-pick-supplier-confirm");
    await expect(confirmSupplier).toBeEnabled({ timeout: 20_000 });
    await confirmSupplier.click();
    await expect(invoicePage.getByRole("heading", { name: "登记进项发票" })).toBeVisible({
        timeout: 20_000,
    });
    await expect(invoicePage.getByRole("button", { name: "提交审批" })).toHaveCount(0);
    // 池内会混入其他采购单和零余额目标。多行一起核销时必须逐笔填税额，
    // 只填表头税额会让提交一直禁用。只保留本采购单的正余额行。
    // Base UI 把 id 放在隐藏 input 上，aria-label「选择 采购单号」在可见复选框上。
    const poolSection = invoicePage.locator('section[aria-label="同供应商待核销池"]');
    const poolChecks = poolSection.locator('[role="checkbox"][aria-label^="选择"]');
    await expect(poolChecks.first()).toBeVisible({ timeout: 20_000 });
    const checkCount = await poolChecks.count();
    for (let i = 0; i < checkCount; i += 1) {
        if ((await poolChecks.nth(i).getAttribute("aria-checked")) !== "true") {
            await poolChecks.nth(i).click();
        }
    }
    await invoicePage.locator("#supplier-payables-allocation-pool-fill-all").click();
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
    await invoicePage.locator("#supplier-payables-allocation-form-gross-amount").fill(gross);
    await invoicePage.locator("#supplier-payables-allocation-form-invoice-no").fill(invoiceNo);
    await invoicePage.locator("#supplier-payables-allocation-form-net-amount").fill(net);
    await invoicePage.locator("#supplier-payables-allocation-form-tax-amount").fill(tax);
    await expect(
        invoicePage.locator("#supplier-payables-allocation-form-submit"),
    ).toBeEnabled({ timeout: 20_000 });
    await invoicePage.locator("#supplier-payables-allocation-form-submit").click();
    const invoiceConfirm = invoicePage.getByRole("alertdialog").filter({
        hasText: "确认登记进项发票并核销",
    });
    await expect(invoiceConfirm).toBeVisible({ timeout: 20_000 });
    // 进项发票提交同样是慢事务：先挂响应等待再点确认，以落定为准再断言 toast。
    const invoiceCommit = invoicePage
        .waitForResponse(
            (res) =>
                res.request().method() === "POST" &&
                res.url().includes("/admin/purchase-invoice-allocations"),
            { timeout: 60_000 },
        );
    await invoiceConfirm.locator("#supplier-payables-invoice-allocate-confirm-confirm").click();
    expect((await invoiceCommit).ok()).toBeTruthy();
    await expect(invoicePage.getByText("进项发票已登记").first()).toBeVisible({ timeout: 20_000 });
    await invoicePage.locator("#supplier-payables-allocation-result-close").click();
    await switchSupplierView(invoicePage, "purchase_invoice");
    await expect(invoicePage.getByText(invoiceNo).first()).toBeVisible({ timeout: 20_000 });
    await expect(invoicePage.getByText("已登记").first()).toBeVisible({ timeout: 20_000 });

    // ── 6. 负向：caiwu 不得自己提交付款冲正 ──────────────────────────
    await caiwuPage.goto("/finance/supplier-accounts?view=payment");
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
    const reversalCommitted = fukuanPage.waitForResponse(
        (response) => response.request().method() === "POST" && new URL(response.url()).pathname === "/admin/payment-reversals/commit",
        { timeout: 90_000 },
    )
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
    const reversalResponse = await reversalCommitted
    expect(reversalResponse.ok(), await reversalResponse.text()).toBeTruthy()
    const reversal = (await reversalResponse.json()).data as { id: string; reversal_no: string }
    expect(reversal.id).toBeTruthy()
    expect(reversal.reversal_no).toBeTruthy()

    await openWorkspaceTask(caigouPage, /付款冲正审批/, reversal.reversal_no, "approval");
    await rejectFinancialOriginal(caigouPage, "付款冲正原因需补齐，修改原冲正单后重提")
    await reviseFinancialOriginal(fukuanPage, {
        kind: "payment_reversal",
        id: reversal.id,
        documentNo: reversal.reversal_no,
        changedText: "已核对原采购付款回单，按原金额冲正错付款",
    })
    await openWorkspaceTask(caigouPage, /付款冲正审批/, reversal.reversal_no, "approval");
    await approveOpenTask(caigouPage, "采购确认冲正依据");

    await openWorkspaceTask(caiwuPage, /付款冲正审批/, reversal.reversal_no, "approval");
    await approveOpenTask(caiwuPage, "财务总监审批");

    await fukuanPage.goto("/finance/supplier-accounts?view=payment");
    await switchSupplierView(fukuanPage, "payment");
    await expect(fukuanPage.getByText("已冲正").first()).toBeVisible({ timeout: 20_000 });
    await switchSupplierView(fukuanPage, "payable");
    await expect(fukuanPage.getByText(/未结|部分结清/).first()).toBeVisible({ timeout: 20_000 });

    await gotoWorkspace(fukuanPage);
    await selectWorkspaceFamily(fukuanPage, "finance");
    await expect(
        fukuanPage.getByRole("button", { name: supplierPaymentTaskPattern(purchaseNo) }),
    ).toBeVisible({
        timeout: 20_000,
    });
    await assertNoSupplierPaymentApproval(fukuanPage);

    await caigou.close();
    await caiwu.close();
    await invoice.close();
    await fukuan.close();
});
