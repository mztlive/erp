/**
 * [flow-07] 供应商票款：付款、进项发票、核销与冲正
 *
 * 文档章节:
 *   - docs/erp-phase-1.md §9.2「供应商侧」: 采购单最终审批通过后形成应付；每次实际付款先
 *     形成供应商付款草稿，提交后按 SupplierPayment 已发布定义审批，末节点通过并过账后才
 *     计入已付与付款门禁；付款与进项发票分别记录和核销；同一供应商一张付款单可核销多张
 *     采购单应付（多对多）。
 *   - docs/erp-phase-1.md §6.5.4「资金与发票纠错」: 供应商侧纠错单据含付款冲正单；
 *     退款与收付款冲正为 PROCESS_REQUIRED，红票 Invoice 为 NO_APPROVAL 不进入审批实例；
 *     过账后原子冲减原付款，原收付款与发票记录保留。
 *   - docs/erp-phase-1.md §7.4「采购单规则」: 销售单生效后在 W08 创建采购单（选源在建单时完成）。
 *
 * 使用账号（密码均 123456）:
 *   - xiaoshou（销售）: 新建客户、上传合同 PDF、创建并提交销售单
 *   - caigou（采购）  : 审批销售单（SalesOrder=采购确认）、创建并提交采购单、
 *                      审批供应商付款（SupplierPayment=采购复核）、审批付款冲正（PaymentReversal=采购复核）
 *   - caiwu（财务）   : 审批采购单（PurchaseOrder=财务审核）、W12 登记供应商付款、登记进项发票、发起付款冲正
 *
 * 已发布审批定义（见 e2e/scripts/publish-approval-definitions.mjs）:
 *   SalesOrder=采购确认(caigou)；PurchaseOrder=财务审核(caiwu)；
 *   SupplierPayment=采购复核(caigou)；PaymentReversal=采购复核(caigou)；Invoice=NO_APPROVAL。
 * 提交人不得审批自己的单据。
 *
 * 文档-代码差异（以代码为准）:
 * 1. doc: 待办审批在 W02 统一待办（/workspace/tasks）完成；
 *    code: app/(workspace)/workspace/tasks/page.tsx 将 /workspace/tasks permanentRedirect
 *    到唯一工作台 /workspace，审批决定在 /workspace 右侧详情区完成（workspace-home-page.tsx）。
 * 2. doc(ui-glossary G5): 资金/库存类单据 POSTED 界面禁用「过账」表述；
 *    code: 供应商付款/付款冲正状态徽标实际显示「已过账」「已冲正」
 *    （backend/entities/src/payable/supplier_payment.rs、returns/payment_reversal.rs 的
 *    status.label()），本 spec 按代码断言。
 * 3. doc §7.4: 销售单生效后按采购二次确认创建依据在 W08 选源建采购单；
 *    code: 选源建单已可用（creation_basis 恢复），采购单从创建依据弹窗创建。
 * 4. doc §9.2: 付款与进项发票「分别记录和核销」；
 *    code: W12 同一「核销工作区」按 track=payment/purchase_invoice 区分付款核销/进项票核销
 *    （allocation-session.tsx），记录与分配明细独立——语义一致。
 */

import { test, expect, type Locator, type Page } from "@playwright/test"
import path from "path"

import { api, apiLogin } from "../helpers/api"
import { createSinglePageAccountSwitcher } from "../helpers/login"
import {
    clickButton,
    completePurchaseOrderCreate,
    expectTableRow,
    gotoPage,
    pickOption,
} from "../helpers/ui"
import { approveWorkspaceTaskByDocumentNo } from "../helpers/workspace"

// ---------------------------------------------------------------------------
// 流程内专用小工具（不写入 helpers，避免影响其他 spec）
// ---------------------------------------------------------------------------

const CONTRACT_PDF = path.join(__dirname, "..", "fixtures", "sample-contract.pdf")

/** 当前时间戳+随机后缀，保证每个流程内编号唯一。 */
function stamp(): string {
    return `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`
}

function pad(n: number): string {
    return String(n).padStart(2, "0")
}

/** 今天 YYYY-MM-DD。 */
function todayStr(): string {
    const d = new Date()
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

/** 今天偏移 N 天后的 YYYY-MM-DD。 */
function addDaysStr(days: number): string {
    const d = new Date()
    d.setDate(d.getDate() + days)
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

/** 取最后可见弹窗（Dialog 或 AlertDialog；弹窗与抽屉同开时优先 AlertDialog）。 */
async function lastDialog(page: Page): Promise<Locator> {
    return page
        .locator('[role="alertdialog"]:visible, [role="dialog"]:visible')
        .last()
}

/** 点击确认弹窗内按钮并等待弹窗关闭（弹窗可能延迟出现，按按钮文本等待）。 */
async function confirmDialog(page: Page, name: string | RegExp): Promise<void> {
    const button = page
        .locator('[role="alertdialog"]:visible, [role="dialog"]:visible')
        .getByRole("button", { name })
        .first()
    await expect(button).toBeVisible({ timeout: 20_000 })
    await button.click()
    await expect(button).not.toBeVisible({ timeout: 20_000 }).catch(() => {})
}

/** 表单字段容器：Field 根节点（data-slot=field）内包含指定 label 文本。 */
function fieldBox(page: Page, label: string): Locator {
    return page
        .locator('[data-slot="field"]')
        .filter({ has: page.getByText(label, { exact: true }) })
        .first()
}

/** 远程搜索组合框：点开输入框、输入查询词、点选包含目标文本的选项。 */
async function pickRemoteOption(
    page: Page,
    input: Locator,
    query: string,
    optionText: string,
): Promise<void> {
    await input.click()
    await input.fill(query)
    const box = page.locator('[role="listbox"]').last()
    const option = box.getByText(optionText, { exact: false }).first()
    await expect(option).toBeVisible({ timeout: 20_000 })
    await option.click()
}

const MONTH_NAMES = [
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
]

/** 在已打开的日历弹层中导航到目标月份并点选日期（react-day-picker v10，默认 en-US）。 */
async function pickDayInCalendar(cal: Locator, ymd: string): Promise<void> {
    const [y, m, d] = ymd.split("-").map(Number)
    const want = `${MONTH_NAMES[m - 1]} ${y}`
    const caption = cal.locator('[class*="caption_label"]').first()
    for (let i = 0; i < 25; i += 1) {
        const cap = ((await caption.textContent()) ?? "").trim()
        if (cap === want) break
        const parts = cap.split(" ")
        const capMonth = MONTH_NAMES.indexOf(parts[0] ?? "")
        const capYear = Number(parts[1])
        const diff = (y - capYear) * 12 + (m - 1 - capMonth)
        await cal
            .getByRole("button", {
                name: diff > 0 ? "Go to the Next Month" : "Go to the Previous Month",
            })
            .click()
    }
    // 日按钮的 accessible name 是完整日期（如 "Saturday, August 21st, 2026"），
    // 今天的按钮带 "Today, " 前缀；正则锚定「, 月 日, 年」避免跨月单元格误匹配
    const dayBtn = cal
        .getByRole("button", {
            name: new RegExp(
                `(^|, )${MONTH_NAMES[m - 1]} ${d}(st|nd|rd|th)?, ${y}`,
            ),
        })
        .first()
    // 表单默认值恰为目标日（如开票日期默认今天）时，单击会反选清空
    // （react-day-picker 切换语义）；命中已选日则连点两次恢复选中。
    // 注意：aria-selected 在 gridcell 上，按钮上的选中标记是 data-selected-single
    const wasSelected =
        (await dayBtn.getAttribute("data-selected-single")) === "true"
    await dayBtn.click()
    if (wasSelected) {
        await dayBtn.click()
    }
}

/** 日期字段（DatePicker 弹层日历）。 */
async function pickDate(page: Page, label: string, ymd: string): Promise<void> {
    const box = fieldBox(page, label)
    await box.getByRole("button").first().click()
    const cal = page.locator('[data-slot="calendar"]').last()
    await expect(cal).toBeVisible({ timeout: 10_000 })
    await pickDayInCalendar(cal, ymd)
}

/**
 * W01 工作台按可见业务单号找到审批任务并提交「通过」。
 */
async function approveTask(page: Page, documentNo: string): Promise<void> {
    const task = await approveWorkspaceTaskByDocumentNo(page, documentNo)
    await expect(task).not.toBeVisible({ timeout: 30_000 })
}

// ---------------------------------------------------------------------------
// 主流程
// ---------------------------------------------------------------------------

test("供应商票款：付款→审批→过账→核销应付；进项发票登记与核销；付款冲正→审批→过账", async ({
    page,
    request,
}) => {
    test.setTimeout(600_000)

    const switchAccount = createSinglePageAccountSwitcher(page)
    const salesPage = page
    const procurementPage = page
    const finPage = page

    const financeToken = await apiLogin(request, "finance")
    const salesToken = await apiLogin(request, "sales")
    const procurementToken = await apiLogin(request, "procurement")

    // ---------- 主数据发现（reset 保留：结算主体 / 公司商品池 SKU / 供应商） ----------
    const parties = await api<{ items: Array<{ id: string; party_no: string }> }>(
        request,
        "GET",
        "/admin/parties",
        {
            token: salesToken,
            query: { status: "active", page: 1, page_size: 20, sort_by: "party_no", sort_dir: "asc" },
        },
    )
    const party = parties.items?.[0]
    expect(party, "保留主数据应至少有一个启用结算主体（/admin/parties）").toBeTruthy()
    const partyNo = party!.party_no

    const skus = await api<{
        items: Array<{ sku_id: string; sku_no: string; name: string; product_kind: string }>
    }>(request, "GET", "/admin/sellable-skus", {
        token: salesToken,
        query: { page: 1, page_size: 20 },
    })
    const sku = (skus.items ?? []).find(
        (s) => (s.product_kind ?? "").toUpperCase() !== "VOUCHER",
    )
    expect(sku, "保留主数据应至少有一个可售实物/服务 SKU（/admin/sellable-skus）").toBeTruthy()

    const suppliers = await api<{
        items: Array<{ id: string; supplier_no: string; legal_name?: string; short_name?: string }>
    }>(request, "GET", "/admin/suppliers", {
        token: financeToken,
        query: { status: "active", page: 1, page_size: 20, sort_by: "supplier_no", sort_dir: "asc" },
    })
    const supplier = suppliers.items?.[0]
    expect(supplier, "保留主数据应至少有一个启用供应商（/admin/suppliers）").toBeTruthy()
    const supplierName = (
        supplier!.legal_name || supplier!.short_name || supplier!.supplier_no
    ) as string

    // =====================================================================
    // 阶段 1：销售侧准备——客户 → 合同(PDF) → 销售单 → 提交 → 采购审批 → 生效
    // =====================================================================
    const uniq = stamp()
    const customerName = `E2E票款客户${uniq}`
    const legalName = `E2E测试票款客户${uniq}有限公司`
    const creditCode = `91330100${String(Date.now()).slice(0, 10)}`
    const contractNo = `HT-E2E-${uniq}`

    await switchAccount("sales")
    await gotoPage(salesPage, "/sales/customers")

    // --- 新建客户 ---
    await clickButton(salesPage, "新建客户")
    let dlg = await lastDialog(salesPage)
    await expect(dlg.getByText("新建客户", { exact: true })).toBeVisible({ timeout: 20_000 })
    await dlg.getByLabel("法定名称").fill(legalName)
    await dlg.getByLabel("客户简称").fill(customerName)
    await dlg.getByLabel("统一社会信用代码").fill(creditCode)
    await pickOption(salesPage, dlg.getByLabel("默认付款条件"), "先款 100%")
    await dlg.getByRole("button", { name: "创建客户" }).click()
    await expect(salesPage.getByText("客户已创建")).toBeVisible({ timeout: 20_000 })
    await salesPage.getByRole("link", { name: customerName, exact: true }).click()
    await salesPage.waitForURL(/\/sales\/customers\/[^/]+$/, { timeout: 20_000 })

    // --- 建单页内直接上传合同 PDF（最快路径：销售单创建页的加号入口） ---
    await gotoPage(salesPage, "/sales/orders?mode=create")
    await salesPage.getByRole("button", { name: "上传合同 PDF" }).click()
    dlg = await lastDialog(salesPage)
    await expect(
        dlg.getByRole("heading", { name: "上传合同 PDF" }),
    ).toBeVisible({ timeout: 20_000 })
    await dlg.locator('input[type="file"]').setInputFiles(CONTRACT_PDF)
    await dlg.getByLabel("合同编号").fill(contractNo)
    await pickRemoteOption(
        salesPage,
        dlg.getByPlaceholder("搜索客户编号或名称"),
        legalName,
        legalName,
    )
    await pickRemoteOption(salesPage, dlg.getByPlaceholder("搜索结算主体"), partyNo, partyNo)
    await pickOption(salesPage, dlg.getByLabel("付款条件"), "先款 100%")
    await confirmDialog(salesPage, "上传并归档")
    // 上传成功回调自动选中合同并带出客户/结算主体/付款条件
    await expect(
        salesPage.getByText(new RegExp(`${contractNo}@v\\d+`)).first(),
    ).toBeVisible({
        timeout: 20_000,
    })

    // --- 填写销售单头与明细 ---
    await pickOption(salesPage, salesPage.getByLabel("福利场景"), "年节礼包")
    await pickOption(salesPage, salesPage.getByLabel("付款条件"), "先款 100%")
    await pickDate(salesPage, "履约期限", addDaysStr(30))
    await pickRemoteOption(salesPage, salesPage.getByLabel("商品"), sku!.name, sku!.name)
    await salesPage.getByLabel("含税单价").fill("1000.00")
    await pickDate(salesPage, "交付日期", addDaysStr(30))

    // --- 提交销售单（SalesOrder=采购确认） ---
    await salesPage.getByRole("button", { name: "提交", exact: true }).click()
    dlg = await lastDialog(salesPage)
    await expect(dlg.getByText("确认提交销售单", { exact: true })).toBeVisible({ timeout: 20_000 })
    await confirmDialog(salesPage, "确认提交")
    await salesPage.waitForURL(/\/sales\/orders\/[^/]+$/, { timeout: 20_000 })
    const salesOrderId = salesPage.url().split("/").pop()!
    expect(salesOrderId).toBeTruthy()
    const pendingOrder = await api<{ order_no: string }>(
        request,
        "GET",
        `/admin/sales-orders/${salesOrderId}`,
        { token: salesToken },
    )
    expect(pendingOrder.order_no, "提交后应分配销售单号").toBeTruthy()

    // --- caigou 在工作台审批销售单 ---
    await switchAccount("procurement")
    await gotoPage(procurementPage, "/workspace")
    await approveTask(procurementPage, pendingOrder.order_no)

    // 断言：销售单已生效（后端主状态）
    const orderDetail = await api<{ commercial_status?: string }>(
        request,
        "GET",
        `/admin/sales-orders/${salesOrderId}`,
        { token: salesToken },
    )
    expect(String(orderDetail.commercial_status ?? "").toUpperCase()).toBe("EFFECTIVE")

    // =====================================================================
    // 阶段 2：caigou 创建采购单并提交 → caiwu 财务审核 → 生效形成应付
    // =====================================================================
    await gotoPage(procurementPage, "/procurement/orders")
    await clickButton(procurementPage, "新建采购单")
    await expect(procurementPage).toHaveURL(/mode=create/, { timeout: 20_000 })
    const noBasis = procurementPage.getByText("当前没有可建采购依据")
    if (await noBasis.isVisible().catch(() => false)) {
        throw new Error(
            "阻塞点：当前没有可建采购依据，无法按文档 §7.4 创建采购单，供应商应付无从形成。",
        )
    }
    await completePurchaseOrderCreate(procurementPage)
    await procurementPage.waitForURL(/\/procurement\/orders\/[^/]+\?mode=edit/, {
        timeout: 30_000,
    })
    const purchaseOrderId = procurementPage.url().split("/").pop()!.split("?")[0]!
    expect(purchaseOrderId).toBeTruthy()

    // 明细行来自采购二次确认分行（预填）；直接提交审批
    await expect(procurementPage.getByText("采购草稿", { exact: true })).toBeVisible({
        timeout: 20_000,
    })
    await procurementPage.getByRole("button", { name: "提交审批", exact: true }).click()
    dlg = await lastDialog(procurementPage)
    await expect(dlg.getByText("确认提交采购单", { exact: true })).toBeVisible({
        timeout: 20_000,
    })
    await confirmDialog(procurementPage, "确认提交")
    // 提交成功进入详情页：结果面板出现「撤回审批」入口。
    // 注意：确认对话框自身含「草稿→审批中」文案，不能以文本「审批中」断言提交成功
    await expect(
        procurementPage.getByRole("button", { name: "撤回审批" }).first(),
    ).toBeVisible({ timeout: 30_000 })
    // 提交成功后才分配正式采购单号（当前后端为 "PO-<UUID>"，无自增编号）
    const headerText = await procurementPage
        .locator('[data-slot="document-header"]')
        .innerText()
    const poNo = headerText.match(/PO[0-9A-Za-z-]+/)?.[0]
    expect(poNo, "提交后应分配采购单号").toBeTruthy()

    // --- caiwu 在工作台审批采购单（PurchaseOrder=财务审核） ---
    await switchAccount("finance")
    await gotoPage(finPage, "/workspace")
    await approveTask(finPage, poNo!)

    // 断言：采购单已生效（后端主状态），应付随后在 W12 出现
    const poDetail = await api<
        { status?: string; supplier_name?: string; supplier_id?: string }
    >(
        request,
        "GET",
        `/admin/purchase-orders/${purchaseOrderId}`,
        { token: procurementToken },
    )
    expect(String(poDetail.status ?? "").toUpperCase()).toBe("EFFECTIVE")
    // 供应商名/ID 以采购单冻结快照为准（结算主体→供应商链，而非供应商列表首项）
    const poSupplierName = poDetail.supplier_name ?? supplierName
    const poSupplierId = poDetail.supplier_id ?? supplier!.id
    expect(poSupplierName, "采购单应带出供应商名称").toBeTruthy()
    expect(poSupplierId, "采购单应带出供应商 ID").toBeTruthy()

    // =====================================================================
    // 阶段 3：财务登记供应商付款 → 提交 → 采购复核 → 过账 → 应付结清
    // =====================================================================
    await gotoPage(finPage, "/finance/supplier-accounts")
    // 应付台账出现该供应商的应付（行文本含供应商名与采购单号）
    const payableRow = await expectTableRow(finPage, poSupplierName, { timeout: 30_000 })
    await expect(payableRow.getByText("未结", { exact: true })).toBeVisible({ timeout: 20_000 })

    // --- 登记付款：选择供应商 → 核销工作区 ---
    await clickButton(finPage, "登记付款")
    dlg = await lastDialog(finPage)
    await expect(dlg.getByText("选择供应商 · 登记付款", { exact: true })).toBeVisible({
        timeout: 20_000,
    })
    await pickRemoteOption(finPage, dlg.getByPlaceholder("选择供应商"), poSupplierName, poSupplierName)
    await confirmDialog(finPage, "进入本次核销")

    // --- 填写付款事实并核销分配 ---
    await expect(finPage.getByRole("heading", { name: `核销 · ${poSupplierName}` })).toBeVisible({
        timeout: 30_000,
    })
    await expect(finPage.getByText("付款核销", { exact: true })).toBeVisible()
    const pool = finPage.locator("#alloc-pool")
    await expect(pool).toBeVisible({ timeout: 20_000 })
    const poolText = await pool.innerText()
    expect(poolText, "核销池应包含采购单应付").toContain(poNo!)
    // 勾选应付目标：本次分配按开放余额自动带出
    await finPage.getByLabel(`选择 ${poNo!}`).check()
    await expect(finPage.getByLabel("本次分配")).not.toHaveValue("")
    // 付款事实：金额取核销池「开放余额」（含税），时间默认今天
    const openTotal = poolText.match(/¥\s*([\d,]+\.\d{2})/)?.[1]
    expect(openTotal, "核销池应展示开放余额").toBeTruthy()
    await finPage.getByLabel("付款金额（含税）").fill(openTotal!)
    await finPage.getByLabel("银行流水引用").fill(`E2E-PAY-${uniq}`)

    // --- 提交付款（SupplierPayment=采购复核） ---
    await finPage.getByRole("button", { name: "确认登记并核销" }).click()
    dlg = await lastDialog(finPage)
    await expect(dlg.getByText("确认提交付款", { exact: true })).toBeVisible({ timeout: 20_000 })
    await confirmDialog(finPage, "确认提交")
    await expect(finPage.getByText("付款已提交审批")).toBeVisible({ timeout: 20_000 })

    // 取付款单号与 ID（工作台按可见单号定位；单号前端生成 FK-<8位>，见 payments.ts）
    const pmNo = (await finPage.getByText(/FK-[0-9a-z]{8}/).first().innerText()).match(
        /FK-[0-9a-z]{8}/,
    )?.[0]
    expect(pmNo, "提交后应分配付款单号").toBeTruthy()
    const payments = await api<
        { items: Array<{ id: string; payment_no: string; status: string }> }
    >(request, "GET", "/admin/supplier-payments", {
        token: financeToken,
        query: { payment_no: pmNo, page: 1, page_size: 10 },
    })
    const payment = payments.items?.[0]
    expect(payment, "提交后应存在供应商付款单").toBeTruthy()
    expect(payment!.payment_no).toBe(pmNo)
    const paymentId = payment!.id

    await finPage.getByRole("button", { name: "回到列表" }).first().click()

    // --- caigou 审批付款 ---
    await switchAccount("procurement")
    await gotoPage(procurementPage, "/workspace")
    await approveTask(procurementPage, pmNo!)

    // 断言：付款已过账（API + UI 徽标）
    const postedPayment = await api<{ status: string }>(
        request,
        "GET",
        `/admin/supplier-payments/${paymentId}`,
        { token: financeToken },
    )
    expect(postedPayment.status.toLowerCase()).toBe("posted")

    await switchAccount("finance")
    await finPage.goto("/finance/supplier-accounts?view=payment")
    const postedRow = await expectTableRow(finPage, pmNo!, { timeout: 30_000 })
    await expect(postedRow.getByText("已过账", { exact: true })).toBeVisible({ timeout: 20_000 })

    // 断言：应付结清（核销完成）
    const settledAccounts = await api<
        { items: Array<{ status: string; open_total: string }> }
    >(request, "GET", "/admin/payable-accounts", {
        token: financeToken,
        query: { supplier_id: poSupplierId, page: 1, page_size: 10 },
    })
    expect(settledAccounts.items?.[0]?.status, "核销后应付应结清").toBe("settled")
    await finPage.goto("/finance/supplier-accounts")
    const settledRow = await expectTableRow(finPage, poNo!, { timeout: 30_000 })
    await expect(settledRow.getByText("已结清", { exact: true })).toBeVisible({ timeout: 20_000 })

    // =====================================================================
    // 阶段 4：财务登记进项发票并核销（Invoice=NO_APPROVAL，登记即过账）
    // =====================================================================
    const invoiceNo = `3330${String(Date.now()).slice(-8)}`
    await clickButton(finPage, "登记进项发票")
    dlg = await lastDialog(finPage)
    await expect(dlg.getByText("选择供应商 · 登记进项发票", { exact: true })).toBeVisible({
        timeout: 20_000,
    })
    await pickRemoteOption(finPage, dlg.getByPlaceholder("选择供应商"), poSupplierName, poSupplierName)
    await confirmDialog(finPage, "进入本次核销")

    await expect(finPage.getByText("进项票核销", { exact: true })).toBeVisible({
        timeout: 30_000,
    })
    // 勾选应付目标（本次分配按可收票余额自动带出）
    await finPage.getByLabel(`选择 ${poNo!}`).check()
    const invoicePoolText = await pool.innerText()
    const openInvoiceable = invoicePoolText.match(/¥\s*([\d,]+\.\d{2})/)?.[1]
    expect(openInvoiceable, "核销池应展示可收票余额").toBeTruthy()
    // 含税金额 = 可收票余额；不含税 + 税额按 13% 拆算（分位精确，满足 ±0.01 校验）
    const grossCents = Math.round(parseFloat(openInvoiceable!.replace(/,/g, "")) * 100)
    const taxCents = Math.round(grossCents * 0.13)
    await finPage.getByLabel("发票代码").fill("044032500111")
    await finPage.getByLabel("发票号码").fill(invoiceNo)
    await pickDate(finPage, "开票日期", todayStr())
    await finPage.getByLabel("含税金额").fill((grossCents / 100).toFixed(2))
    await finPage.getByLabel("不含税").fill(((grossCents - taxCents) / 100).toFixed(2))
    await finPage.getByLabel("税额").fill((taxCents / 100).toFixed(2))

    await finPage.getByRole("button", { name: "确认登记并核销" }).click()
    dlg = await lastDialog(finPage)
    await expect(dlg.getByText("确认登记进项发票并核销", { exact: true })).toBeVisible({
        timeout: 20_000,
    })
    await confirmDialog(finPage, "确认提交")
    await expect(finPage.getByText("进项发票已登记")).toBeVisible({ timeout: 20_000 })
    await finPage.getByRole("button", { name: "回到列表" }).first().click()

    // 断言：进项发票已登记（API + UI 徽标）
    const invoices = await api<
        { items: Array<{ id: string; invoice_no: string; status: string }> }
    >(request, "GET", "/admin/invoices", {
        token: financeToken,
        query: { invoice_direction: "purchase", invoice_no: invoiceNo, page: 1, page_size: 10 },
    })
    const invoice = invoices.items?.[0]
    expect(invoice?.status, "进项发票登记后应已注册（NO_APPROVAL 直接过账）").toBe("registered")

    await finPage.goto("/finance/supplier-accounts?view=purchase_invoice")
    const invoiceRow = await expectTableRow(finPage, invoiceNo, { timeout: 30_000 })
    await expect(invoiceRow.getByText("已登记", { exact: true })).toBeVisible({ timeout: 20_000 })

    // =====================================================================
    // 阶段 5：财务发起付款冲正 → 提交 → 采购复核 → 过账 → 原付款冲正
    // =====================================================================
    await finPage.goto("/finance/supplier-accounts?view=payment")
    const paymentRow = await expectTableRow(finPage, pmNo!, { timeout: 30_000 })
    await paymentRow.getByRole("button", { name: "冲正", exact: true }).click()

    // 发起付款冲正：登记冲正草稿
    dlg = await lastDialog(finPage)
    await expect(dlg.getByText("发起付款冲正", { exact: true })).toBeVisible({ timeout: 20_000 })
    await dlg.getByLabel("原因说明").fill(`E2E 付款冲正-${uniq}`)
    await dlg.getByRole("button", { name: "下一步" }).click()

    // 草稿创建后打开提交确认（PaymentReversal=采购复核）
    dlg = await lastDialog(finPage)
    await expect(dlg.getByText("确认提交冲正", { exact: true })).toBeVisible({ timeout: 20_000 })
    await confirmDialog(finPage, "确认提交")
    await expect(finPage.getByText("冲正已提交审批")).toBeVisible({ timeout: 20_000 })

    // 冲正详情预览已打开，从 URL 取冲正单 ID（previewKind=reversal&detailId=...）
    await finPage.waitForURL(/previewKind=reversal/, { timeout: 20_000 })
    const reversalId = new URL(finPage.url()).searchParams.get("detailId")
    expect(reversalId, "应能拿到付款冲正单 ID").toBeTruthy()
    const pendingReversal = await api<{ reversal_no: string }>(
        request,
        "GET",
        `/admin/payment-reversals/${reversalId}`,
        { token: financeToken },
    )
    expect(pendingReversal.reversal_no, "提交后应分配付款冲正单号").toBeTruthy()
    const reversalSheet = finPage.locator('[data-slot="sheet-content"]').last()
    await reversalSheet
        .getByRole("button", { name: "关闭" })
        .click({ timeout: 5_000 })
        .catch(() => {})

    // --- caigou 审批冲正 ---
    await switchAccount("procurement")
    await gotoPage(procurementPage, "/workspace")
    await approveTask(procurementPage, pendingReversal.reversal_no)

    // 断言：冲正已过账、原付款已冲正（API + UI 徽标）
    // 注：冲正详情读取用财务 token（冲正单属财务域，采购账号读详情 403）
    const postedReversal = await api<{ status: string }>(
        request,
        "GET",
        `/admin/payment-reversals/${reversalId}`,
        { token: financeToken },
    )
    expect(postedReversal.status.toLowerCase()).toBe("posted")
    const reversedPayment = await api<{ status: string }>(
        request,
        "GET",
        `/admin/supplier-payments/${paymentId}`,
        { token: financeToken },
    )
    expect(reversedPayment.status.toLowerCase()).toBe("reversed")

    await switchAccount("finance")
    await finPage.goto("/finance/supplier-accounts?view=payment")
    const reversedRow = await expectTableRow(finPage, pmNo!, { timeout: 30_000 })
    await expect(reversedRow.getByText("已冲正", { exact: true })).toBeVisible({ timeout: 20_000 })

    // 冲正冲减核销：应付恢复未结（原记录保留、付款结清被撤销）
    const reopenedAccounts = await api<
        { items: Array<{ status: string }> }
    >(request, "GET", "/admin/payable-accounts", {
        token: financeToken,
        query: { supplier_id: poSupplierId, page: 1, page_size: 10 },
    })
    expect(reopenedAccounts.items?.[0]?.status, "冲正过账后应付应恢复未结").toBe("open")
    await finPage.goto("/finance/supplier-accounts")
    const reopenedRow = await expectTableRow(finPage, poNo!, { timeout: 30_000 })
    await expect(reopenedRow.getByText("未结", { exact: true })).toBeVisible({ timeout: 20_000 })

    // 冲正后的付款不再进入待核销队列（视图仅含 POSTED 且存在未分配余额的付款/蓝票；
    // 已冲正付款与已全额分配的进项票均不出现，以代码为准）
    await finPage.goto("/finance/supplier-accounts?view=unallocated")
    const unallocatedTable = finPage.locator("table").first()
    await expect(
        unallocatedTable.getByRole("row").filter({ hasText: pmNo! }),
    ).toHaveCount(0)
    await expect(
        unallocatedTable.getByRole("row").filter({ hasText: invoiceNo }),
    ).toHaveCount(0)
})
