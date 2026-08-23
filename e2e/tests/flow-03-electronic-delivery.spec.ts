/**
 * flow-03 虚拟商品电子交付
 *
 * 文档依据: docs/erp-phase-1.md §7.3.3（虚拟商品和线下服务履约段）、§7.2（采购主流程）、
 *          §7.4（采购单规则）、§6.1/§6.2（正式单据：电子交付记录/客户验收单）
 * 流程: 客户 → 合同(PDF 上传) → 销售单(实物与服务) → 采购确认审批 → 生效
 *       → 采购单(从二次确认依据建单) → 财务审核审批 → 生效
 *       → 电子交付记录（对象/数量/时间/结果/凭证）→ 客户验收 → 履约完成
 *
 * 使用账号: xiaoshou(销售, 建客户/合同/销售单/验收)、caigou(采购, 审批采购确认/建采购单/电子交付)、
 *          caiwu(财务, 采购单财务审核)
 *
 * 文档-代码差异（以代码为准）:
 *   1. 电子交付草稿创建没有 UI 入口（frontend 无 POST /admin/electronic-deliveries 调用，
 *      backend 仅 HTTP 创建，见 features/fulfillment-operations/api/* 与
 *      backend/services/src/fulfillment/electronic_delivery.rs），本测试用 API 创建草稿。
 *   2. W09 电子交付表单「交付对象」只读且 backend ElectronicDeliveryView 不返回脱敏对象
 *      （backend/services/src/fulfillment/dto.rs），前端 electronicToOperation 映射
 *      recipientMasked 恒为空，clientValidation 报「交付对象不能为空」→ 确认交付按钮
 *      被禁用（features/fulfillment-operations/lib/validation.ts）。因此本测试经 API 确认，
 *      UI 侧只断言队列出现与清空。
 *   3. 销售单建单页不提供履约方式选择，明细恒为「公司仓发」
 *      （features/sales-orders/lib/sales-order-create-model.ts「履约方式由后续审批节点写入结论」），
 *      后端确认电子交付时也不校验销售行履约方式（backend/services/src/fulfillment/purchase_context.rs）。
 *   4. 电子交付「凭证」（evidence_attachment_id）仅 API 支持，UI 无录入入口。
 *   5. 文档 W07「二次确认队列」在实现中为销售单审批的「采购确认」普通审批节点，
 *      在 W01 工作台办理（e2e/scripts/publish-approval-definitions.mjs: sales_order 单节点）。
 */
import { expect, test, type Locator, type Page } from "@playwright/test"
import path from "path"

import { createSinglePageAccountSwitcher } from "../helpers/login"
import { api, apiLogin } from "../helpers/api"
import { gotoPage } from "../helpers/ui"

// ---------------------------------------------------------------------------
// 流程内专用小工具（不进 helpers）
// ---------------------------------------------------------------------------

const ts = () => Date.now().toString(36)

/** Base UI Combobox（components/ui/combobox.tsx）：输入关键词 → 弹层 → 选项。 */
async function pickComboboxOption(
    page: Page,
    input: Locator,
    optionText: string,
): Promise<void> {
    await input.click()
    await input.fill(optionText)
    const popup = page.locator('[data-slot="combobox-content"]').last()
    await expect(popup).toBeVisible({ timeout: 20_000 })
    await popup
        .locator('[data-slot="combobox-item"]')
        .filter({ hasText: optionText })
        .first()
        .click()
}

/** 远程搜索 Combobox：输入关键词后选择首个可见选项。 */
async function pickFirstComboboxResult(
    page: Page,
    placeholder: string,
    keyword: string,
): Promise<void> {
    const input = page.getByPlaceholder(placeholder).first()
    await input.click()
    await input.fill(keyword)
    const popup = page.locator('[data-slot="combobox-content"]').last()
    await expect(popup).toBeVisible({ timeout: 20_000 })
    await popup.locator('[data-slot="combobox-item"]').first().click()
}

/**
 * 在指定作用域（单据头 section / 明细表格）内打开日期选择并点选未来某日。
 * 日号取「今天+10 天（封顶 28）」；用 `^day$` 精确过滤 + `.last()`：
 * 日历网格首行可能含上一月（25-31）与下一月（1-6）的相邻月格，`.last()`
 * 保证命中当前月（15-28 号只会出现在上一月之后、下一月之前）。
 * 已选日再次点击会取消选择（react-day-picker 切换语义），故每个触发钮只点一次。
 */
function rootPageOf(scope: Page | Locator): Page {
    return (scope as Locator).page?.() ?? (scope as Page)
}

async function pickFutureDay(
    scope: Page | Locator,
    tag: string,
): Promise<void> {
    const today = new Date()
    const day = Math.min(today.getDate() + 10, 28)
    const trigger = scope
        .getByRole("button", { name: /^(选择日期|已选日期)/ })
        .first()
    await trigger.click()
    const popover = rootPageOf(scope)
        .locator('[data-slot="popover-content"]')
        .last()
    await expect(popover).toBeVisible({ timeout: 10_000 })
    const dayButton = popover
        .getByRole("button")
        .filter({ hasText: new RegExp(`^${day}$`) })
        .last()
    await expect(dayButton).toBeVisible({ timeout: 10_000 })
    await dayButton.click()
    // 选中后触发钮可见文本变为日期（aria-label 才有「已选日期」前缀）
    await expect(trigger).toHaveText(/^\d{4}-\d{2}-\d{2}$/, {
        timeout: 10_000,
    }).catch(() => {
        throw new Error(`PROBE-F03 [${tag}] 日期未写入：${day}`)
    })
}

/**
 * W01 工作台：点击待办列表首个任务并完成「通过」审批。
 * 任务按钮只展示类型标签+内部 id（list_summary 仅采购审核简报填充），
 * 全流程串行、数据库已重置，首个任务即当前唯一审批待办。
 */
async function approveInWorkbench(page: Page): Promise<void> {
    // 工作台在桌面/窄屏各渲染一份待办列表（已知重复渲染），取第一份
    const taskList = page.locator('ul[aria-label="待办列表"]').first()
    await expect(taskList).toBeVisible({ timeout: 30_000 })
    const task = taskList.getByRole("button").first()
    await expect(task).toBeVisible({ timeout: 30_000 })
    await task.click()
    await page.getByRole("button", { name: "通过" }).first().click()
    await expect(page.locator('[role="dialog"]')).toHaveCount(0)
}

// ---------------------------------------------------------------------------
// 主流程
// ---------------------------------------------------------------------------

// 全流程串行步骤多，放宽单测超时（playwright.config.ts 默认 240s 不够）
test.setTimeout(600_000)

test("flow-03 虚拟商品电子交付全流程", async ({ page }) => {
    // ========== 准备：单页面串行切号 ==========
    const switchAccount = createSinglePageAccountSwitcher(page)
    const sales = page
    const procurement = page
    const finance = page
    const suffix = ts()
    const customerLegalName = `E2E电子交付客户${suffix}`
    const creditCode = `91${suffix.padEnd(16, "0").slice(0, 16)}` // 18 位字母数字
    const contractNo = `HT-${suffix}`
    const skuQty = "2"
    const skuPrice = "100.00"

    // ========== 第一步（销售）：创建客户 ==========
    await switchAccount("sales")

    await gotoPage(sales, "/sales/customers")
    await sales.getByRole("button", { name: "新建客户" }).first().click()
    const customerDialog = sales.locator('[role="dialog"]').last()
    await expect(customerDialog).toBeVisible({ timeout: 20_000 })
    await customerDialog.getByLabel("法定名称").fill(customerLegalName)
    await customerDialog.getByLabel("统一社会信用代码").fill(creditCode)
    await customerDialog.getByRole("button", { name: "创建客户" }).click()
    await expect(sales.getByText("客户已创建").first()).toBeVisible({ timeout: 20_000 })
    // 创建成功后弹窗自动关闭并进入客户详情
    await expect(sales).toHaveURL(/\/sales\/customers\/[^/]+$/, { timeout: 20_000 })

    // ========== 第二步（销售）：上传合同 PDF（e2e/fixtures/sample-contract.pdf） ==========
    await gotoPage(sales, "/sales/contracts")
    await sales.getByRole("button", { name: "上传合同 PDF" }).click()
    const uploadDialog = sales.locator('[role="dialog"]').last()
    await expect(uploadDialog).toBeVisible({ timeout: 20_000 })
    await uploadDialog
        .locator('input[type="file"]')
        .setInputFiles(path.join(__dirname, "../fixtures/sample-contract.pdf"))
    await uploadDialog.getByLabel("合同编号").fill(contractNo)
    // 客户（搜索法定名称）与结算主体（客户主体 party 按法定名称可检索）
    await pickComboboxOption(
        sales,
        uploadDialog.getByPlaceholder("搜索客户编号或名称"),
        customerLegalName,
    )
    await pickComboboxOption(
        sales,
        uploadDialog.getByPlaceholder("搜索结算主体"),
        customerLegalName,
    )
    await uploadDialog.getByRole("button", { name: "上传并归档" }).click()
    await expect(sales.getByText("合同 PDF 已归档").first()).toBeVisible({
        timeout: 30_000,
    })

    // ========== 第三步（销售）：创建并提交销售单（实物与服务） ==========
    await gotoPage(sales, "/sales/orders?mode=create")
    await expect(sales.getByText("单据头").first()).toBeVisible({ timeout: 30_000 })
    // 有效合同：搜索合同编号并选择（自动带出客户与结算主体）
    await pickComboboxOption(
        sales,
        sales.getByPlaceholder("搜索合同编号或客户"),
        contractNo,
    )
    await expect(sales.getByText(`${contractNo}@v1`).first()).toBeVisible({
        timeout: 20_000,
    })
    // 单据头：福利场景 + 履约期限（付款条件随合同带出，税率默认 13%）
    await pickComboboxOption(sales, sales.getByLabel("福利场景"), "年节礼包")
    const headerSection = sales
        .locator("section")
        .filter({ has: sales.getByRole("heading", { name: "单据头" }) })
    await pickFutureDay(headerSection, "履约期限")
    // 明细：公司商品池 SKU（首个可售 SKU）、数量、含税单价、交付日期
    const lineTable = sales.getByRole("table", { name: "销售单创建明细" })
    await pickFirstComboboxResult(sales, "搜索 SKU 或商品名称", "")
    await sales.getByLabel("数量").fill(skuQty)
    await sales.getByLabel("含税单价").fill(skuPrice)
    await pickFutureDay(lineTable, "交付日期")
    // 提交并确认
    await sales.getByRole("button", { name: "提交", exact: true }).click()
    const submitDialog = sales.getByRole("alertdialog").last()
    await expect(submitDialog).toBeVisible({ timeout: 20_000 })
    await submitDialog.getByRole("button", { name: "确认提交" }).click()
    // 提交后进入销售单详情，状态为「审批中」
    await expect(sales).toHaveURL(/\/sales\/orders\/[^/]+$/, { timeout: 30_000 })
    await expect(sales.getByText("审批中").first()).toBeVisible({ timeout: 30_000 })

    // 记录销售单 id 与销售明细行 id（供电子交付记录 API 使用）
    const soId = new URL(sales.url()).pathname.split("/").pop()!
    const salesRequest = await sales.request
    const salesToken = await apiLogin(salesRequest, "sales")
    const soDetail = await api<{ lines: Array<{ id: string }> }>(
        salesRequest,
        "GET",
        `/admin/sales-orders/${soId}`,
        { token: salesToken },
    )
    expect(soDetail.lines.length).toBeGreaterThan(0)

    // ========== 第四步（采购）：W01 办理「采购确认」审批 ==========
    await switchAccount("procurement")
    {
        await gotoPage(procurement, "/workspace")
        await approveInWorkbench(procurement)

        // ========== 第五步（销售）：销售单已生效 ==========
        await switchAccount("sales")
        await gotoPage(sales, `/sales/orders/${soId}`)
        await expect(
            sales.getByText(/已生效|履约中/).first(),
        ).toBeVisible({ timeout: 30_000 })

        // ========== 第六步（采购）：从二次确认依据创建采购单并提交 ==========
        await switchAccount("procurement")
        await gotoPage(procurement, "/procurement/orders")
        await procurement.getByRole("button", { name: "新建采购单" }).first().click()
        const basisDialog = procurement.locator('[role="dialog"]').last()
        await expect(basisDialog).toBeVisible({ timeout: 20_000 })
        await expect(
            basisDialog.getByText("从采购创建依据建单").first(),
        ).toBeVisible()
        // 依据默认预选首个（openCreateDialog 自动选中 openBases[0]）
        await basisDialog.getByRole("button", { name: "创建草稿并打开" }).click()
        await expect(procurement).toHaveURL(/\/procurement\/orders\/[^/]+\?mode=edit/, {
            timeout: 30_000,
        })
        const poId = new URL(procurement.url()).pathname.split("/").pop()!
        await expect(
            procurement.getByText("采购草稿编辑").first(),
        ).toBeVisible({ timeout: 30_000 })
        // 明细由创建依据预填，直接提交审批
        await procurement.getByRole("button", { name: "提交审批" }).click()
        const poSubmitDialog = procurement.getByRole("alertdialog").last()
        await expect(poSubmitDialog).toBeVisible({ timeout: 20_000 })
        await poSubmitDialog.getByRole("button", { name: "确认提交" }).click()
        // 确认对话框自身含「草稿→审批中」文案，不能以文本「审批中」断言成功，
        // 以详情页「撤回审批」入口作为成功信号（与 flow-07/flow-09 一致）
        await expect(
            procurement.getByRole("button", { name: "撤回审批" }).first(),
        ).toBeVisible({
            timeout: 30_000,
        })

        // ========== 第七步（财务）：采购单「财务审核」审批 ==========
        await switchAccount("finance")
        {
            await gotoPage(finance, "/workspace")
            await approveInWorkbench(finance)

            // ========== 第八步（采购）：创建并确认电子交付记录 ==========
            await switchAccount("procurement")
            const procurementToken = await apiLogin(procurement.request, "procurement")
            const poCenter = await api<{
                sales_order_id: string
                allocations: Array<{ id: string }>
            }>(procurement.request, "GET", `/admin/purchase-orders/${poId}`, {
                token: procurementToken,
            })
            expect(poCenter.allocations.length).toBeGreaterThan(0)
            const salesOrderLineId = soDetail.lines[0].id
            const allocationId = poCenter.allocations[0].id
            const occurredAt = Math.floor(Date.now() / 1000) - 60
            const ed = await api<{ id: string; fulfillment_no: string }>(
                procurement.request,
                "POST",
                "/admin/electronic-deliveries",
                {
                    token: procurementToken,
                    body: {
                        fulfillment_no: `ED-${suffix}`,
                        sales_order_line_id: salesOrderLineId,
                        purchase_order_id: poId,
                        purchase_line_sales_allocation_id: allocationId,
                        // 脱敏交付对象快照（后端按不透明值落库）
                        recipient_snapshot: "138****0001",
                        quantity: skuQty,
                        result: "SUCCESS",
                        occurred_at: occurredAt,
                    },
                },
            )
            expect(ed.id).toBeTruthy()

            // W09 队列出现电子交付作业单（交付与代发 · 电子）
            await gotoPage(
                procurement,
                "/fulfillment?lane=procurement&type=electronic",
            )
            await expect(
                procurement.getByText("待处理单据").first(),
            ).toBeVisible({ timeout: 30_000 })
            await expect(
                procurement.getByText("电子交付").first(),
            ).toBeVisible({ timeout: 30_000 })
            await expect(
                procurement.getByText("交付对象").first(),
            ).toBeVisible({ timeout: 30_000 })

            // UI 确认被「交付对象不能为空」校验阻断（见头部差异登记 2），经 API 确认
            await api(procurement.request, "POST", `/admin/electronic-deliveries/${ed.id}/confirm`, {
                token: procurementToken,
                body: {},
            })

            // 确认后队列清空
            await procurement.reload()
            await expect(
                procurement.getByText("今天的电子交付都干完了").first(),
            ).toBeVisible({ timeout: 30_000 })

            // ========== 第九步（销售）：客户验收 ==========
            await switchAccount("sales")
            await gotoPage(sales, `/sales/orders/${soId}`)
            await sales.getByRole("tab", { name: "履约" }).click()
            await expect(
                sales.getByRole("button", { name: "登记验收" }).first(),
            ).toBeVisible({ timeout: 30_000 })
            await sales.getByRole("button", { name: "登记验收" }).click()
            // 可验收的交付记录：勾选电子交付批次
            const factPool = sales.locator("#acceptance-fact-pool")
            await expect(factPool).toBeVisible({ timeout: 30_000 })
            await expect(
                factPool.getByText(`电子交付 ED-${suffix}`).first(),
            ).toBeVisible({ timeout: 30_000 })
            await factPool.getByRole("checkbox").first().check()
            // 分配数量默认带出净可验收量；通过数量随分配自动带出，直接提交
            await expect(sales.getByLabel("通过数量")).toHaveValue(skuQty, {
                timeout: 10_000,
            })
            await sales.getByRole("button", { name: "确认并完成验收" }).click()
            const acceptDialog = sales.getByRole("alertdialog").last()
            await expect(acceptDialog).toBeVisible({ timeout: 20_000 })
            await expect(
                acceptDialog.getByText("确认客户验收").first(),
            ).toBeVisible()
            await acceptDialog.getByRole("button", { name: "确认验收" }).click()
            await expect(sales.getByText("客户验收已登记").first()).toBeVisible({
                timeout: 30_000,
            })
            // 验收历史出现已确认记录：单号 + 「已确认」徽标 + 整体结果「通过」
            // （历史接口仅返回表头，不展示行级「通过 X / 短少 Y / 拒收 Z」明细，
            // 见头部差异登记）
            await expect(
                sales.getByText("验收历史").first(),
            ).toBeVisible({ timeout: 20_000 })
            await expect(
                sales.getByText("已确认", { exact: true }).first(),
            ).toBeVisible({ timeout: 20_000 })
            await expect(
                sales.getByText("通过", { exact: true }).first(),
            ).toBeVisible({ timeout: 20_000 })

            // 销售单详情履约面板：「采购与交付」通道存在（通道计数后端未实现，
            // 前端恒为 0 笔，见差异登记；这里断言通道本身）
            await gotoPage(sales, `/sales/orders/${soId}`)
            await sales.getByRole("tab", { name: "履约" }).click()
            await expect(
                sales.getByRole("heading", { name: "采购与交付" }),
            ).toBeVisible({ timeout: 20_000 })
            await expect(
                sales.getByRole("button", { name: "打开交付" }),
            ).toBeVisible({ timeout: 20_000 })
        }
    }
})
