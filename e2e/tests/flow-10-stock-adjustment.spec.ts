/**
 * [flow-10] 库存调整（盘亏/盘盈）
 *
 * 文档依据: docs/erp-phase-1.md §6.5.5「库存调整」时序 + §6.4 异常处理责任规则；
 *          erp-data-model.md §6.7（库存流水/余额/调整单）；docs/ui-workspaces/w10-inventory-ledger.md。
 * 流程要点: 建立库存 → 以 admin 创建库存调整单（盘亏）→ 提交 → 财务(caiwu)审批
 *          → 末节点通过自动过账 → 断言库存台账：调整记录「已过账」、余额数量变化、
 *          正式流水出现「库存调整」。
 *
 * 使用账号（密码 123456）:
 *   - xiaoshou(销售): 建客户、上传合同、建并提交销售单（库存前置链路）
 *   - caigou(采购): 审批销售单「采购确认」节点、创建并提交采购单
 *   - caiwu(财务): 审批采购单「财务审核」、审批库存调整单「财务审批」并触发过账
 *   - admin(超级管理员 role-root): 无仓储账号，代行仓储动作（采购入库、发起库存调整、台账核查）
 *
 * 文档-代码差异（doc=文档说法 / code=代码实际行为）:
 *   1. doc: 审批待办在 W02 统一待办（/workspace/tasks）
 *      code: app/(workspace)/workspace/tasks/page.tsx 对 /workspace/tasks 做 permanentRedirect
 *            到 W01 唯一工作台 /workspace；审批决定在工作台详情区完成
 *            （features/workspace/components/workspace-task-detail.tsx + ApprovalActionBar）。
 *            待办列表只渲染一份；口径筛选在主面板工具条，无独立统计卡。
 *   2. doc: 库存调整单由仓储经办人创建（§6.4）；无仓储账号时实际以代码为准
 *      code: 预置账号中没有仓储角色；仓储职责权限（stock_adjustment:* 等）由
 *            admin(role-root, *:*) 代行；财务 FINANCE_PERMISSIONS 不含
 *            stock_adjustment:create / stock_balance:list，只能在 W01 待办中审批。
 *   3. doc: 采购二次确认是独立队列（W07）
 *      code: docs/ui-workspaces/w07-procurement-confirmation-queue.md 已废止；「采购确认」
 *           收敛为 SalesOrder 审批定义中的普通节点（publish-approval-definitions.mjs）。
 *   4. doc: §7 采购与履约流程描述采购单从「采购二次确认产生的创建依据」建单（W08）
 *      code: 旧「采购二次确认」队列已废止；采购单从「从采购创建依据建单」弹窗创建，
 *           依据在销售单生效后由系统推导（前端 creation-basis 接口已接线，
 *           弹窗空态文案「当前没有可消费的创建依据」仅在依据未就绪时出现）。
 *   5. doc: W10 空数据文案「当前仓库尚无 ERP 自有库存事实」
 *      code: ledger-table.tsx 实际文案为「当前仓库尚无 ERP 自有库存记录」。
 *   6. doc: 库存调整审批通过后由系统过账（§6.5.5 末节点通过→原子执行库存调整）
 *      code: services/src/inventory/adapter.rs 中 on_final_approve =
 *           ApprovalDomainAction::StockAdjustmentPost，末节点（唯一节点财务审批）通过时
 *           同事务写流水并更新余额，无需人工「过账」动作；前端无过账按钮。
 *   7. 本流程验证的修复（历史缺陷 → 现状）:
 *      - 原因说明/业务发生时间随保存落库，过账时带入正式流水（reason_text/occurred_at）；
 *      - 余额台账「最后变动」列显示流水类型与业务时间（此前恒 Invalid Date，
 *        余额 last_movement_id 从未被写入）；
 *      - 流水视图「记录人」列显示后端记录人（此前恒为空）。
 *
 * 运行前提（见 e2e/README.md）：run-flow.sh 已 reset 数据库（业务数据为空、无种子，
 * 保留账号/RBAC 与供应商/商品/仓库主数据）并发布 12 个审批定义
 * （StockAdjustment=财务审批(caiwu)）。
 */
import { Locator, Page, expect, test } from "@playwright/test"

import { api, apiLogin } from "../helpers/api"
import { createSinglePageAccountSwitcher } from "../helpers/login"
import { completePurchaseOrderCreate, pickOption } from "../helpers/ui"
import {
    approveFirstWorkspaceTask,
    approveWorkspaceTaskByDocumentNo,
} from "../helpers/workspace"

// 库存调整弹窗内容较长，使用全局最大化窗口与实际 viewport，禁止退回 720px 固定视口。

// ─── 流程常量 ────────────────────────────────────────────────────────────────

/** 唯一标识本流程（客户/合同后缀），避免与历史数据混淆。 */
const FLOW_TAG = `f10-${Date.now().toString(36)}`

const CUSTOMER_NAME = `测试客户${FLOW_TAG}`
/** 统一社会信用代码：18 位字母数字。 */
const CUSTOMER_CODE = `91${FLOW_TAG.replace(/[^0-9a-z]/gi, "").slice(0, 8)}`.padEnd(18, "X")
const CONTRACT_NO = `HT-${FLOW_TAG}`
/** 入库合格数量（账面现存基线）。 */
const RECEIPT_QTY = "20"
/** 盘亏数量。 */
const ADJUST_QTY = "2"
/** 盘亏后账面现存期望值。 */
const EXPECTED_ON_HAND = "18"
/** 盘盈数量（用例 2）。 */
const GAIN_QTY = "3"
/** 盘亏 2 + 盘盈 3 后账面现存期望值。 */
const EXPECTED_ON_HAND_AFTER_GAIN = "21"

/**
 * 合同 PDF 夹具。相对路径按 run-flow.sh 约定解析：playwright 从 e2e 目录执行
 * （cwd = e2e/），故相对 e2e 目录的路径为 "fixtures/sample-contract.pdf"。
 */
const CONTRACT_PDF = "fixtures/sample-contract.pdf"

/** 前端 createAdjustmentDraft 生成的调整单号（TZ + base36 时间戳）。 */
const ADJUST_NO_PATTERN = /TZ[0-9A-Z]+/

// ─── 流程内小工具（仅本 spec 使用，勿写入 helpers） ─────────────────────────

/**
 * 选择组合框选项：点击触发元素后，在页面级（选项弹层 portal 到 body）选择选项。
 * 本地枚举（OptionCombobox / SelectField）与远程搜索组合框通用：
 * 首屏无目标选项时向输入框输入关键字触发查询（防抖后重试）。
 */
async function selectOption(
    page: Page,
    trigger: Locator,
    optionText: string,
): Promise<void> {
    await trigger.click()
    const option = page.getByRole("option", { name: optionText, exact: false }).first()
    try {
        await option.waitFor({ state: "visible", timeout: 5_000 })
    } catch {
        await trigger.fill(optionText)
        await option.waitFor({ state: "visible", timeout: 15_000 })
    }
    await option.click()
}

/**
 * 在日期弹层中选择「今天」。DatePicker 触发钮空态 aria-label 为 placeholder
 * （"选择日期"）；react-day-picker 日期格按钮文本为日号（禁用/相邻月格排除）。
 */
async function pickToday(page: Page, trigger: Locator): Promise<void> {
    await trigger.click()
    const popover = page.locator('[data-slot="popover-content"], [role="dialog"]').last()
    await expect(popover).toBeVisible({ timeout: 10_000 })
    const day = String(new Date().getDate())
    await popover
        .locator("button:not([disabled])")
        .filter({ hasText: new RegExp(`^${day}$`) })
        .first()
        .click()
    await page.keyboard.press("Escape").catch(() => {})
    await expect(popover).not.toBeVisible({ timeout: 10_000 }).catch(() => {})
}

/**
 * W01 工作台审批：打开当前唯一审批任务 → 通过。
 * 待办行展示类型标签 + 稳定单号；详情标题带业务对象标签。
 * 数据库已重置，当前账号此刻只有一个待办，直接取第一行。
 */
async function approveFirstTask(page: Page): Promise<void> {
    await approveFirstWorkspaceTask(page)
}

/** 同上，并校验列表或详情含期望单号。 */
async function approveByDocumentNo(page: Page, docNo: string): Promise<void> {
    await approveWorkspaceTaskByDocumentNo(page, docNo)
}

/**
 * 读取余额详情抽屉（QuickPreviewSheet，data-slot="quick-preview-content"）中
 * 「账面现存/有效预占/可用数量」的数值。抽屉内三列网格，每列 = 标签 div + 数值 div。
 */
async function readPreviewStat(scope: Locator, label: string): Promise<string> {
    const col = scope.getByText(label, { exact: true }).first().locator("..")
    const text = (await col.innerText()).trim()
    const value = text.split("\n").pop()?.trim() ?? ""
    expect(value).not.toBe("")
    return value
}

// ─── 用例 ────────────────────────────────────────────────────────────────────

test.describe.configure({ mode: "serial" })

test("flow-10 库存调整：创建盘亏调整单 → 财务审批 → 自动过账 → 台账数量变化", async ({
    page,
    request,
}) => {
    const switchAccount = createSinglePageAccountSwitcher(page)
    const adminPage = page
    const salesPage = page
    const caigouPage = page
    const caiwuPage = page

    // ============================================================
    // S1 基线核查（admin）：reset 后台账为空（业务数据从 0 开始）
    // ============================================================
    await switchAccount("admin")
    await adminPage.goto("/inventory")
    await expect(adminPage.getByText("库存台账").first()).toBeVisible({ timeout: 30_000 })
    await expect(
        adminPage.getByText("当前仓库尚无 ERP 自有库存记录"),
    ).toBeVisible({ timeout: 30_000 })
    // 视图页签齐全：余额 | 流水 | 销售预占 | 调整记录
    for (const tab of ["余额", "流水", "销售预占", "调整记录"]) {
        await expect(adminPage.getByRole("tab", { name: tab })).toBeVisible()
    }

    // ============================================================
    // S2 建立库存（前置）：销售链 → 采购单 → 采购入库（UI 创建）
    // ============================================================

    // ---- S2.1 销售(xiaoshou) 创建客户 ----
    await switchAccount("sales")
    await salesPage.goto("/sales/customers")
    await salesPage.getByRole("button", { name: "新建客户" }).first().click()
    const customerDialog = salesPage.getByRole("dialog")
    await expect(customerDialog).toBeVisible({ timeout: 20_000 })
    await customerDialog.getByLabel("法定名称", { exact: false }).fill(CUSTOMER_NAME)
    await customerDialog.getByLabel("统一社会信用代码", { exact: false }).fill(CUSTOMER_CODE)
    await customerDialog.getByRole("button", { name: "创建客户" }).click()
    await expect(customerDialog).not.toBeVisible({ timeout: 20_000 })
    await expect(salesPage.getByText(CUSTOMER_NAME).first()).toBeVisible({
        timeout: 20_000,
    })

    // ---- S2.2 销售(xiaoshou) 上传合同 PDF（fixtures/sample-contract.pdf）----
    await salesPage.goto("/sales/contracts")
    await salesPage.getByRole("button", { name: "上传合同 PDF" }).first().click()
    const contractDialog = salesPage.getByRole("dialog")
    await expect(contractDialog).toBeVisible({ timeout: 20_000 })
    await contractDialog.getByLabel("上传合同 PDF").setInputFiles(CONTRACT_PDF)
    await contractDialog.getByLabel("合同编号", { exact: false }).fill(CONTRACT_NO)
    // 客户/结算主体为 Base UI 远程搜索组合框（无 aria-label，用 placeholder 定位）
    await selectOption(salesPage, contractDialog.getByPlaceholder("搜索客户编号或名称"), CUSTOMER_NAME)
    await selectOption(salesPage, contractDialog.getByPlaceholder("搜索结算主体"), CUSTOMER_NAME)
    // 付款条件：SelectField 即 OptionCombobox（aria-label=付款条件，placeholder 请选择）
    await selectOption(salesPage, contractDialog.getByLabel("付款条件"), "货到 30 天")
    // 签订日期/有效期起/有效期止由 use-contract-upload-form 打开时自动预填（今天~明年）
    await contractDialog.getByRole("button", { name: "上传并归档" }).click()
    await expect(contractDialog).not.toBeVisible({ timeout: 30_000 })
    await expect(salesPage.getByText(CONTRACT_NO).first()).toBeVisible({
        timeout: 30_000,
    })

    // ---- S2.3 销售(xiaoshou) 创建并提交实物销售单 ----
    await salesPage.goto("/sales/orders?mode=create")
    const orderForm = salesPage.locator("form").last()
    // 有效合同（Base UI 远程搜索组合框，placeholder「搜索合同编号或客户」）
    await selectOption(
        salesPage,
        orderForm.getByPlaceholder("搜索合同编号或客户"),
        CONTRACT_NO,
    )
    await selectOption(salesPage, orderForm.getByLabel("福利场景"), "年节礼包")
    await selectOption(salesPage, orderForm.getByLabel("付款条件"), "货到 30 天")
    // 履约期限（单据头第一个「选择日期」触发钮）
    await pickToday(salesPage, orderForm.getByRole("button", { name: /^(选择日期|已选日期)/ }).first())
    // 销售明细：公司商品池 SKU + 数量 + 含税单价 + 交付日期
    const skuCombobox = orderForm.getByPlaceholder("搜索 SKU 或商品名称")
    await skuCombobox.click()
    // 远程搜索：等待防抖请求返回；若首屏无结果需输入关键字（见 risks）
    await salesPage.waitForTimeout(1500)
    const skuOption = salesPage.getByRole("option").first()
    await expect(skuOption).toBeVisible({ timeout: 15_000 })
    // 选项文本多行（名称/可售状态/SKU 编码/单位与供应商），仅取首行商品名
    const skuName = (await skuOption.innerText()).trim().split("\n")[0]
    await skuOption.click()
    await orderForm.getByLabel("数量", { exact: false }).fill("10")
    await orderForm.getByLabel("含税单价", { exact: false }).fill("100")
    await pickToday(salesPage, orderForm.getByRole("button", { name: /^(选择日期|已选日期)/ }).last())
    // 提交 → 正式确认弹窗
    await orderForm.getByRole("button", { name: "提交", exact: true }).click()
    const salesConfirm = salesPage.getByRole("alertdialog")
    await expect(salesConfirm).toBeVisible({ timeout: 20_000 })
    await expect(salesConfirm.getByText("确认提交销售单")).toBeVisible()
    await salesConfirm.getByRole("button", { name: "确认提交" }).click()
    // 提交后进入销售单详情并处于审批中
    await expect(salesPage).toHaveURL(/\/sales\/orders\//, { timeout: 30_000 })
    await expect(salesPage.getByText("审批中").first()).toBeVisible({ timeout: 30_000 })

    // ---- S2.4 采购(caigou) 审批销售单「采购确认」节点 → 销售单生效 ----
    await switchAccount("procurement")
    await caigouPage.goto("/workspace")
    await approveFirstTask(caigouPage)
    await expect(caigouPage.getByText("当前没有待处理事项").first()).toBeVisible({
        timeout: 30_000,
    })

    // ---- S2.5 采购(caigou) 从创建依据建采购单并提交 ----
    await caigouPage.goto("/procurement/orders")
    await caigouPage.getByRole("button", { name: "新建采购单" }).first().click()
    await expect(caigouPage).toHaveURL(/mode=create/, { timeout: 20_000 })
    const basisEmptyHint = caigouPage.getByText("当前没有可建采购依据")
    if (await basisEmptyHint.isVisible().catch(() => false)) {
        throw new Error(
            "前置缺陷（阻断 flow-10）：销售单已生效，但新建采购单页没有可建采购依据，无法继续库存调整核心流程。",
        )
    }
    await completePurchaseOrderCreate(caigouPage)
    // 采购草稿编辑面：付款条件来自创建依据且只读，仅数量/含税单价可调整。
    await expect(caigouPage.getByText("采购草稿").first()).toBeVisible({ timeout: 30_000 })
    await expect(
        caigouPage.getByText("付款条件（只读）", { exact: true }),
    ).toBeVisible()
    await caigouPage.getByRole("textbox", { name: /数量$/ }).fill(RECEIPT_QTY)
    await caigouPage.getByRole("textbox", { name: /含税单价/ }).fill("80")
    await caigouPage.getByRole("button", { name: "提交审批" }).click()
    const poConfirm = caigouPage.getByRole("alertdialog")
    await expect(poConfirm).toBeVisible({ timeout: 20_000 })
    await expect(poConfirm.getByText("确认提交采购单")).toBeVisible()
    await poConfirm.getByRole("button", { name: "确认提交" }).click()
    // 提交成功进入详情页；确认对话框自身含「草稿→审批中」文案，不能以文本「审批中」
    // 断言成功（提交慢时弹窗未关，测试切号会中止在途保存请求，造成静默失败），
    // 以详情页「撤回审批」入口作为成功信号（与 flow-07/flow-09 一致）
    await expect(
        caigouPage.getByRole("button", { name: "撤回审批" }).first(),
    ).toBeVisible({ timeout: 30_000 })

    // ---- S2.6 财务(caiwu) 审批采购单「财务审核」→ 采购单生效 ----
    await switchAccount("finance")
    await caiwuPage.goto("/workspace")
    await approveFirstTask(caiwuPage)

    // ---- S2.7 admin 代行仓储：采购入库（W09 收货与发货，PurchaseReceipt 为 NO_APPROVAL）----
    await switchAccount("admin")
    await adminPage.goto("/fulfillment?lane=warehouse")
    // 待处理单据出现「入库」作业。注意：收货 DTO 不含供应商/客户标签（显示缺口），
    // 卡片无法按客户名过滤，重置后当前唯一入库作业，直接取第一条
    const receiptItem = adminPage
        .getByRole("button")
        .filter({ hasText: "入库" })
        .filter({ hasText: "待处理" })
        .first()
    await expect(receiptItem).toBeVisible({ timeout: 30_000 })
    await receiptItem.click()
    const receiptForm = adminPage.locator('section[aria-label="入库表单"]')
    await expect(receiptForm).toBeVisible({ timeout: 20_000 })
    await receiptForm.getByLabel("到货数量", { exact: false }).fill(RECEIPT_QTY)
    // 合格数量由 withDerivedQualified 自动算（到货 − 不合格）
    await pickOption(adminPage, receiptForm.getByPlaceholder("选择质量结果"), "合格")
    await adminPage
        .getByRole("button", { name: "确认入库", exact: true })
        .click()
    const receiptConfirm = adminPage.getByRole("alertdialog")
    await expect(receiptConfirm).toBeVisible({ timeout: 20_000 })
    await expect(receiptConfirm.getByText("确认入库？")).toBeVisible()
    await receiptConfirm.getByRole("button", { name: "确认入库" }).click()
    // 确认弹窗只在过账请求完成后关闭（弹窗描述里也含「已入库」文案，不能按文本断言成功）
    await expect(receiptConfirm).not.toBeVisible({ timeout: 30_000 })

    // ---- S2.8 admin 核查台账：余额行出现且账面现存 = 20 ----
    await adminPage.goto("/inventory")
    const balanceRow = adminPage
        .locator("table")
        .first()
        .getByRole("row")
        .filter({ hasText: skuName })
        .first()
    await expect(balanceRow).toBeVisible({ timeout: 30_000 })
    // 打开余额详情抽屉读取账面现存（抽屉内容加载后出现「账面现存」标签）
    await balanceRow.getByRole("button", { name: "查看" }).click()
    const sheet = adminPage.locator('[data-slot="quick-preview-content"]').last()
    await expect(sheet.getByText("账面现存").first()).toBeVisible({ timeout: 20_000 })
    expect(await readPreviewStat(sheet, "账面现存")).toBe(RECEIPT_QTY)
    await adminPage.getByRole("button", { name: "关闭" }).first().click()

    // ============================================================
    // S3 核心流程（admin）：发起库存调整（盘亏）→ 填写 → 提交审批
    // ============================================================
    await balanceRow.getByRole("button", { name: "库存调整" }).click()
    const adjustDialog = adminPage.getByRole("dialog")
    await expect(adjustDialog).toBeVisible({ timeout: 20_000 })
    await expect(adjustDialog.getByText("发起库存调整")).toBeVisible()

    // 原因类型：盘亏（减少）——Base UI 组合框，placeholder「原因类型」
    await pickOption(adminPage, adjustDialog.getByPlaceholder("原因类型"), "盘亏（减少）")
    // 调整数量（label=调整数量（基础单位，正数）；基础单位后端暂缺，前缀匹配）
    await adjustDialog.getByLabel("调整数量", { exact: false }).fill(ADJUST_QTY)
    await adjustDialog.getByLabel("原因说明", { exact: false }).fill("E2E 盘亏测试")
    // 业务发生时间：改为今天 00:07（默认预填当前时间）——验证业务时间落库并带入正式流水
    const bizDate = (() => {
        const now = new Date()
        return `${now.getFullYear()}/${String(now.getMonth() + 1).padStart(2, "0")}/${String(
            now.getDate(),
        ).padStart(2, "0")}`
    })()
    const bizLocalInput = `${bizDate.replaceAll("/", "-")}T00:07:00`
    await adjustDialog.getByRole("button", { name: /· Asia\/Shanghai/ }).click()
    const bizTimeInput = adminPage.getByLabel("时间，精确到秒")
    await expect(bizTimeInput).toBeVisible({ timeout: 10_000 })
    await bizTimeInput.fill("00:07:00")
    await adminPage.getByRole("button", { name: "完成" }).click()
    await expect(bizTimeInput).not.toBeVisible({ timeout: 10_000 }).catch(() => {})

    await adjustDialog.getByRole("button", { name: "提交审批" }).click()
    const adjustConfirm = adminPage.getByRole("alertdialog")
    await expect(adjustConfirm).toBeVisible({ timeout: 20_000 })
    await expect(adjustConfirm.getByText("确认提交库存调整")).toBeVisible()
    await expect(adjustConfirm.getByText("不立即修改账面、预占和可用数量")).toBeVisible()
    await adjustConfirm.getByRole("button", { name: "确认提交" }).click()

    // 提交成功横幅：单号 TZ… + 余额尚未变化（审批通过后由系统更新）
    await expect(adminPage.getByText("调整已提交审批")).toBeVisible({ timeout: 30_000 })
    const adjustNo = (await adminPage.getByText(ADJUST_NO_PATTERN).first().innerText()).match(
        ADJUST_NO_PATTERN,
    )?.[0]
    expect(adjustNo).toBeTruthy()
    await expect(adminPage.getByText(/余额尚未变化/).first()).toBeVisible({
        timeout: 10_000,
    })

    // 原因说明与业务发生时间随保存落库（GET 调整单列表断言；修复前二者均不落库）
    const adminToken = await apiLogin(request, "admin")
    const adjustmentPage = await api<{
        items: Array<{
            adjustment_no: string
            note: string | null
            occurred_at: number | null
        }>
    }>(request, "GET", "/admin/stock-adjustments", {
        token: adminToken,
        query: { page: 1, page_size: 50 },
    })
    const persisted = adjustmentPage.items.find((item) => item.adjustment_no === adjustNo)
    expect(persisted).toBeTruthy()
    expect(persisted?.note).toBe("E2E 盘亏测试")
    expect(persisted?.occurred_at).toBe(
        Math.floor(new Date(bizLocalInput).getTime() / 1000),
    )

    // 调整记录视图出现「审批中」记录（提交后、审批前里程碑）
    await adminPage.getByRole("tab", { name: "调整记录" }).click()
    const adjustRow = adminPage
        .locator("table")
        .first()
        .getByRole("row")
        .filter({ hasText: adjustNo! })
        .first()
    await expect(adjustRow).toBeVisible({ timeout: 20_000 })
    await expect(adjustRow.getByText("审批中").first()).toBeVisible({ timeout: 20_000 })
    await expect(adjustRow.getByText(/盘亏/).first()).toBeVisible()

    // ============================================================
    // S4 核心流程（caiwu）：财务审批「财务审批」节点 → 末节点通过自动过账
    // ============================================================
    await switchAccount("finance")
    await caiwuPage.goto("/workspace")
    await approveByDocumentNo(caiwuPage, adjustNo!)

    // ============================================================
    // S5 断言（admin）：调整记录已过账 + 余额变化 + 正式流水
    // ============================================================
    // S5.1 调整记录：状态「已过账」
    await switchAccount("admin")
    await adminPage.goto("/inventory")
    await adminPage.getByRole("tab", { name: "调整记录" }).click()
    const postedRow = adminPage
        .locator("table")
        .first()
        .getByRole("row")
        .filter({ hasText: adjustNo! })
        .first()
    await expect(postedRow).toBeVisible({ timeout: 30_000 })
    await expect(postedRow.getByText("已过账").first()).toBeVisible({ timeout: 30_000 })

    // S5.2 余额视图：账面现存 20 → 18（盘亏 2）
    await adminPage.getByRole("tab", { name: "余额" }).click()
    const balanceRowAfter = adminPage
        .locator("table")
        .first()
        .getByRole("row")
        .filter({ hasText: skuName })
        .first()
    await expect(balanceRowAfter).toBeVisible({ timeout: 30_000 })
    // 最后变动列：显示流水类型「库存调整」与合法业务时间（修复前恒为 Invalid Date）
    const lastChangedCell = balanceRowAfter.getByRole("cell").nth(5)
    await expect(lastChangedCell.getByText("库存调整").first()).toBeVisible()
    await expect(
        lastChangedCell.getByText(new RegExp(`^${bizDate} 00:07$`)).first(),
    ).toBeVisible()
    await balanceRowAfter.getByRole("button", { name: "查看" }).click()
    const sheetAfter = adminPage.locator('[data-slot="quick-preview-content"]').last()
    await expect(sheetAfter.getByText("账面现存").first()).toBeVisible({ timeout: 20_000 })
    expect(await readPreviewStat(sheetAfter, "账面现存")).toBe(EXPECTED_ON_HAND)
    await adminPage.getByRole("button", { name: "关闭" }).first().click()

    // S5.3 流水视图：出现「库存调整 · 减少 2」的正式流水，来源为调整单
    await adminPage.getByRole("tab", { name: "流水" }).click()
    const movementRow = adminPage
        .locator("table")
        .first()
        .getByRole("row")
        .filter({ hasText: "库存调整" })
        .filter({ hasText: adjustNo! })
        .first()
    await expect(movementRow).toBeVisible({ timeout: 30_000 })
    await expect(movementRow.getByText("减少").first()).toBeVisible()
    await expect(movementRow.getByText(ADJUST_QTY).first()).toBeVisible()
    // 业务发生时间落库：流水「发生」时间 = 表单选择的业务时间（今天 00:07），而非过账时刻
    await expect(
        movementRow.getByText(new RegExp(`发生 ${bizDate} 00:07`)).first(),
    ).toBeVisible()
    // 记录人：流水视图不再为空（后端 recorded_by 已透出）
    const recorderCell = movementRow.getByRole("cell").nth(5)
    expect((await recorderCell.innerText()).trim()).not.toBe("")
})

test("flow-10 库存调整：盘盈调整单 → 财务审批 → 自动过账 → 台账数量增加", async ({
    page,
}) => {
    const switchAccount = createSinglePageAccountSwitcher(page)
    const adminPage = page
    const caiwuPage = page

    // 前置：上一用例结束后账面现存 = 18（盘亏 2）。本用例验证盘盈（增加）方向
    // 从 UI 提交到过账的全链路（历史缺陷：草稿固定为减少方向，盘盈提交后
    // 过账被「原因类型与明细方向不一致」拒绝）。

    // ---- G1 admin 核查基线：余额 18 ----
    await switchAccount("admin")
    await adminPage.goto("/inventory")
    const balanceRow = adminPage
        .locator("table")
        .first()
        .getByRole("row")
        .filter({ hasText: "E2E-WH-TEST1" })
        .first()
    await expect(balanceRow).toBeVisible({ timeout: 30_000 })
    // 行首格文本格式「仓库名\nSKU 编码 · SKU 名称」，只取首格，取名称用于后续余额行过滤
    const skuName = (
        await balanceRow.getByRole("cell").first().innerText()
    )
        .split("·")[1]
        ?.trim()
    expect(skuName).toBeTruthy()
    await balanceRow.getByRole("button", { name: "查看" }).click()
    const sheet = adminPage.locator('[data-slot="quick-preview-content"]').last()
    await expect(sheet.getByText("账面现存").first()).toBeVisible({ timeout: 20_000 })
    expect(await readPreviewStat(sheet, "账面现存")).toBe(EXPECTED_ON_HAND)
    await adminPage.getByRole("button", { name: "关闭" }).first().click()

    // ---- G2 发起盘盈调整并提交审批 ----
    await balanceRow.getByRole("button", { name: "库存调整" }).click()
    const adjustDialog = adminPage.getByRole("dialog")
    await expect(adjustDialog).toBeVisible({ timeout: 20_000 })
    await pickOption(adminPage, adjustDialog.getByPlaceholder("原因类型"), "盘盈（增加）")
    await adjustDialog.getByLabel("调整数量", { exact: false }).fill(GAIN_QTY)
    await adjustDialog.getByLabel("原因说明", { exact: false }).fill("E2E 盘盈测试")
    await adjustDialog.getByRole("button", { name: "提交审批" }).click()
    const adjustConfirm = adminPage.getByRole("alertdialog")
    await expect(adjustConfirm).toBeVisible({ timeout: 20_000 })
    await expect(adjustConfirm.getByText("确认提交库存调整")).toBeVisible()
    await adjustConfirm.getByRole("button", { name: "确认提交" }).click()
    await expect(adminPage.getByText("调整已提交审批")).toBeVisible({ timeout: 30_000 })
    const gainNo = (await adminPage.getByText(ADJUST_NO_PATTERN).first().innerText()).match(
        ADJUST_NO_PATTERN,
    )?.[0]
    expect(gainNo).toBeTruthy()

    // 调整记录：审批中 + 盘盈
    await adminPage.getByRole("tab", { name: "调整记录" }).click()
    const gainRow = adminPage
        .locator("table")
        .first()
        .getByRole("row")
        .filter({ hasText: gainNo! })
        .first()
    await expect(gainRow).toBeVisible({ timeout: 20_000 })
    await expect(gainRow.getByText("审批中").first()).toBeVisible({ timeout: 20_000 })
    await expect(gainRow.getByText(/盘盈/).first()).toBeVisible()

    // ---- G3 财务（caiwu）审批 → 末节点通过自动过账 ----
    await switchAccount("finance")
    await caiwuPage.goto("/workspace")
    await approveByDocumentNo(caiwuPage, gainNo!)

    // ---- G4 断言：已过账 + 余额 18 → 21 + 流水「库存调整 · 增加 3」 ----
    await switchAccount("admin")
    await adminPage.goto("/inventory")
    await adminPage.getByRole("tab", { name: "调整记录" }).click()
    const postedGain = adminPage
        .locator("table")
        .first()
        .getByRole("row")
        .filter({ hasText: gainNo! })
        .first()
    await expect(postedGain).toBeVisible({ timeout: 30_000 })
    await expect(postedGain.getByText("已过账").first()).toBeVisible({ timeout: 30_000 })

    await adminPage.getByRole("tab", { name: "余额" }).click()
    const balanceRowAfter = adminPage
        .locator("table")
        .first()
        .getByRole("row")
        .filter({ hasText: skuName })
        .first()
    await expect(balanceRowAfter).toBeVisible({ timeout: 30_000 })
    await balanceRowAfter.getByRole("button", { name: "查看" }).click()
    const sheetAfter = adminPage.locator('[data-slot="quick-preview-content"]').last()
    await expect(sheetAfter.getByText("账面现存").first()).toBeVisible({ timeout: 20_000 })
    expect(await readPreviewStat(sheetAfter, "账面现存")).toBe(EXPECTED_ON_HAND_AFTER_GAIN)
    await adminPage.getByRole("button", { name: "关闭" }).first().click()

    // 流水：来源为盘盈调整单（单号过滤避免命中上一用例的盘亏流水）
    await adminPage.getByRole("tab", { name: "流水" }).click()
    const movementRow = adminPage
        .locator("table")
        .first()
        .getByRole("row")
        .filter({ hasText: "库存调整" })
        .filter({ hasText: gainNo! })
        .first()
    await expect(movementRow).toBeVisible({ timeout: 30_000 })
    await expect(movementRow.getByText("增加").first()).toBeVisible()
    await expect(movementRow.getByText(GAIN_QTY).first()).toBeVisible()
})
