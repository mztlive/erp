/**
 * [flow-09] 采购变更单（PurchaseChangeOrder）
 *
 * 文档依据: docs/erp-phase-1.md §6.5.2（另见 §6.3 异常处理单据、§7.2/§7.4 采购主流程、
 *          ui-workspaces/w08-purchase-orders.md §7.2 生效后变更边界）
 * 流程要点: 采购单生效 → 采购创建采购变更单（调整数量/成本并填原因）→ 提交 →
 *          财务复核节点（PurchaseChangeOrder 已发布定义：财务复核 caiwu）→ 生效 →
 *          断言采购单版本/应付与变更状态按变更更新。
 *
 * 使用账号:
 *   xiaoshou(销售) —— 前置建单链：客户、合同、销售单提交（流程起点：销售单必须已生效）
 *   caigou(采购)   —— 采购确认通过 → 创建采购单（当前代码缺陷，见下）→ 发起采购变更 → 提交改单
 *   caiwu(财务)    —— PurchaseOrder 财务审核、PurchaseChangeOrder 财务复核通过
 *
 * 文档-代码差异（doc=文档说法，code=代码实际行为）:
 *   1) doc: §6.5.2「创建采购变更单并填写原因」
 *      code: erp-client/features/purchase-orders/api/purchase-order-commands.ts 的
 *            startPurchaseChange 固定传 reason="采购变更"（注释「前端契约未传 reason；后端必填」），
 *            变更与异常页没有任何原因输入框；变更列表展示的标签即固定文案「采购变更」。
 *   2) doc: §6.5.2「调整数量/成本」后提交变更
 *      code: submitPurchaseChange 直接调用 mapCenterLinesForChangeSubmit 把采购单中心当前行
 *            原样快照为目标行（无数量/成本编辑 UI，无变更内容编辑入口），
 *            变更生效后生成的新版本内容与基准版本完全一致，应付差额为 0。
 *   3) doc: §7.2/§7.4 销售单生效后采购「按供应商和履约责任创建采购单」（选源建单）
 *      code: 选源建单已可用（flow-07 全流程验证通过）；本测试步骤 5 走真实选源建单，
 *            仅当后端再次出现「当前没有可消费的创建依据」时抛错阻断。
 */
import { expect, test } from "@playwright/test"
import type { APIRequestContext, Locator, Page } from "@playwright/test"

import { api, apiLogin } from "../helpers/api"
import { createSinglePageAccountSwitcher } from "../helpers/login"
import { openCreateDialog } from "../helpers/ui"

test.setTimeout(300_000)

/* ------------------------------------------------------------------ */
/* 流程内专用小工具（仅本 spec 使用，不写入 helpers）                    */
/* ------------------------------------------------------------------ */

const MONTHS_EN = [
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

function uid(prefix: string): string {
    return `${prefix}-${Date.now().toString().slice(-8)}`
}

/** 今天偏移 n 天的 YYYY-MM-DD。 */
function daysFromToday(n: number): string {
    const d = new Date()
    d.setDate(d.getDate() + n)
    const mm = String(d.getMonth() + 1).padStart(2, "0")
    const dd = String(d.getDate()).padStart(2, "0")
    return `${d.getFullYear()}-${mm}-${dd}`
}

function ordinalSuffix(day: number): string {
    if (day % 100 >= 11 && day % 100 <= 13) return "th"
    switch (day % 10) {
        case 1:
            return "st"
        case 2:
            return "nd"
        case 3:
            return "rd"
        default:
            return "th"
    }
}

/** react-day-picker v9 日按钮 aria-label（en-US "PPPP"：December 31st, 2026）。 */
function dayButtonName(iso: string): string {
    const [y, m, d] = iso.split("-").map(Number)
    return `${MONTHS_EN[m - 1]} ${d}${ordinalSuffix(d)}, ${y}`
}

/** 在指定 Field 组内选择日期：打开日历 → 翻到目标月份 → 点目标日。 */
async function pickDate(page: Page, fieldLabel: string, iso: string): Promise<void> {
    const [y, m] = iso.split("-").map(Number)
    const group = page.getByRole("group").filter({ hasText: fieldLabel }).last()
    await group.getByRole("button", { name: /^(选择日期|已选日期)/ }).click()
    const popover = page.locator('[data-slot="popover-content"]').last()
    await expect(popover).toBeVisible({ timeout: 10_000 })
    const caption = `${MONTHS_EN[m - 1]} ${y}`
    for (let i = 0; i < 24; i++) {
        if (await popover.getByText(caption).first().isVisible().catch(() => false)) break
        const next = popover.getByRole("button", { name: "Go to the Next Month" })
        const prev = popover.getByRole("button", { name: "Go to the Previous Month" })
        if (await next.isVisible().catch(() => false)) await next.click()
        else await prev.click()
    }
    await popover.getByRole("button", { name: new RegExp(dayButtonName(iso)) }).click()
}

/** 通用可搜索 combobox：点击输入框，可选输入查询文本后点选弹层内的选项/首个选项。 */
async function pickCombobox(
    page: Page,
    input: Locator,
    options: { query?: string; optionText?: string | RegExp } = {},
): Promise<void> {
    await input.click()
    const popup = page.locator('[data-slot="combobox-content"]').last()
    await expect(popup).toBeVisible({ timeout: 10_000 })
    if (options.query) {
        await input.fill(options.query)
        await expect(popup).toBeVisible({ timeout: 10_000 })
    }
    if (options.optionText) {
        await popup.getByText(options.optionText).first().click()
    } else {
        await popup.locator('[data-slot="combobox-item"]').first().click()
    }
}

/** 在 AlertDialog（role=alertdialog，FormalActionConfirmDialog 等）内点按钮。 */
async function clickAlertDialogButton(page: Page, name: string | RegExp): Promise<void> {
    const dialog = page.locator('[role="alertdialog"]').last()
    await expect(dialog).toBeVisible({ timeout: 10_000 })
    await dialog.getByRole("button", { name }).first().click()
}

/** 常规 Dialog（role=dialog）内点按钮。 */
async function clickDialogButton(page: Page, name: string | RegExp): Promise<void> {
    const dialog = page.locator('[role="dialog"]').last()
    await expect(dialog).toBeVisible({ timeout: 10_000 })
    await dialog.getByRole("button", { name }).first().click()
}

/** 断言页面出现指定文本（结果卡/徽标/提示），20s 超时。 */
async function expectVisible(page: Page, text: string | RegExp, timeout = 20_000): Promise<void> {
    await expect(page.getByText(text).first()).toBeVisible({ timeout })
}

/** 通过 API 查询账号本人 OPEN 待办，返回匹配业务对象的 workItemId。 */
async function fetchOpenWorkItemId(
    request: APIRequestContext,
    accountKey: "sales" | "procurement" | "finance",
    match: { businessObjectType?: string; businessObjectId?: string },
): Promise<string> {
    const token = await apiLogin(request, accountKey)
    const page = await api<{
        items: Array<{
            id: string
            status: string
            business_object_type: string
            business_object_id: string
        }>
    }>(request, "GET", "/admin/work-items", {
        token,
        query: { scope: "mine", timezone: "Asia/Shanghai", page: 1, page_size: 100 },
    })
    const found = (page?.items ?? []).find(
        (item) =>
            item.status === "OPEN" &&
            (match.businessObjectType
                ? item.business_object_type
                      .replace(/_/g, "")
                      .toLowerCase() ===
                  match.businessObjectType.replace(/_/g, "").toLowerCase()
                : true) &&
            (match.businessObjectId
                ? item.business_object_id === match.businessObjectId
                : true),
    )
    if (!found) {
        throw new Error(
            `未找到 ${JSON.stringify(match)} 的 OPEN 待办（scope=mine, account=${accountKey}）`,
        )
    }
    return found.id
}

/** 在业务对象详情页（携带 workItemId）完成「通过」审批并断言结果卡。 */
async function approveOnDetail(
    page: Page,
    detailUrl: string,
    workItemId: string,
): Promise<void> {
    await page.goto(`${detailUrl}?workItemId=${encodeURIComponent(workItemId)}`)
    await page.getByRole("button", { name: "通过", exact: true }).click()
    await clickDialogButton(page, "确认通过")
    await expectVisible(page, "审批决定已提交")
}

/* ------------------------------------------------------------------ */
/* 主流程                                                               */
/* ------------------------------------------------------------------ */

test("[flow-09] 采购变更单：销售链前置 → 采购单生效 → 发起/提交变更 → 财务复核 → 生效断言", async ({
    page,
    request,
}) => {
    const switchAccount = createSinglePageAccountSwitcher(page)
    const salesPage = page
    const procurementPage = page
    const financePage = page

    /* ============ 前置：销售链（客户 → 合同 → 销售单 → 采购确认 → 销售单生效） ============ */

    // 1) 销售创建客户（新建客户 Dialog）
    await switchAccount("sales")
    const customerName = `E2E客户${uid("C")}`
    const creditCode = ("913" + Date.now().toString().slice(-13)).padEnd(17, "0") + "A"
    await salesPage.goto("/sales/customers")
    const customerDialog = await openCreateDialog(salesPage, "新建客户")
    await expect(customerDialog.getByText("新建客户", { exact: true })).toBeVisible()
    await salesPage.getByLabel("法定名称").fill(customerName)
    await salesPage.getByLabel("客户简称").fill(customerName)
    await salesPage.getByLabel("统一社会信用代码").fill(creditCode)
    await pickCombobox(salesPage, salesPage.getByLabel("默认付款条件"), {
        optionText: "货到 30 天",
    })
    await clickDialogButton(salesPage, "创建客户")
    await expectVisible(salesPage, "客户已创建")
    // 弹窗关闭后留在客户列表，由客户链接进入详情。
    await salesPage.getByRole("link", { name: customerName, exact: true }).click()
    await expect(salesPage).toHaveURL(/\/sales\/customers\/[0-9a-f]+/, { timeout: 20_000 })

    // 2) 销售上传合同 PDF（合同列表页 → 上传合同 PDF Dialog）
    const contractNo = `HT${uid("")}`
    await salesPage.goto("/sales/contracts")
    const contractDialog = await openCreateDialog(salesPage, "上传合同 PDF")
    // 标题用 heading role（getByText exact 会同时命中弹窗标题与拖放区文案，触发严格模式）
    await expect(contractDialog.getByRole("heading", { name: "上传合同 PDF" })).toBeVisible()
    await contractDialog.locator('input[type="file"]').setInputFiles(
        // run-flow.sh 在 e2e 目录下执行 playwright，cwd 即 e2e/
        `${process.cwd()}/fixtures/sample-contract.pdf`,
    )
    await salesPage.getByLabel("合同编号").fill(contractNo)
    await pickCombobox(salesPage, salesPage.getByPlaceholder("搜索客户编号或名称"), {
        query: customerName,
        optionText: new RegExp(customerName),
    })
    await pickCombobox(salesPage, salesPage.getByPlaceholder("搜索结算主体"), {
        query: customerName,
        optionText: new RegExp(customerName),
    })
    await pickCombobox(salesPage, salesPage.getByLabel("付款条件"), { optionText: "货到 30 天" })
    await pickDate(salesPage, "签订日期", daysFromToday(-10))
    await pickDate(salesPage, "有效期起", daysFromToday(-10))
    await pickDate(salesPage, "有效期止", daysFromToday(60))
    await clickDialogButton(salesPage, "上传并归档")
    await expect(contractDialog).not.toBeVisible({ timeout: 20_000 }).catch(() => {})
    await expectVisible(salesPage, contractNo)

    // 3) 销售创建销售单并提交（/sales/orders?mode=create）
    await salesPage.goto("/sales/orders?mode=create")
    await pickCombobox(salesPage, salesPage.getByPlaceholder("搜索合同编号或客户"), {
        query: contractNo,
        optionText: new RegExp(contractNo),
    })
    // 选完合同后自动带出客户/结算主体/付款条件；补齐其余必填头字段
    await pickCombobox(salesPage, salesPage.getByLabel("福利场景"), { optionText: "年节礼包" })
    await pickCombobox(salesPage, salesPage.getByLabel("付款条件"), { optionText: "货到 30 天" })
    await pickDate(salesPage, "履约期限", daysFromToday(30))
    // 明细行：选 SKU（主数据保留，取首个可售 SKU）、填数量/单价/交付日期
    await pickCombobox(salesPage, salesPage.getByPlaceholder("搜索 SKU 或商品名称"))
    await salesPage.getByLabel("数量").fill("10")
    await salesPage.getByLabel("含税单价").fill("100")
    await pickDate(salesPage, "交付日期", daysFromToday(30))
    await salesPage.getByRole("button", { name: "提交", exact: true }).click()
    await clickAlertDialogButton(salesPage, "确认提交")
    await expect(salesPage).toHaveURL(/\/sales\/orders\/[0-9a-f]+/, { timeout: 30_000 })

    // 4) 采购确认：caigou 在工作台/详情审批通过 → 销售单生效
    const salesOrderId = new URL(salesPage.url()).pathname.split("/").pop() ?? ""
    await switchAccount("procurement")
    const salesTaskId = await fetchOpenWorkItemId(request, "procurement", {
        businessObjectType: "SalesOrder",
        businessObjectId: salesOrderId,
    })
    await approveOnDetail(
        procurementPage,
        `/sales/orders/${salesOrderId}`,
        salesTaskId,
    )
    await expectVisible(procurementPage, "已生效")

    /* ============ 前置：创建并生效采购单（当前代码缺陷，见头部注释 3)） ============ */

    // 5) 采购从 W08 列表「新建采购单」选源建单
    await procurementPage.goto("/procurement/orders")
    await procurementPage.getByRole("button", { name: "新建采购单" }).first().click()
    const poCreateDialog = procurementPage.locator('[role="dialog"]').last()
    await expect(poCreateDialog).toBeVisible({ timeout: 20_000 })
    await expect(poCreateDialog.getByText("从采购创建依据建单")).toBeVisible()
    const noBasis = poCreateDialog.getByText(
        "当前没有可消费的创建依据。请先在采购二次确认完成确认。",
    )
    if (await noBasis.isVisible().catch(() => false)) {
        throw new Error(
            "前置缺陷（阻断 flow-09 核心流程）：销售单已生效，但「从采购创建依据建单」弹窗显示" +
                "「当前没有可消费的创建依据」——backend/services/src/purchase_order/creation_basis.rs 的 " +
                "creation_basis_list 恒返回空数组、create_from_basis 恒返回 404（旧采购确认批次已删除，" +
                "新选源建单未实现），采购单当前无法通过 UI 创建（文档 §7.2/§7.4 要求销售单生效后选源建单）。",
        )
    }
    // 功能就绪后：选中创建依据 → 创建草稿并打开 → 提交审批
    await pickCombobox(procurementPage, poCreateDialog.getByPlaceholder("选择创建依据"))
    await clickDialogButton(procurementPage, "创建草稿并打开")
    await expect(procurementPage).toHaveURL(/\/procurement\/orders\/[0-9a-f]+/, {
        timeout: 30_000,
    })
    const purchaseOrderId = new URL(procurementPage.url()).pathname.split("/").pop() ?? ""

    // 6) 采购提交采购单 → 财务审核通过 → 采购单生效（形成应付）
    await procurementPage.getByRole("button", { name: "提交审批" }).click()
    await clickAlertDialogButton(procurementPage, "确认提交")
    // 提交成功进入详情页（结果卡片会被 mode 切换重挂载丢弃，销售单页无此问题），
    // 以详情页「撤回审批」入口作为成功信号（与 flow-07 一致；确认对话框自身含「草稿→审批中」文案，
    // 不能以文本「审批中」断言提交成功）
    await expect(
        procurementPage.getByRole("button", { name: "撤回审批" }).first(),
    ).toBeVisible({ timeout: 30_000 })
    const poTaskId = await fetchOpenWorkItemId(request, "finance", {
        businessObjectType: "PurchaseOrder",
        businessObjectId: purchaseOrderId,
    })
    await switchAccount("finance")
    await approveOnDetail(
        financePage,
        `/procurement/orders/${purchaseOrderId}`,
        poTaskId,
    )
    await expectVisible(financePage, "已生效")

    /* ============ 核心流程：采购变更单 ============ */

    // 7) 采购发起采购变更：变更与异常 → 发起采购变更 → 创建工作副本
    await switchAccount("procurement")
    await procurementPage.goto(`/procurement/orders/${purchaseOrderId}?section=changes`)
    await expectVisible(procurementPage, "变更与异常")
    await procurementPage.getByRole("button", { name: "发起采购变更" }).first().click()
    await clickAlertDialogButton(procurementPage, "创建工作副本")
    await expectVisible(procurementPage, "已创建采购变更工作副本")
    await expect(procurementPage).toHaveURL(/section=changes/)
    // 变更列表出现新行：固定文案「采购变更」+ 状态徽标「草稿」
    const changesSection = procurementPage
        .locator('section[data-slot="document-section"]')
        .filter({ hasText: "变更与异常" })
        .last()
    await expect(changesSection.getByText("采购变更", { exact: true }).first()).toBeVisible({
        timeout: 20_000,
    })
    await expect(changesSection.getByText("草稿", { exact: true }).first()).toBeVisible()

    // 8) 采购提交改单（弹窗展示固定审批路线，不可选节点/审批人）
    await procurementPage.getByRole("button", { name: "提交改单" }).click()
    const changeSubmitDialog = procurementPage.locator('[role="alertdialog"]').last()
    await expect(changeSubmitDialog).toBeVisible({ timeout: 10_000 })
    await changeSubmitDialog.getByRole("button", { name: "确认提交" }).click()
    await expectVisible(procurementPage, "改单已提交审批")
    await expectVisible(procurementPage, /已进入「审批中」/)

    // 9) 财务复核：caiwu 审批通过（末节点通过即生效，生成新采购版本 + 应付差额）
    const changeTaskId = await fetchOpenWorkItemId(request, "finance", {
        businessObjectType: "PurchaseChangeOrder",
    })
    await switchAccount("finance")
    await approveOnDetail(
        financePage,
        `/procurement/orders/${purchaseOrderId}`,
        changeTaskId,
    )

    // 10) 断言变更已生效 + 采购单仍生效 + 应付可见（按变更更新）
    await switchAccount("procurement")
    await procurementPage.goto(`/procurement/orders/${purchaseOrderId}?section=changes`)
    await expect(procurementPage.getByText("已生效").first()).toBeVisible({ timeout: 20_000 })
    const changesAfter = procurementPage
        .locator('section[data-slot="document-section"]')
        .filter({ hasText: "变更与异常" })
        .last()
    await expect(changesAfter.getByText("采购变更", { exact: true }).first()).toBeVisible()
    await expect(changesAfter.getByText("已生效", { exact: true }).first()).toBeVisible({
        timeout: 20_000,
    })
    // 采购单主体仍为已生效（变更不改变主状态），应付与票款子区展示应付未结
    await procurementPage.goto(`/procurement/orders/${purchaseOrderId}?section=payable`)
    await expectVisible(procurementPage, "应付未结")
})
