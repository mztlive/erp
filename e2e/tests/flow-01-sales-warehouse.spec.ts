/**
 * flow-01 实物销售单·仓发完整基准流程
 *
 * 文档依据: docs/erp-phase-1.md §7.3.1（商品入仓后由公司仓库发货，完整基准流程）+ §9.1（客户票款）
 *
 * 流程:
 *   W03 客户创建(xiaoshou) → W04 合同上传 PDF(fixtures/sample-contract.pdf) →
 *   W05 销售单创建并提交(选可售 SKU) → 采购确认审批通过(caigou) → 销售单生效 →
 *   W08 采购创建采购单(基于二次确认创建依据)并提交(caigou) → 财务审核通过(caiwu) →
 *   W09 采购入库(入仓) → 仓发出库(admin 代仓储) →
 *   W06 销售登记客户验收(xiaoshou) →
 *   W11 财务登记客户回款并核销(caiwu) → 销售领导复核(lisiyong) → 过账结清 →
 *   财务登记销项发票并核销(caiwu) →
 *   断言销售单履约/回款/开票进度与关闭条件（履约完成 + 应收结清 → 已关闭；开票不挡结案）
 *
 * 使用账号: xiaoshou(销售) / caigou(采购) / caiwu(财务) / lisiyong(销售领导) / admin(代仓储作业，见 risks)
 * 运行方式: 单个浏览器页面按业务节点串行切换账号，禁止同时打开多个账号窗口。
 *
 * 文档-代码差异（以代码为准）:
 * 1. doc=7.3.1: 采购先"提交到货信息"，再由仓储"创建采购入库单并入库"→"创建仓发出库单并出库"。
 *    code=W09 履约工作面(/fulfillment?lane=warehouse)在采购单生效后由系统直接生成 DRAFT
 *    采购入库单与仓发发货单工作项（含留货），操作员在工作面直接"确认入库/确认发货"，
 *    没有独立的"提交到货信息"步骤。
 * 2. doc=7.4: "选源在本环节完成：采购单创建时选择供给、成本与交期"。
 *    code=供给/成本/交期在销售单提交时的采购二次确认（创建依据）阶段已固定，
 *    采购单编辑面供应商/履约责任只读，仅可微调数量/含税单价/税率与付款条件；交期无编辑入口。
 * 3. doc=W02: 统一待办在 /workspace/tasks。code=/workspace/tasks 永久重定向到 /workspace（唯一工作台）。
 * 4. doc=9.1: 回款草稿提交后按 CustomerReceipt 审批，末节点通过并过账后计入已收。
 *    code=一致（提交即启动审批，销售领导复核通过后过账核销）；"待复核"已收敛为"审批中"。
 */
import {
    expect,
    test,
    type APIRequestContext,
    type Locator,
    type Page,
} from "@playwright/test"
import path from "node:path"

import { createSinglePageAccountSwitcher } from "../helpers/login"
import { api, apiLogin } from "../helpers/api"
import {
    clickButton,
    clickDialogButton,
    expectTableRow,
    fillByLabel,
    gotoPage,
} from "../helpers/ui"
import {
    approveVisibleWorkspaceTask,
    openWorkspaceTask,
    workspaceTaskButtons,
} from "../helpers/workspace"

// ── 流程专用小工具（保持 helpers 通用，业务细节留在本文件）──────────────────

const CONTRACT_PDF = path.join(__dirname, "..", "fixtures", "sample-contract.pdf")
const stamp = () => Date.now().toString()
const CUSTOMER_LEGAL = `E2E客户${stamp().slice(-8)}`
const CUSTOMER_SHORT = `客${stamp().slice(-6)}`
const CREDIT_CODE = `91${stamp().padEnd(16, "0").slice(0, 16)}`
const CONTRACT_NO = `HT-${stamp().slice(-10)}`
const INVOICE_NO = `FP-${stamp().slice(-10)}`
const UNIT_PRICE = "100.00"
const RECEIPT_AMOUNT = "100.00"
/** 日历默认展示当月；选择"今天"的日号，避免跨月翻页。 */
const TODAY_DAY = String(new Date().getDate())
/** 页面上的弹窗（含 AlertDialog，role 为 alertdialog）。 */
function dialogOn(page: Page): Locator {
    return page.locator('[role="dialog"], [role="alertdialog"]').last()
}

/** 打开远程搜索组合框，按文本选择选项；弹层偶发未渲染时重试一次。 */
async function pickComboboxOption(
    page: Page,
    trigger: Locator,
    optionText: string,
): Promise<void> {
    for (let attempt = 0; attempt < 2; attempt++) {
        await trigger.click()
        const item = page
            .locator('[data-slot="combobox-item"]')
            .filter({ hasText: optionText })
            .first()
        try {
            await item.waitFor({ state: "visible", timeout: 8_000 })
            await item.click()
            await expect(item).not.toBeVisible({ timeout: 20_000 }).catch(() => {})
            return
        } catch {
            // 页面导航后首开偶发失败：关闭残留弹层后重试
            await page.keyboard.press("Escape").catch(() => {})
        }
    }
    throw new Error(`combobox 选项 ${optionText} 未出现`)
}

/** 打开远程搜索组合框并选择第一个选项（主数据内容未知或唯一候选时用）。 */
async function pickFirstComboboxOption(page: Page, placeholder: string): Promise<void> {
    const input = page.getByPlaceholder(placeholder).first()
    await input.click()
    const item = page.locator('[data-slot="combobox-item"]').first()
    await expect(item).toBeVisible({ timeout: 20_000 })
    await item.click()
}

/** 搜索型组合框：输入关键字后选择包含该文本的选项；搜索无果时回退到第一个选项。 */
async function pickSearchedOption(
    page: Page,
    placeholder: string,
    searchText: string,
): Promise<void> {
    const input = page.getByPlaceholder(placeholder).first()
    await input.click()
    await input.fill(searchText)
    let item = page
        .locator('[data-slot="combobox-item"]')
        .filter({ hasText: searchText })
        .first()
    try {
        await item.waitFor({ state: "visible", timeout: 10_000 })
    } catch {
        // reset 后业务数据为空，命中的实体通常是唯一选项；取第一项兜底
        item = page.locator('[data-slot="combobox-item"]').first()
        await expect(item).toBeVisible({ timeout: 10_000 })
    }
    await item.click()
}

/** 在已打开的日期弹窗里点选某日（react-day-picker 当月网格）。 */
async function pickDayInCalendar(page: Page, day: string): Promise<void> {
    const calendar = page.locator('[data-slot="calendar"]').last()
    await expect(calendar).toBeVisible({ timeout: 10_000 })
    const m = String(new Date().getMonth() + 1)
    const d = String(new Date().getDate())
    const y = String(new Date().getFullYear())
    await calendar
        .locator(`[data-day="${m}/${d}/${y}"],[data-day="${y}/${m}/${d}"],[data-day="${day}"]`)
        .first()
        .click()
    await expect(calendar).not.toBeVisible({ timeout: 10_000 }).catch(() => {})
}

/** 表单字段容器：Field 根节点（data-slot=field）内包含指定 label 文本。 */
function fieldBox(page: Page, label: string): Locator {
    return page
        .locator('[data-slot="field"]')
        .filter({ has: page.getByText(label, { exact: true }) })
        .first()
}

/** 纯日期选择：点击字段内触发按钮并选日；已选中的日期会反选清空，必须跳过点击。 */
async function pickDate(page: Page, label: string, day: string): Promise<void> {
    const box = fieldBox(page, label)
    await box.getByRole("button").first().click()
    const calendar = page.locator('[data-slot="calendar"]').last()
    await expect(calendar).toBeVisible({ timeout: 10_000 })
    const m = String(new Date().getMonth() + 1)
    const d = String(new Date().getDate())
    const y = String(new Date().getFullYear())
    const dayCell = calendar
        .locator(`[data-day="${m}/${d}/${y}"],[data-day="${y}/${m}/${d}"],[data-day="${day}"]`)
        .first()
    // 表单已预填今天时，日历把今天标记为已选中；再次点击会反选清空（react-day-picker
    // 单选反选），所以已选中时跳过点击，仅关闭弹层。
    if ((await dayCell.getAttribute("data-selected-single")) !== "true") {
        await dayCell.click()
    }
    // 关闭日历弹层：残留的可见 dialog 会抢占 fillByLabel 的作用域
    await page.keyboard.press("Escape").catch(() => {})
    await expect(calendar).not.toBeVisible({ timeout: 5_000 }).catch(() => {})
}

/** 日期时间选择：选日 + 填秒级时间 + 完成（时间输入/完成按钮在 Calendar 之外）。 */
async function pickDateTime(page: Page, label: string, day: string, time: string): Promise<void> {
    const box = fieldBox(page, label)
    await box.getByRole("button").first().click()
    const popover = page.locator('[data-slot="calendar"]').last()
    await expect(popover).toBeVisible({ timeout: 10_000 })
    const m = String(new Date().getMonth() + 1)
    const d = String(new Date().getDate())
    const y = String(new Date().getFullYear())
    await popover
        .locator(`[data-day="${m}/${d}/${y}"],[data-day="${y}/${m}/${d}"],[data-day="${day}"]`)
        .first()
        .click()
    await page.getByLabel("时间，精确到秒").fill(time)
    await page.getByRole("button", { name: "完成", exact: true }).click()
    await expect(popover).not.toBeVisible({ timeout: 10_000 }).catch(() => {})
}

/**
 * W01 工作台审批：/workspace。
 * 左列任务行展示类型标签 + 稳定单号 + 摘要；各审批人此刻恰好一条待办，
 * guardText 命中失败时回退到第一行。
 */
async function approveInWorkspace(page: Page, guardText?: string): Promise<void> {
    await gotoPage(page, "/workspace")
    const rows = workspaceTaskButtons(page)
    await expect(rows.first()).toBeVisible({ timeout: 30_000 })
    let target = rows.first()
    if (guardText) {
        const filtered = rows.filter({ hasText: guardText }).first()
        if ((await filtered.count()) > 0) target = filtered
    }
    await openWorkspaceTask(page, target)
    await approveVisibleWorkspaceTask(page)
    await expect(target).toHaveCount(0, { timeout: 20_000 })
}

/** W09 履约工作面（lane=warehouse，autoNext=0 保证按钮文案为"确认入库/确认发货"）。 */
async function postFulfillmentOperation(
    page: Page,
    cardTitlePrefix: string,
    confirmLabel: string,
    doneTitle: string,
): Promise<void> {
    const card = page.getByText(new RegExp(`^${cardTitlePrefix} · `)).first()
    await expect(card).toBeVisible({ timeout: 30_000 })
    // 「连续处理操作」栏与卡片底部各有一个同名主按钮，两者同义（均触发 onConfirm）
    await page.getByRole("button", { name: confirmLabel, exact: true }).first().click()
    const dialog = dialogOn(page)
    await expect(dialog).toBeVisible({ timeout: 10_000 })
    await dialog.getByRole("button", { name: confirmLabel, exact: true }).click()
    // 确认对话框内含「状态变化」预览（toStatus 即为 doneTitle 文案），
    // 必须等对话框关闭（POST 完成且成功后才会关闭），再断言结果面板文本
    await expect(dialog).not.toBeVisible({ timeout: 30_000 })
    await expect(page.getByText(doneTitle).first()).toBeVisible({ timeout: 20_000 })
}

/** 确保存在至少一个启用仓库（主数据；入库草稿生成依赖它，UI 禁止新建，走 API）。 */
async function ensureWarehouse(request: APIRequestContext): Promise<void> {
    const token = await apiLogin(request, "admin")
    const page = await api<{ items: Array<{ id: string }> }>(
        request,
        "GET",
        "/admin/warehouses",
        { token, query: { page: 1, page_size: 10 } },
    )
    if ((page.items ?? []).length > 0) return
    await api(
        request,
        "POST",
        "/admin/warehouses",
        {
            token,
            body: {
                warehouse_code: `E2E-WH-${Date.now().toString().slice(-8)}`,
                name: "E2E 测试仓",
                address: "上海市测试路 1 号",
                contact: "仓储测试",
                effective_from: "2026-01-01",
                change_reason: "E2E 流程主数据准备",
            },
        },
    )
}

/** 客户往来登记（回款/销项发票共用）：往来主体选择器过滤出「已有应收子账的主体」，取第一个。 */
async function openAllocationSession(page: Page, registerButton: string): Promise<void> {
    await clickButton(page, registerButton)
    const picker = dialogOn(page)
    const sessionTitle = page.getByText(/^核销 · /).first()
    try {
        await picker.waitFor({ state: "visible", timeout: 3_000 })
        await pickFirstComboboxOption(page, "请选择往来主体")
        await clickDialogButton(page, "打开核销工作区")
    } catch {
        // 往来主体已能唯一确定时跳过选择弹窗，直接进入核销工作区。
    }
    await expect(sessionTitle).toBeVisible({ timeout: 20_000 })
}

test("flow-01 实物销售单仓发完整基准流程", async ({ page, request }) => {
    test.setTimeout(260_000)

    // ── 主数据准备：确保存在启用仓库（入库草稿生成依赖；UI 禁止新建，走 API）──
    await ensureWarehouse(request)

    // ── 单页面串行切号：角色别名均指向同一个可见页面 ─────────────────────────
    const switchAccount = createSinglePageAccountSwitcher(page)
    const salesPage = page
    const poPage = page
    const financePage = page
    const leaderPage = page
    const whPage = page
    await switchAccount("sales") // xiaoshou

    // ── W03 客户创建 ────────────────────────────────────────────────────────
    await gotoPage(salesPage, "/sales/customers")
    await clickButton(salesPage, "新建客户")
    const customerDialog = dialogOn(salesPage)
    await expect(customerDialog.getByText("新建客户")).toBeVisible({ timeout: 10_000 })
    await fillByLabel(salesPage, "法定名称", CUSTOMER_LEGAL)
    await fillByLabel(salesPage, "客户简称", CUSTOMER_SHORT)
    await fillByLabel(salesPage, "统一社会信用代码", CREDIT_CODE)
    await pickComboboxOption(salesPage, salesPage.getByLabel("默认付款条件"), "货到 15 天")
    await clickButton(salesPage, "创建客户")
    await clickButton(salesPage, "打开客户")
    await expect(salesPage).toHaveURL(/\/sales\/customers\/[0-9a-f]{24,32}/, {
        timeout: 20_000,
    })

    // ── W04 合同上传 PDF ────────────────────────────────────────────────────
    await gotoPage(salesPage, "/sales/contracts")
    await clickButton(salesPage, "上传合同 PDF")
    const uploadDialog = dialogOn(salesPage)
    await expect(uploadDialog.getByRole("heading", { name: "上传合同 PDF" })).toBeVisible({
        timeout: 10_000,
    })
    await salesPage.locator('input[type="file"][aria-label="上传合同 PDF"]').setInputFiles(CONTRACT_PDF)
    await fillByLabel(salesPage, "合同编号", CONTRACT_NO)
    await pickSearchedOption(salesPage, "搜索客户编号或名称", CUSTOMER_LEGAL)
    // 结算主体为保留主数据，内容未知：取第一个可选主体（列表按 party_no 升序）。
    // 回款/开票登记的往来主体选择器会过滤出「已有应收子账的主体」，唯一主体即本单主体
    await pickFirstComboboxOption(salesPage, "搜索结算主体")
    // 付款条件默认"按合同约定"，日期已预填（今天/明年同日），直接归档
    await clickButton(salesPage, "上传并归档")
    await expect(salesPage.getByText("合同 PDF 已归档")).toBeVisible({ timeout: 20_000 })
    await expectTableRow(salesPage, CONTRACT_NO)

    // ── W05 销售单创建并提交 ────────────────────────────────────────────────
    await gotoPage(salesPage, "/sales/orders?mode=create")
    // 选择有效合同（自动带出客户/结算主体/付款条件）
    const contractInput = salesPage.getByPlaceholder("搜索合同编号或客户").first()
    await contractInput.click()
    await contractInput.fill(CONTRACT_NO)
    const contractItem = salesPage
        .locator('[data-slot="combobox-item"]')
        .filter({ hasText: CONTRACT_NO })
        .first()
    await expect(contractItem).toBeVisible({ timeout: 20_000 })
    await contractItem.click()
    await expect(salesPage.getByText(`${CONTRACT_NO}@v`).first()).toBeVisible({
        timeout: 20_000,
    })
    // 负责销售固定为当前登录用户，等待档案加载完成后再提交
    await expect(salesPage.getByLabel("负责销售")).toHaveValue(/.+/, {
        timeout: 20_000,
    })
    // 单据头：福利场景 / 付款条件（合同已带出则跳过）/ 履约期限
    await pickComboboxOption(salesPage, salesPage.getByLabel("福利场景"), "年节礼包")
    const paymentInput = salesPage.getByLabel("付款条件")
    if (!(await paymentInput.inputValue()).trim()) {
        await pickComboboxOption(salesPage, paymentInput, "按合同约定")
    }
    await pickDate(salesPage, "履约期限", TODAY_DAY)
    // 销售明细：选可售 SKU（主数据未知，取第一个），数量默认 1，填含税单价与交付日期
    const skuInput = salesPage.getByPlaceholder("搜索 SKU 或商品名称").first()
    await skuInput.click()
    const skuItem = salesPage.locator('[data-slot="combobox-item"]').first()
    await expect(skuItem).toBeVisible({ timeout: 20_000 })
    await skuItem.click()
    const skuName = await skuInput.inputValue()
    expect(skuName.trim().length).toBeGreaterThan(0)
    await fillByLabel(salesPage, "含税单价", UNIT_PRICE)
    await pickDate(salesPage, "交付日期", TODAY_DAY) // 明细交付日期（hideLabel，文本仍在 DOM）
    // 提交
    await clickButton(salesPage, "提交")
    const submitDialog = dialogOn(salesPage)
    await expect(submitDialog.getByText("提交销售单")).toBeVisible({ timeout: 10_000 })
    await submitDialog.getByRole("button", { name: "确认提交" }).click()
    await salesPage.waitForURL(/\/sales\/orders\/[0-9a-f]{24,32}/, { timeout: 30_000 })
    const salesOrderId = salesPage.url().match(/\/sales\/orders\/([0-9a-f]{24,32})/)![1]
    const salesOrderNo = (
        await salesPage.getByText(/XS\d{8,}\d*/).first().textContent()
    )!.trim()
    // 实际单号前缀为 XS（与文档 SO 前缀不一致，见文件头 doc_mismatches）
    expect(salesOrderNo).toMatch(/^XS\d{8,}/)
    // 里程碑：审批中（详情头部状态徽标）
    await expect(salesPage.getByText("审批中", { exact: true }).first()).toBeVisible({
        timeout: 20_000,
    })

    // ── 采购确认审批通过 → 销售单生效 ───────────────────────────────────────
    await switchAccount("procurement") // caigou
    await approveInWorkspace(poPage, salesOrderNo)
    await gotoPage(poPage, `/sales/orders/${salesOrderId}`)
    await expect(poPage.getByText("已生效").first()).toBeVisible({ timeout: 20_000 })

    // ── W08 采购建采购单并提交（依据：采购二次确认创建依据）────────────────────
    await gotoPage(poPage, "/procurement/orders")
    await clickButton(poPage, "新建采购单")
    const basisDialog = dialogOn(poPage)
    await expect(basisDialog.getByText("从采购创建依据建单")).toBeVisible({
        timeout: 10_000,
    })
    // 创建依据自动选中第一个（唯一），等待摘要出现
    await expect(basisDialog.getByText(salesOrderNo).first()).toBeVisible({
        timeout: 20_000,
    })
    await clickDialogButton(poPage, "创建草稿并打开")
    await poPage.waitForURL(/\/procurement\/orders\/[0-9a-f]{24,32}\?mode=edit/, {
        timeout: 30_000,
    })
    const purchaseOrderId = poPage.url().match(/\/procurement\/orders\/([0-9a-f]{24,32})/)![1]
    await expect(poPage.getByText("采购草稿").first()).toBeVisible({ timeout: 20_000 })
    // 供应商/成本/交期来自创建依据（只读或已带出），数量/单价/税率已带出；直接提交审批
    await clickButton(poPage, "提交审批")
    const poSubmitDialog = dialogOn(poPage)
    await expect(poSubmitDialog.getByText("提交采购单")).toBeVisible({ timeout: 10_000 })
    await poSubmitDialog.getByRole("button", { name: "确认提交" }).click()
    // 提交成功后回到详情（去掉 mode=edit），审批区进入运行态
    await poPage.waitForURL(/\/procurement\/orders\/[0-9a-f]{24,32}\/?$/, {
        timeout: 30_000,
    })
    await expect(poPage.getByText("审批中", { exact: true }).first()).toBeVisible({
        timeout: 20_000,
    })

    // ── 财务审核通过 → 采购单生效 ───────────────────────────────────────────
    await switchAccount("finance") // caiwu
    await approveInWorkspace(financePage)
    await gotoPage(financePage, `/procurement/orders/${purchaseOrderId}`)
    await expect(financePage.getByText("已生效").first()).toBeVisible({ timeout: 20_000 })

    // ── W09 采购入库（入仓）→ 仓发出库 ──────────────────────────────────────
    await switchAccount("admin") // admin 代仓储（risks 见文件头）
    await gotoPage(whPage, `/fulfillment?lane=warehouse&purchaseOrderId=${purchaseOrderId}&autoNext=0`)
    await postFulfillmentOperation(whPage, "入库", "确认入库", "已入库")
    // 入库后按销售单维度进入仓发：仓发草稿只关联销售单（purchase_order_id 为空），
    // 若沿用 purchaseOrderId 筛选会被队列过滤掉（见 queue.ts matchOperation）
    await gotoPage(whPage, `/fulfillment?lane=warehouse&salesOrderId=${salesOrderId}&autoNext=0`)
    // 仓发必填承运方与物流单号（验收/对账需要），先填好再确认发货。
    const shipCard = whPage.getByText(/^公司仓发 · /).first()
    await expect(shipCard).toBeVisible({ timeout: 30_000 })
    await whPage.getByRole("button", { name: "京东物流", exact: true }).click()
    await fillByLabel(whPage, "物流单号", `E2E${Date.now()}`)
    await postFulfillmentOperation(whPage, "公司仓发", "确认发货", "已发货")

    // ── W06 销售登记客户验收 ────────────────────────────────────────────────
    await switchAccount("sales")
    await gotoPage(salesPage, `/sales/orders/${salesOrderId}`)
    await salesPage.getByRole("tab", { name: "履约" }).click()
    await salesPage.getByRole("button", { name: "登记验收" }).click()
    await expect(salesPage.getByRole("heading", { name: "可验收的交付记录" })).toBeVisible({ timeout: 20_000 })
    const factCheckbox = salesPage.getByRole("checkbox", { name: /仓发/ }).first()
    await expect(factCheckbox).toBeVisible({ timeout: 20_000 })
    await factCheckbox.check()
    await salesPage.getByRole("button", { name: "确认并完成验收" }).click()
    const acceptanceDialog = dialogOn(salesPage)
    await expect(acceptanceDialog.getByText("确认客户验收")).toBeVisible({
        timeout: 10_000,
    })
    await acceptanceDialog.getByRole("button", { name: "确认验收" }).click()
    await expect(salesPage.getByText("客户验收已登记")).toBeVisible({ timeout: 20_000 })
    await salesPage.getByRole("button", { name: "回到履约" }).click()
    await expect(salesPage.getByText("已完成").first()).toBeVisible({ timeout: 20_000 })

    // ── W11 客户回款登记并核销 → 销售领导复核 → 过账 ─────────────────────────
    await switchAccount("finance")
    await gotoPage(financePage, "/finance/customer-accounts")
    // 应收台账出现本单（销售单生效即形成应收）
    await expectTableRow(financePage, salesOrderNo)
    await openAllocationSession(financePage, "登记回款")
    await pickDateTime(financePage, "实际到账时间", TODAY_DAY, "10:00:00")
    await fillByLabel(financePage, "到账金额（含税）", RECEIPT_AMOUNT)
    await clickButton(financePage, "加入")
    await clickButton(financePage, "填满")
    await clickButton(financePage, "确认登记并核销")
    const receiptDialog = dialogOn(financePage)
    await expect(receiptDialog.getByText("提交回款")).toBeVisible({ timeout: 10_000 })
    await receiptDialog.getByRole("button", { name: "确认提交" }).click()
    await expect(financePage.getByText("回款已提交审批")).toBeVisible({ timeout: 20_000 })
    const receiptNo = (
        await financePage.getByText(/已进入审批。单号 \S+/).first().textContent()
    )!.match(/单号 (SK-\d+)/)![1]
    // 销售领导复核
    await switchAccount("salesLeader") // lisiyong
    await approveInWorkspace(leaderPage)
    // 里程碑：回款单过账
    await switchAccount("finance")
    await gotoPage(financePage, "/finance/customer-accounts?view=receipt")
    const receiptRow = await expectTableRow(financePage, receiptNo)
    await expect(receiptRow.getByText("已过账")).toBeVisible({ timeout: 20_000 })

    // ── 销项发票登记并核销（Invoice 为 NO_APPROVAL，无需审批）────────────────
    await gotoPage(financePage, "/finance/customer-accounts")
    await openAllocationSession(financePage, "登记销项发票")
    await fillByLabel(financePage, "发票号码", INVOICE_NO)
    await pickDate(financePage, "开票日期", TODAY_DAY)
    await fillByLabel(financePage, "含税金额", RECEIPT_AMOUNT) // 不含税/税额按 13% 自动
    await clickButton(financePage, "加入")
    await clickButton(financePage, "填满")
    await clickButton(financePage, "确认登记并核销")
    const invoiceDialog = dialogOn(financePage)
    await expect(invoiceDialog.getByText("确认登记销项发票并分配")).toBeVisible({
        timeout: 10_000,
    })
    await invoiceDialog.getByRole("button", { name: "确认提交" }).click()
    await expect(financePage.getByText("销项发票已登记并分配")).toBeVisible({
        timeout: 20_000,
    })

    // ── 终态断言：销售单履约/回款/开票进度与关闭条件 ─────────────────────────
    await switchAccount("sales")
    await gotoPage(salesPage, `/sales/orders/${salesOrderId}`)
    // 履约完成 + 应收结清 → 已关闭（开票不挡结案）
    await expect(salesPage.getByText("已关闭").first()).toBeVisible({ timeout: 20_000 })
    await expect(salesPage.getByText("已结清").first()).toBeVisible()
    await salesPage.getByRole("tab", { name: "票款" }).click()
    // 票款区车道：回款 1 笔 · 已结清；开票 1 笔 · 已完成
    await expect(salesPage.getByText(/· 已结清/).first()).toBeVisible()
    await expect(salesPage.getByText(/· 已开齐/).first()).toBeVisible()
    // 发票台账出现本票
    await switchAccount("finance")
    await gotoPage(financePage, "/finance/customer-accounts?view=sales_invoice")
    await expectTableRow(financePage, INVOICE_NO)
})
