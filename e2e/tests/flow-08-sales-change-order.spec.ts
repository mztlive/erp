/**
 * [flow-08] 销售变更单（SalesChangeOrder）
 *
 * 文档依据: docs/erp-phase-1.md §6.5.1（销售变更时序）+ §6.3（异常处理单据表）
 * 使用账号: xiaoshou(销售) / caigou(采购) / lisiyong(销售领导)，密码 123456
 *
 * 流程: 销售单生效（不履约）→ 销售在销售单详情「发起改单」创建改单草稿 →「提交改单」
 *       → 采购确认节点（caigou）→ 销售领导复核节点（lisiyong）→ 变更生效
 *       → 销售单追加新版本 v2，明细/金额按改单工作副本更新（历史 v1 保留并标注改单来源）
 *
 * 待办审批位置: /workspace/tasks 已重定向到唯一工作台 /workspace（W01），审批决定在页内完成。
 *
 * 文档-代码差异（以代码为准，代码依据见下文各注）:
 * 1. 文档 §6.5.1 时序为「创建销售变更单并填写原因 → 采购确认 → 财务复核」；
 *    实际已发布定义（e2e/scripts/publish-approval-definitions.mjs）SalesChangeOrder =
 *    采购确认(caigou) → 销售领导复核(lisiyong)，无财务节点；后端路由仅
 *    create/detail/submit-impact/void/cancel-approval（apps/web-api/src/core/routes/sales_review.rs）。
 * 2. 文档要求「调整数量/金额并填原因」；当前前端「发起改单」（sales-order-detail-command-dialogs.tsx）
 *    按当前工作副本原样复制明细创建改单草稿，原因由 prepareStartSalesChangeOrder 固定为
 *    「销售发起变更」，没有编辑改单明细/原因的 UI，也没有改单草稿更新接口。
 *    因此本用例断言「生效后追加 v2 且金额/明细与改单工作副本一致」，而不是金额变化。
 * 3. W02 统一待办路由 /workspace/tasks 为 permanentRedirect 到 /workspace（W01），不再有独立待办页。
 * 4. 工作台待办类型标签表（features/workspace/api/work-item-meta.ts）未配置 DOCUMENT_APPROVAL
 *    中文名，审批任务列表显示原始类型码「DOCUMENT_APPROVAL」。
 */
import {
    test,
    expect,
    type Locator,
    type Page,
    type APIRequestContext,
} from "@playwright/test"
import * as path from "path"

import { createSinglePageAccountSwitcher } from "../helpers/login"
import { api, apiLogin } from "../helpers/api"
import { gotoPage } from "../helpers/ui"

/** 弹窗统一选择器（shadcn Dialog=role dialog / AlertDialog=role alertdialog）。 */
const DIALOG_SELECTOR = '[role="dialog"], [role="alertdialog"]'

/** 当前页面最后一个弹窗。 */
function lastDialog(page: Page) {
    return page.locator(DIALOG_SELECTOR).last()
}

/** 点击弹窗内按钮并等待弹窗关闭。 */
async function confirmInDialog(page: Page, buttonName: string): Promise<void> {
    const dialog = lastDialog(page)
    await dialog.getByRole("button", { name: buttonName, exact: true }).first().click()
    await expect(dialog).not.toBeVisible({ timeout: 20_000 })
}

/**
 * 可搜索下拉/组合框选择：点击输入框 → 输入关键字 → 点击弹出项。
 * 弹出项以 data-slot="combobox-item" 定位（base-ui Combobox，选项渲染在 portal）。
 */
async function selectComboboxOption(
    page: Page,
    scope: Locator,
    ariaLabel: string,
    search: string,
    optionText: string,
): Promise<void> {
    const input = scope.getByRole("combobox", { name: ariaLabel })
    await input.click()
    await input.fill(search)
    const option = page
        .locator('[data-slot="combobox-item"]')
        .filter({ hasText: optionText })
        .first()
    await expect(option).toBeVisible({ timeout: 15_000 })
    await option.click()
}

const MONTH_NAMES_EN = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
]

function ordinalSuffix(day: number): string {
    if (day % 100 >= 11 && day % 100 <= 13) return "th"
    switch (day % 10) {
        case 1: return "st"
        case 2: return "nd"
        case 3: return "rd"
        default: return "th"
    }
}

/** react-day-picker 日按钮 accessible name 为完整日期（如 "Saturday, August 15th, 2026"）。 */
function dayButtonName(day: string): RegExp {
    const now = new Date()
    const d = Number(day)
    return new RegExp(
        `^\\w+, ${MONTH_NAMES_EN[now.getMonth()]} ${d}${ordinalSuffix(d)}, ${now.getFullYear()}$`,
    )
}

/** 打开日期选择并点当前月的固定日（15 日），避免月份边界漂移。 */
async function pickCalendarDay(page: Page, day = "15"): Promise<void> {
    const calendar = page.locator('[data-slot="calendar"]').last()
    await expect(calendar).toBeVisible({ timeout: 10_000 })
    await calendar.getByRole("button", { name: dayButtonName(day) }).first().click()
}

/** 点击作用域内第一个「选择日期」按钮并选日。 */
async function pickDateInScope(page: Page, scope: Locator): Promise<void> {
    await scope.getByRole("button", { name: /^(选择日期|已选日期)/ }).first().click()
    await pickCalendarDay(page)
}

/**
 * 在工作台（W01）处理第一条审批任务：通过 → 提交决定 → 弹窗关闭。
 * 数据库每个流程前已重置，各审批人每个节点恰好一条待办。
 */
async function approveFirstWorkspaceTask(page: Page): Promise<void> {
    await gotoPage(page, "/workspace")
    const list = page.locator('ul[aria-label="待办列表"]')
    await expect(list.getByRole("button").first()).toBeVisible({ timeout: 20_000 })
    await list.getByRole("button").first().click()
    const approve = page.getByRole("button", { name: "通过", exact: true }).first()
    await expect(approve).toBeVisible({ timeout: 20_000 })
    await approve.click()
    const dialog = lastDialog(page)
    await expect(dialog).toBeVisible({ timeout: 20_000 })
    await dialog.getByRole("button", { name: "提交决定", exact: true }).click()
    await expect(dialog).not.toBeVisible({ timeout: 20_000 })
    // 审批完成后工作台回到空态（无下一项待办）
    await expect(page.getByText("当前没有待处理事项").first()).toBeVisible({ timeout: 20_000 })
}

type SellableSkuRow = {
    sku_id: string
    sku_revision_id: string
    sku_no: string
    product_kind: string
    name: string
    base_unit_name?: string | null
}

/**
 * 只读发现一笔可销售的实物/服务 SKU（数据库重置保留商品主数据，名称不硬编码）。
 * 只做读取，业务数据仍全部通过 UI 创建。
 */
async function discoverSellableSku(
    request: APIRequestContext,
    token: string,
): Promise<{ skuNo: string; name: string }> {
    const page = await api<{ items: SellableSkuRow[] }>(
        request,
        "GET",
        "/admin/sellable-skus",
        { token, query: { page: 1, page_size: 20 } },
    )
    const row = page.items.find((item) => item.product_kind.toUpperCase() !== "VOUCHER")
    if (!row) throw new Error("未发现可销售的实物/服务 SKU 主数据（sellable-skus 为空）")
    return { skuNo: row.sku_no, name: row.name }
}

test("flow-08 销售变更单：生效销售单发起改单 → 采购确认 → 销售领导复核 → 生效追加 v2", async ({
    page,
    request,
}) => {
    test.setTimeout(300_000)

    const switchAccount = createSinglePageAccountSwitcher(page)
    const caigouPage = page
    const leaderPage = page

    // 准备：账号 token 与可销售 SKU（只读发现主数据）
    const salesToken = await apiLogin(request, "sales")
    const sku = await discoverSellableSku(request, salesToken)

    const ts = Date.now()
    const legalName = `E2E变更客户${ts}`
    const shortName = `E2EBG${String(ts).slice(-6)}`
    const creditCode = `91${String(ts).slice(-16).padStart(16, "0")}`.slice(0, 18)
    const contractNo = `HT-E2E-${ts}`

    // ---------- 步骤 1：销售登录并创建客户 ----------
    await switchAccount("sales")
    await gotoPage(page, "/sales/customers")
    // 页面操作区与空态各有一个「新建客户」，二者等价，取第一个
    await page.getByRole("button", { name: "新建客户" }).first().click()
    const customerDialog = lastDialog(page)
    await expect(customerDialog).toBeVisible({ timeout: 20_000 })
    await customerDialog.getByLabel("法定名称").fill(legalName)
    await customerDialog.getByLabel("客户简称").fill(shortName)
    await customerDialog.getByLabel("统一社会信用代码").fill(creditCode)
    await customerDialog.getByRole("button", { name: "创建客户" }).click()
    await expect(customerDialog).not.toBeVisible({ timeout: 20_000 })
    // 创建成功自动进入客户详情（创建时自动建立 OWNER 归属，销售可检索该客户）
    await expect(page).toHaveURL(/\/sales\/customers\//, { timeout: 20_000 })
    await expect(page.getByText(legalName).first()).toBeVisible({ timeout: 20_000 })

    // ---------- 步骤 2：新建销售单（合同经上传对话框原地归档） ----------
    await gotoPage(page, "/sales/orders?mode=create")

    // 无已有合同：点加号打开 ContractUploadDialog
    await page.getByRole("button", { name: "上传合同 PDF" }).click()
    const uploadDialog = lastDialog(page)
    await expect(uploadDialog).toBeVisible({ timeout: 20_000 })
    await uploadDialog
        .getByLabel("上传合同 PDF")
        .setInputFiles(path.join(__dirname, "..", "fixtures", "sample-contract.pdf"))
    await uploadDialog.getByLabel("合同编号").fill(contractNo)
    await selectComboboxOption(page, uploadDialog, "客户", shortName, legalName)
    await selectComboboxOption(page, uploadDialog, "结算主体", legalName, legalName)
    await uploadDialog.getByRole("button", { name: "上传并归档" }).click()
    await expect(uploadDialog).not.toBeVisible({ timeout: 20_000 })
    // 上传成功后自动选中新合同并带出合同版本/客户/结算主体
    await expect(
        page.getByText(new RegExp(contractNo.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))).first(),
    ).toBeVisible({ timeout: 20_000 })

    // 单据头：福利场景（付款条件由合同快照带出）
    await selectComboboxOption(page, page.locator("body"), "福利场景", "年节礼包", "年节礼包")
    const headerSection = page.locator("section").filter({ hasText: "单据头" }).first()
    await pickDateInScope(page, headerSection)

    // 销售明细：选择公司商品池 SKU，填数量/含税单价/交付日期
    await selectComboboxOption(page, page.locator("body"), "商品", sku.skuNo, sku.skuNo)
    await page.getByLabel("数量").fill("2")
    await page.getByLabel("含税单价").fill("100.00")
    const lineSection = page.locator("#sales-line-items-section")
    await pickDateInScope(page, lineSection)

    // 提交 → 确认提交销售单
    await page.getByRole("button", { name: "提交", exact: true }).click()
    const submitOrderDialog = lastDialog(page)
    await expect(submitOrderDialog).toBeVisible({ timeout: 20_000 })
    await expect(submitOrderDialog.getByText("确认提交销售单")).toBeVisible({ timeout: 10_000 })
    await confirmInDialog(page, "确认提交")

    // 落单详情页并断言「审批中」
    await page.waitForURL(/\/sales\/orders\/[^/]+/, { timeout: 20_000 })
    const salesOrderId = new URL(page.url()).pathname.split("/").pop()!
    expect(salesOrderId).toBeTruthy()
    await expect(page.getByText("审批中").first()).toBeVisible({ timeout: 20_000 })

    // ---------- 步骤 3：采购确认节点通过 → 销售单生效（不履约） ----------
    await switchAccount("procurement")
    await approveFirstWorkspaceTask(caigouPage)

    await switchAccount("sales")
    await gotoPage(page, `/sales/orders/${salesOrderId}`)
    await expect(page.getByText("已生效").first()).toBeVisible({ timeout: 20_000 })
    await expect(page.getByRole("button", { name: "发起改单" })).toBeEnabled({ timeout: 20_000 })

    // ---------- 步骤 4：销售创建销售变更单（发起改单 → 改单草稿） ----------
    await page.getByRole("button", { name: "发起改单" }).click()
    const startChangeDialog = lastDialog(page)
    await expect(startChangeDialog).toBeVisible({ timeout: 20_000 })
    await expect(startChangeDialog.getByText("发起改单")).toBeVisible({ timeout: 10_000 })
    await confirmInDialog(page, "确认创建")
    // 改单草稿已生成：结果卡 + 版本页签出现「改单中」徽标
    await expect(page.getByText("改单已创建").first()).toBeVisible({ timeout: 20_000 })
    await expect(
        page.getByRole("tab", { name: /版本/ }).getByText("改单中"),
    ).toBeVisible({ timeout: 20_000 })
    // 聚焦条：生效的实物与服务订单优先提示「可以做客户验收」（验收随时可登记），
    // 改单提醒体现为版本页签「改单中」徽标 + 「发起改单」禁用（阻塞原因 tooltip）
    await expect(page.getByText("可以做客户验收").first()).toBeVisible({ timeout: 20_000 })
    await expect(page.getByRole("button", { name: "发起改单" })).toBeDisabled()
    await expect(
        page.getByRole("button", { name: "发起改单" }),
    ).toHaveAttribute("title", /已有一笔改单在处理中/)

    // ---------- 步骤 5：提交改单 → 审批中 ----------
    await page.getByRole("tab", { name: /版本/ }).click()
    await expect(page.getByRole("button", { name: "提交改单" })).toBeVisible({ timeout: 20_000 })
    await page.getByRole("button", { name: "提交改单" }).click()
    const submitChangeDialog = lastDialog(page)
    await expect(submitChangeDialog).toBeVisible({ timeout: 20_000 })
    await expect(submitChangeDialog.getByText("确认提交改单")).toBeVisible({ timeout: 10_000 })
    await confirmInDialog(page, "确认提交")
    await expect(page.getByText("改单已提交审批").first()).toBeVisible({ timeout: 20_000 })

    // ---------- 步骤 6：采购确认节点（caigou）→ 销售领导复核节点（lisiyong） ----------
    await switchAccount("procurement")
    await approveFirstWorkspaceTask(caigouPage)

    await switchAccount("salesLeader")
    await approveFirstWorkspaceTask(leaderPage)

    // ---------- 步骤 7：变更生效 → 销售单追加 v2，明细/金额按改单工作副本更新 ----------
    await switchAccount("sales")
    await gotoPage(page, `/sales/orders/${salesOrderId}`)
    await expect(page.getByText("已生效").first()).toBeVisible({ timeout: 20_000 })

    // 版本页签：v2 为当前在用并标注改单来源（销售变更单），v1 保留为历史（审批生效）
    await page.getByRole("tab", { name: /版本/ }).click()
    await expect(page.getByText("当前 v2").first()).toBeVisible({ timeout: 20_000 })
    await expect(page.getByText("销售变更单").first()).toBeVisible({ timeout: 20_000 })
    await expect(page.getByText("审批生效").first()).toBeVisible({ timeout: 20_000 })
    await expect(page.getByText("当前在用").first()).toBeVisible({ timeout: 20_000 })
    await expect(page.getByText("历史").first()).toBeVisible({ timeout: 20_000 })
    await expect(
        page.locator('ol[aria-label="销售版本时间线"] > li'),
    ).toHaveCount(2)
    // 改单已生效，版本页签不再有「改单中」徽标
    await expect(page.getByRole("tab", { name: /版本/ }).getByText("改单中")).toHaveCount(0)

    // 概览：当前版本 v2，金额/明细与改单工作副本一致（改单草稿原样复制工作副本）
    await page.getByRole("tab", { name: /概览/ }).click()
    await expect(page.getByText("当前版本").first()).toBeVisible({ timeout: 20_000 })
    await expect(
        page.locator('dl[aria-label="销售单金额摘要"]').getByText(/200\.00/).first(),
    ).toBeVisible({ timeout: 20_000 })
    await expect(page.getByText(sku.name).first()).toBeVisible({ timeout: 20_000 })

    // 服务端事实：销售变更单终态为已生效（列表接口只读核对）
    const changes = await api<{ items: Array<{ id: string; status: string }> }>(
        request,
        "GET",
        "/admin/sales-change-orders",
        { token: salesToken, query: { sales_order_id: salesOrderId, page: 1, page_size: 10 } },
    )
    const active = changes.items.find((item) => item.id)
    expect(active?.status).toBe("EFFECTIVE")
})
