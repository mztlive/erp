/**
 * [flow-06] 客户票款：回款、销项发票、核销与冲正
 *
 * 文档章节:
 *   - docs/erp-phase-1.md §9.1「客户侧」: 销售单生效后形成应收项目；每次实际到账先形成
 *     客户回款草稿，提交后按 CustomerReceipt 已发布定义审批，末节点通过并过账后才计入
 *     已收；回款与销项发票分别记录和核销；同一主体内多对多核销。
 *   - docs/erp-phase-1.md §6.5.4「资金与发票纠错」: 客户侧纠错单据含回款冲正单；
 *     退款与收付款冲正为 PROCESS_REQUIRED，红票 Invoice 为 NO_APPROVAL 不进入审批实例；
 *     过账后原子冲减原回款，原收付款与发票记录保留。
 *
 * 使用账号（密码均 123456）:
 *   - xiaoshou（销售）: 新建客户、上传合同 PDF、创建并提交销售单
 *   - caigou（采购）  : W02 工作台审批销售单（SalesOrder=采购确认）
 *   - caiwu（财务）   : 登记客户回款、登记销项发票并核销、发起回款冲正
 *   - lisiyong（销售领导）: 审批客户回款（CustomerReceipt）与回款冲正（ReceiptReversal）
 *
 * 已发布审批定义（见 e2e/scripts/publish-approval-definitions.mjs）:
 *   SalesOrder=采购确认(caigou)；CustomerReceipt=销售领导复核(lisiyong)；
 *   ReceiptReversal=销售领导复核(lisiyong)；Invoice=NO_APPROVAL。
 * 提交人不得审批自己的单据。
 *
 * 文档-代码差异（以代码为准）:
 * 1. doc: 待办审批在 W02 统一待办（/workspace/tasks）完成；
 *    code: app/(workspace)/workspace/tasks/page.tsx 将 /workspace/tasks permanentRedirect
 *    到唯一工作台 /workspace，审批决定在 /workspace 右侧详情区完成（workspace-home-page.tsx）。
 * 2. doc §6.5.4: 冲正/退款/红票由「财务经办创建纠错单据」；
 *    code: 入口在客户往来页回款/发票详情抽屉 footer 的「冲正/退款/红票」按钮
 *    （customer-account-detail-preview.tsx，按 allowed_actions 白名单渲染），
 *    创建草稿后经提交确认弹窗进入审批——流程语义一致，入口呈现不同。
 * 3. doc §9.1: 回款与销项发票「分别记录和核销」；
 *    code: 同一「核销工作区」按 mode=receipt/invoice 区分回款核销/发票核销
 *    （allocation-session-panel.tsx），记录与分配明细独立——一致。
 * 4. doc §9.3: 主状态仅 4 值（草稿/审批中/已生效/已作废）；
 *    code: 销售单列表状态徽标使用后端 stage label（sales-orders-list-columns.tsx
 *    primaryStatus = stage.label，如「待采购确认」），本 spec 对销售单生效改以
 *    API commercial_status=EFFECTIVE + 客户往来应收台账出现来断言。
 */

import { test, expect, type Locator, type Page } from "@playwright/test"
import path from "path"

import { api, apiLogin } from "../helpers/api"
import { newLoggedInContext } from "../helpers/login"
import { clickButton, expectTableRow, gotoPage, pickOption } from "../helpers/ui"

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

/** 取最后可见弹窗（Dialog 或 AlertDialog；弹窗与抽屉同开时优先 AlertDialog）。
 *  返回实时 locator：弹窗延迟出现时，后续断言会持续重试直至命中。 */
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

/** 日期时间字段（DateTimeLocalPicker：日历 + 时间输入 + 完成）。 */
async function pickDateTime(
    page: Page,
    label: string,
    ymd: string,
    time: string,
): Promise<void> {
    const box = fieldBox(page, label)
    await box.getByRole("button").first().click()
    const dlg = await lastDialog(page)
    await expect(dlg).toBeVisible({ timeout: 10_000 })
    const cal = dlg.locator('[data-slot="calendar"]')
    await expect(cal).toBeVisible({ timeout: 10_000 })
    await pickDayInCalendar(cal, ymd)
    // 时间输入与「完成」在 Calendar 包装之外、对话框之内
    await dlg.getByLabel("时间，精确到秒").fill(time)
    await dlg.getByRole("button", { name: "完成" }).click()
}

/**
 * W02 工作台按业务对象 ID（单据 UUID）找到审批任务并提交「通过」。
 * 任务行按钮 id 来自 workspace-task-list.tsx: id=`work-item-${stableNumber}`，
 * stableNumber=businessObjectId（approval 任务即单据 UUID，见 dto.rs / store.rs）。
 */
async function approveTask(page: Page, businessObjectId: string): Promise<void> {
    const task = page
        .locator(`button[id="work-item-${businessObjectId}"]`)
        .first()
    await expect(task).toBeVisible({ timeout: 30_000 })
    await task.click()
    const approve = page.getByRole("button", { name: "通过", exact: true })
    await expect(approve).toBeVisible({ timeout: 20_000 })
    await approve.click()
    const dlg = await lastDialog(page)
    await expect(dlg).toBeVisible({ timeout: 20_000 })
    await dlg.getByRole("button", { name: "提交决定" }).click()
    await expect(dlg).not.toBeVisible({ timeout: 20_000 })
    // 决定提交后任务从待办列表消失（工作台自动刷新）
    await expect(task).not.toBeVisible({ timeout: 30_000 })
}

type PartyDto = { id: string; party_no: string }
type SkuDto = { sku_id: string; sku_no: string; name: string; product_kind: string }

// ---------------------------------------------------------------------------
// 主流程
// ---------------------------------------------------------------------------

test("客户票款：回款→审批→过账→核销应收；销项发票登记与核销；回款冲正→审批→过账", async ({
    browser,
    request,
}) => {
    test.setTimeout(300_000)

    const financeToken = await apiLogin(request, "finance")
    const salesToken = await apiLogin(request, "sales")
    const leaderToken = await apiLogin(request, "salesLeader")
    // ---------- 主数据发现（reset 保留：结算主体 / 公司商品池 SKU） ----------
    const parties = await api<{ items: PartyDto[] }>(request, "GET", "/admin/parties", {
        token: salesToken,
        query: { status: "active", page: 1, page_size: 20, sort_by: "party_no", sort_dir: "asc" },
    })
    const party = parties.items?.[0]
    expect(party, "保留主数据应至少有一个启用结算主体（/admin/parties）").toBeTruthy()
    const partyNo = party!.party_no

    const skus = await api<{ items: SkuDto[] }>(request, "GET", "/admin/sellable-skus", {
        token: salesToken,
        query: { page: 1, page_size: 20 },
    })
    const sku = (skus.items ?? []).find(
        (s) => (s.product_kind ?? "").toUpperCase() !== "VOUCHER",
    )
    expect(sku, "保留主数据应至少有一个可售实物/服务 SKU（/admin/sellable-skus）").toBeTruthy()

    // =====================================================================
    // 阶段 1：销售侧准备——客户 → 合同(PDF) → 销售单 → 提交 → 采购审批 → 生效
    // =====================================================================
    const uniq = stamp()
    const customerName = `E2E客户${uniq}`
    const legalName = `E2E测试客户${uniq}有限公司`
    const creditCode = `91330100${String(Date.now()).slice(0, 10)}`
    const contractNo = `HT-E2E-${uniq}`

    const sales = await newLoggedInContext(browser, "sales")
    const salesPage = sales.page
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
    await salesPage.waitForURL(/\/sales\/customers\/[^/]+$/, { timeout: 20_000 })
    const customerId = salesPage.url().split("/").pop()!
    expect(customerId).toBeTruthy()

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
    await pickRemoteOption(
        salesPage,
        salesPage.getByLabel("商品"),
        sku!.name,
        sku!.name,
    )
    await salesPage.getByLabel("含税单价").fill("1000.00")
    await pickDate(salesPage, "交付日期", addDaysStr(30))

    // --- 提交销售单（SalesOrder=采购确认） ---
    await salesPage.getByRole("button", { name: "提交", exact: true }).click()
    await confirmDialog(salesPage, "确认提交")
    await salesPage.waitForURL(/\/sales\/orders\/[^/]+$/, { timeout: 20_000 })
    const salesOrderId = salesPage.url().split("/").pop()!
    expect(salesOrderId).toBeTruthy()
    await sales.context.close()

    // --- caigou 在 W02 工作台审批销售单 ---
    const procurement = await newLoggedInContext(browser, "procurement")
    await gotoPage(procurement.page, "/workspace")
    await approveTask(procurement.page, salesOrderId)
    await procurement.context.close()

    // 断言：销售单已生效（后端主状态），应收随后在客户往来出现
    const orderDetail = await api<{ commercial_status?: string }>(
        request,
        "GET",
        `/admin/sales-orders/${salesOrderId}`,
        { token: salesToken },
    )
    expect(String(orderDetail.commercial_status ?? "").toUpperCase()).toBe("EFFECTIVE")
    const orderList = await api<{ items: Array<{ order_no: string }> }>(
        request,
        "GET",
        "/admin/sales-orders",
        { token: salesToken, query: { customer_id: customerId, page: 1, page_size: 10 } },
    )
    const orderNo = orderList.items?.[0]?.order_no
    expect(orderNo, "应能查到该客户名下的销售单号").toBeTruthy()

    // =====================================================================
    // 阶段 2：财务登记客户回款 → 提交 → 销售领导审批 → 过账 → 应收结清
    // =====================================================================
    const finance = await newLoggedInContext(browser, "finance")
    const finPage = finance.page
    await gotoPage(finPage, "/finance/customer-accounts")
    // 销售单生效后应收台账出现该单（行文本为销售单号 XS...）
    const receivableRow = await expectTableRow(finPage, orderNo, { timeout: 30_000 })
    await expect(receivableRow.getByText("未结")).toBeVisible({ timeout: 20_000 })

    // --- 登记回款：选择往来主体 → 核销工作区 ---
    await clickButton(finPage, "登记回款")
    dlg = await lastDialog(finPage)
    await expect(dlg.getByText("登记回款 — 选择往来主体", { exact: true })).toBeVisible({
        timeout: 20_000,
    })
    await pickRemoteOption(finPage, dlg.getByPlaceholder("请选择往来主体"), partyNo, partyNo)
    await confirmDialog(finPage, "打开核销工作区")

    // --- 填写回款事实并分配 ---
    const postButton = finPage.getByRole("button", { name: "确认登记并核销" })
    await expect(postButton).toBeVisible({ timeout: 30_000 })
    await pickDateTime(finPage, "实际到账时间", todayStr(), "10:30:00")
    await finPage.getByLabel("到账金额（含税）").fill("1000.00")
    await finPage.getByLabel("银行流水/回单引用").fill(`E2E-REF-${uniq}`)
    await finPage.getByRole("button", { name: "加入" }).first().click()
    await finPage.getByRole("button", { name: "填满" }).first().click()

    // --- 提交回款（CustomerReceipt=销售领导复核） ---
    await postButton.click()
    await confirmDialog(finPage, "确认提交")
    await expect(finPage.getByText("回款已提交审批")).toBeVisible({ timeout: 20_000 })
    // 审批中状态：结果面板出现「撤回审批」入口（页面无「审批中」文本徽标）
    await expect(
        finPage.getByRole("button", { name: "撤回审批" }).first(),
    ).toBeVisible({ timeout: 20_000 })

    // 取回款单号与 ID（审批任务按单据 UUID 定位）
    const receipts = await api<
        { items: Array<{ id: string; receipt_no: string; status: string }> }
    >(request, "GET", "/admin/customer-receipts", {
        token: financeToken,
        query: { counterparty_party_id: party!.id, page: 1, page_size: 10 },
    })
    const receipt = (receipts.items ?? []).find((r) =>
        /in_approval|pending_review/i.test(r.status),
    )
    expect(receipt, "提交后应存在审批中的客户回款单").toBeTruthy()
    const receiptId = receipt!.id
    const receiptNo = receipt!.receipt_no

    await finPage.getByRole("button", { name: "返回列表" }).first().click()

    // --- lisiyong 审批回款 ---
    const leader = await newLoggedInContext(browser, "salesLeader")
    await gotoPage(leader.page, "/workspace")
    await approveTask(leader.page, receiptId)
    await leader.context.close()

    // 断言：回款已过账（API + UI 徽标）
    const postedReceipt = await api<{ status: string }>(
        request,
        "GET",
        `/admin/customer-receipts/${receiptId}`,
        { token: financeToken },
    )
    expect(postedReceipt.status.toLowerCase()).toBe("posted")

    await finPage.goto("/finance/customer-accounts?view=receipt")
    const postedRow = await expectTableRow(finPage, receiptNo, { timeout: 30_000 })
    await expect(postedRow.getByText("已过账")).toBeVisible({ timeout: 20_000 })

    // 断言：应收结清（核销完成）
    const settledAccounts = await api<
        { items: Array<{ status: string; open_total: string }> }
    >(request, "GET", "/admin/receivable-accounts", {
        token: financeToken,
        query: { sales_order_id: salesOrderId, page: 1, page_size: 10 },
    })
    const settledAccount = settledAccounts.items?.[0]
    expect(settledAccount?.status, "核销后应收应结清").toBe("settled")
    await finPage.goto("/finance/customer-accounts")
    const settledRow = await expectTableRow(finPage, orderNo, { timeout: 30_000 })
    await expect(settledRow.getByText("已结清")).toBeVisible({ timeout: 20_000 })

    // =====================================================================
    // 阶段 3：财务登记销项发票并核销（Invoice=NO_APPROVAL，登记即过账）
    // =====================================================================
    const invoiceNo = `3330${String(Date.now()).slice(-8)}`
    await clickButton(finPage, "登记销项发票")
    dlg = await lastDialog(finPage)
    await expect(dlg.getByText("登记销项发票 — 选择往来主体", { exact: true })).toBeVisible({
        timeout: 20_000,
    })
    await pickRemoteOption(finPage, dlg.getByPlaceholder("请选择往来主体"), partyNo, partyNo)
    await confirmDialog(finPage, "打开核销工作区")

    await expect(finPage.getByRole("button", { name: "确认登记并核销" })).toBeVisible({
        timeout: 30_000,
    })
    await finPage.getByLabel("发票代码").fill("044032500111")
    await finPage.getByLabel("发票号码").fill(invoiceNo)
    await pickDate(finPage, "开票日期", todayStr())
    await finPage.getByLabel("含税金额").fill("1000.00")
    await finPage.getByRole("button", { name: "加入" }).first().click()
    await finPage.getByRole("button", { name: "填满" }).first().click()

    await finPage.getByRole("button", { name: "确认登记并核销" }).click()
    dlg = await lastDialog(finPage)
    await expect(dlg.getByText("确认登记销项发票并分配", { exact: true })).toBeVisible({
        timeout: 20_000,
    })
    await confirmDialog(finPage, "确认提交")
    await expect(finPage.getByText("销项发票已登记并分配")).toBeVisible({ timeout: 20_000 })
    await finPage.getByRole("button", { name: "返回列表" }).first().click()

    // 断言：发票已登记（API + UI 徽标）
    const invoices = await api<
        { items: Array<{ id: string; invoice_no: string; status: string }> }
    >(request, "GET", "/admin/invoices", {
        token: financeToken,
        query: { invoice_direction: "sales", invoice_no: invoiceNo, page: 1, page_size: 10 },
    })
    const invoice = invoices.items?.[0]
    expect(invoice?.status, "销项发票登记后应已注册（NO_APPROVAL 直接过账）").toBe("registered")

    await finPage.goto("/finance/customer-accounts?view=sales_invoice")
    const invoiceRow = await expectTableRow(finPage, invoiceNo, { timeout: 30_000 })
    await expect(invoiceRow.getByText("已登记")).toBeVisible({ timeout: 20_000 })

    // =====================================================================
    // 阶段 4：财务发起回款冲正 → 提交 → 销售领导审批 → 过账 → 原回款冲正
    // =====================================================================
    await finPage.goto("/finance/customer-accounts?view=receipt")
    const receiptRow = await expectTableRow(finPage, receiptNo, { timeout: 30_000 })
    await receiptRow.getByRole("button", { name: "预览" }).click()

    const sheet = finPage.locator('[data-slot="sheet-content"]').last()
    await expect(sheet).toBeVisible({ timeout: 20_000 })
    await expect(
        sheet.getByText(receiptNo, { exact: true }).first(),
    ).toBeVisible({ timeout: 20_000 })
    await sheet.getByRole("button", { name: "冲正", exact: true }).click()

    // 发起回款冲正：登记冲正草稿
    dlg = await lastDialog(finPage)
    await expect(dlg.getByText("发起回款冲正", { exact: true })).toBeVisible({ timeout: 20_000 })
    await dlg.getByLabel("原因说明").fill(`E2E 冲正测试-${uniq}`)
    await dlg.getByRole("button", { name: "下一步" }).click()

    // 草稿创建后打开提交确认（ReceiptReversal=销售领导复核）
    dlg = await lastDialog(finPage)
    await expect(dlg.getByText("确认提交冲正", { exact: true })).toBeVisible({ timeout: 20_000 })
    await confirmDialog(finPage, "确认提交")
    await expect(finPage.getByText("冲正已提交审批")).toBeVisible({ timeout: 20_000 })

    // 预览已切到冲正单，从 URL 取冲正单 ID
    await finPage.waitForURL(/previewKind=reversal/, { timeout: 20_000 })
    const reversalUrl = new URL(finPage.url())
    const reversalId = reversalUrl.searchParams.get("previewId")
    expect(reversalId, "应能拿到回款冲正单 ID").toBeTruthy()
    await sheet.getByRole("button", { name: "关闭" }).click().catch(() => {})

    // --- lisiyong 审批冲正 ---
    const leader2 = await newLoggedInContext(browser, "salesLeader")
    await gotoPage(leader2.page, "/workspace")
    await approveTask(leader2.page, reversalId!)
    await leader2.context.close()

    // 断言：冲正已过账、原回款已冲正（API + UI 徽标）
    const postedReversal = await api<{ status: string }>(
        request,
        "GET",
        `/admin/receipt-reversals/${reversalId}`,
        { token: financeToken },
    )
    expect(postedReversal.status.toLowerCase()).toBe("posted")
    const reversedReceipt = await api<{ status: string }>(
        request,
        "GET",
        `/admin/customer-receipts/${receiptId}`,
        { token: financeToken },
    )
    expect(reversedReceipt.status.toLowerCase()).toBe("reversed")

    await finPage.goto("/finance/customer-accounts?view=receipt")
    const reversedRow = await expectTableRow(finPage, receiptNo, { timeout: 30_000 })
    await expect(reversedRow.getByText("已冲正")).toBeVisible({ timeout: 20_000 })

    // 冲正冲减核销：应收恢复未结（原记录保留、回款结清被撤销）
    const reopenedAccounts = await api<
        { items: Array<{ status: string }> }
    >(request, "GET", "/admin/receivable-accounts", {
        token: financeToken,
        query: { sales_order_id: salesOrderId, page: 1, page_size: 10 },
    })
    expect(reopenedAccounts.items?.[0]?.status, "冲正过账后应收应恢复未结").toBe("open")
    await finPage.goto("/finance/customer-accounts")
    const reopenedRow = await expectTableRow(finPage, orderNo, { timeout: 30_000 })
    await expect(reopenedRow.getByText("未结")).toBeVisible({ timeout: 20_000 })

    await finance.context.close()
})
