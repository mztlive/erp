/**
 * [flow-02] 供应商直接发客户（代发）
 *
 * 文档依据: docs/erp-phase-1.md §7.3.2（前段/后段与 §7.3.1 完全一致，仅履约段替换为代发）
 *
 * 流程:
 *   销售(客户/合同/销售单) → 采购(采购确认节点通过 → 销售单生效)
 *   → 采购(创建采购单并提交) → 财务(采购单财务审核通过 → 采购单生效)
 *   → 采购(登记代发出库单：承运方/物流单号，不经过仓库)
 *   → 销售(登记客户验收) → 断言库存台账无变化
 *
 * 使用账号:
 *   xiaoshou(销售): 客户/合同/销售单/客户验收
 *   caigou(采购):   工作台审批(采购确认) / 采购单 / 代发履约
 *   caiwu(财务):    工作台审批(采购单财务审核)
 *
 * 文档-代码差异（详见最终报告 doc_mismatches）:
 *   1. 文档 §7.4 描述"采购单创建时从供应商供给选源并记录履约责任"；
 *      代码中 W08 建单唯一入口是"从采购创建依据建单"，而后端
 *      backend/services/src/purchase_order/creation_basis.rs 已将依据接口 stub
 *      （creation_basis_list 恒返回空、create_from_basis 恒 NotFound）。
 *      若运行中的后端与源码一致，本流程将在建单步骤失败（测试会给出明确报错）。
 *   2. 文档 §4.1/数据模型约定业务编号为"前缀+YYYYMMDD+6位序号"；
 *      代码中销售单号由前端生成 XS+14位时间戳（erp-client/features/sales-orders/api/mappers.ts
 *      localOrderNo），采购单号在提交时生成 PO-<uuid>（backend/services/src/purchase_order/submission.rs
 *      assign_formal_purchase_no）。
 *   3. 销售单建单页不提供履约方式选择，实物/服务单固定提交 COMPANY_WAREHOUSE
 *      （erp-client/features/sales-orders/lib/sales-order-create-model.ts createEmptyLine +
 *      api/mappers.ts mapFulfillmentMode）；履约责任在采购单侧才体现。
 *
 * 说明: e2e/tests 目录原不存在，本文件为该目录首个 spec；不修改 helpers/其他文件。
 */
import {
    test,
    expect,
    type APIRequestContext,
    type Locator,
    type Page,
} from "@playwright/test"
import path from "node:path"

import { newLoggedInContext } from "../helpers/login"
import { api, apiLogin } from "../helpers/api"
import {
    gotoPage,
    clickButton,
    fillByLabel,
    expectTableRow,
} from "../helpers/ui"

// ---------------------------------------------------------------------------
// 流程内专用小工具（保持 helpers 通用，不放业务逻辑）
// ---------------------------------------------------------------------------

/** 取组合框选项/弹层的根页面（Base UI 选项弹层 portal 到 body，需页面级定位）。 */
function rootPageOf(scope: Page | Locator): Page {
    return (scope as Locator).page?.() ?? (scope as Page)
}

/** 在已打开的日历 popover 中点击指定日号（react-day-picker 日按钮文本即日号）。 */
async function pickCalendarDay(page: Page, day: number): Promise<void> {
    const popover = page.locator('[data-slot="popover-content"]').last()
    await expect(popover).toBeVisible({ timeout: 10_000 })
    const dayButton = popover
        .getByRole("button")
        .filter({ hasText: new RegExp(`^${day}$`) })
        .first()
    await expect(dayButton).toBeVisible({ timeout: 10_000 })
    await dayButton.click()
}

/** 从已打开的组合框（按 aria-label 定位输入框）中选择匹配文本的选项。 */
async function pickOption(
    scope: Page | Locator,
    ariaLabel: string,
    optionText: string,
): Promise<void> {
    // exact 精确匹配：子串匹配会误命中 "合同编号"/"上传合同 PDF" 等相近 label
    const input = scope.getByLabel(ariaLabel, { exact: true }).last()
    await input.click()
    await input.fill(optionText)
    const option = rootPageOf(scope)
        .locator('[data-slot="combobox-item"]')
        .filter({ hasText: optionText })
        .first()
    await expect(option).toBeVisible({ timeout: 20_000 })
    await option.click()
}

/** 从已打开的组合框中选择第一个选项（用于运行时发现主数据）。 */
async function pickFirstOption(scope: Page | Locator, ariaLabel: string): Promise<void> {
    const input = scope.getByLabel(ariaLabel).last()
    await input.click()
    const option = rootPageOf(scope).locator('[data-slot="combobox-item"]').first()
    await expect(option).toBeVisible({ timeout: 20_000 })
    await option.click()
}

/** 在统一工作台（W02 /workspace）中按任务按钮 id 完成"通过"。 */
async function approveFromWorkspace(
    page: Page,
    taskButtonId: string,
): Promise<void> {
    // 工作台列表在桌面/移动布局各渲染一份（后者 lg:hidden），取第一个（可见的桌面列表）
    const task = page.locator(`#${taskButtonId}`).first()
    await expect(task).toBeVisible({ timeout: 30_000 })
    await task.click()
    const detail = page.locator('section[aria-label="当前任务"]').first()
    const approveButton = detail.getByRole("button", { name: "通过" }).first()
    await expect(approveButton).toBeVisible({ timeout: 20_000 })
    await approveButton.click()
    const decisionDialog = page.getByRole("dialog").last()
    const submitButton = decisionDialog.getByRole("button", {
        name: "提交决定",
    })
    await expect(submitButton).toBeVisible({ timeout: 20_000 })
    await submitButton.click()
    await expect(decisionDialog).not.toBeVisible({ timeout: 20_000 }).catch(() => {})
    // 审批完成后任务应从待办列表消失
    await expect(task).toHaveCount(0, { timeout: 20_000 })
}

/** 在 AlertDialog（FormalActionConfirmDialog）中点击确认按钮并等待弹窗关闭。 */
async function confirmAlertDialog(page: Page, buttonName: string): Promise<void> {
    const alertDialog = page.getByRole("alertdialog").last()
    await expect(alertDialog).toBeVisible({ timeout: 20_000 })
    await alertDialog.getByRole("button", { name: buttonName }).first().click()
    await expect(alertDialog).not.toBeVisible({ timeout: 20_000 }).catch(() => {})
}

/** 库存台账快照（余额 + 流水，代发前后对比用）。 */
type InventorySnapshot = { balances: unknown[]; movements: unknown[] }

async function inventorySnapshot(
    apiCtx: APIRequestContext,
    token: string,
): Promise<InventorySnapshot> {
    const balances = await api<{ items: unknown[] }>(
        apiCtx,
        "GET",
        "/admin/stock-balances",
        { token, query: { page: 1, page_size: 100 } },
    )
    const movements = await api<{ items: unknown[] }>(
        apiCtx,
        "GET",
        "/admin/stock-movements",
        { token, query: { page: 1, page_size: 100 } },
    )
    return { balances: balances.items ?? [], movements: movements.items ?? [] }
}

/**
 * 合同上传对话框（合同列表页检测到 customerId 后自动打开）。
 * 签订日期/有效期已由表单种子（今天/明年同日），付款条件默认"按合同约定"。
 */
async function uploadContractPdf(
    page: Page,
    contractNo: string,
    customerLegalName: string,
): Promise<void> {
    const dialog = page.getByRole("dialog").last()
    await expect(dialog).toBeVisible({ timeout: 20_000 })
    await expect(
        dialog.getByRole("heading", { name: "上传合同 PDF" }),
    ).toBeVisible()
    await dialog.getByLabel("合同编号").fill(contractNo)
    await dialog
        .locator('input[type="file"][aria-label="上传合同 PDF"]')
        .setInputFiles(path.join(__dirname, "../fixtures/sample-contract.pdf"))
    // 客户已由 initialCustomerId 预选；结算主体选择当前唯一 party（客户自身）。
    // 名称字段经 onItemChange 回填，远程搜索偶发竞态：按钮未启用则重选一次。
    const submitBtn = dialog.getByRole("button", { name: "上传并归档" })
    for (let attempt = 1; ; attempt++) {
        await pickOption(dialog, "结算主体", customerLegalName)
        try {
            await expect(submitBtn).toBeEnabled({ timeout: 15_000 })
            break
        } catch {
            if (attempt >= 2) {
                console.error(
                    "上传并归档 仍禁用；对话框错误:",
                    await dialog
                        .locator('[data-invalid="true"], [role="alert"]')
                        .allTextContents()
                        .catch(() => []),
                )
                throw new Error("上传并归档 仍禁用（结算主体名称未回填）")
            }
        }
    }
    await submitBtn.click()
    await expect(dialog).not.toBeVisible({ timeout: 20_000 })
}

test.describe("flow-02 供应商直接发客户（代发）", () => {
    test.setTimeout(600_000)

    test("客户/合同/销售单→采购确认→采购单→财务审核→代发→客户验收→库存无变化", async ({
        browser,
        request,
    }) => {
        // -----------------------------------------------------------------
        // 0) 准备：唯一业务标识 + API 上下文（库存台账断言用）
        // -----------------------------------------------------------------
        const stamp = Date.now().toString().slice(-8)
        const customerLegalName = `e2e代发客户${stamp}`
        const customerShortName = `代发${stamp}`
        // 必须恰好 18 位字母数字（customer-form-schemas.ts /^[0-9A-Za-z]{18}$/）
        const unifiedCreditCode = `91310000E2E${stamp.slice(0, 7)}`
        const contractNo = `HT-E2E-${stamp}`
        const trackingNo = `SF${stamp}`
        const futureDay = Math.min(28, new Date().getDate() + 5)

        const apiCtx = request
        const financeToken = await apiLogin(apiCtx, "finance")
        // 库存台账 API 需 stock_balance:list（采购/仓储/管理角色），财务无此权限
        const procurementToken = await apiLogin(apiCtx, "procurement")

        // -----------------------------------------------------------------
        // 1) 销售(xiaoshou)：新建客户
        // -----------------------------------------------------------------
        const salesCtx = await newLoggedInContext(browser, "sales")
        const salesPage = salesCtx.page
        await gotoPage(salesPage, "/sales/customers")

        const createDialog = await test.step("1.1 新建客户", async () => {
            await clickButton(salesPage, "新建客户")
            const dialog = salesPage.getByRole("dialog").last()
            await expect(dialog).toBeVisible({ timeout: 20_000 })
            await expect(dialog.getByText("新建客户")).toBeVisible()
            await fillByLabel(salesPage, "法定名称", customerLegalName)
            await fillByLabel(salesPage, "客户简称", customerShortName)
            await fillByLabel(salesPage, "统一社会信用代码", unifiedCreditCode)
            await dialog.getByRole("button", { name: "创建客户" }).click()
            return dialog
        })
        // 成功结果卡 + 弹窗关闭 + 进入客户详情
        await expect(createDialog.getByText("客户已创建")).toBeVisible({
            timeout: 20_000,
        })
        await expect(createDialog).not.toBeVisible({ timeout: 20_000 })
        await expect(salesPage).toHaveURL(/\/sales\/customers\/[0-9a-f]{32}/, {
            timeout: 20_000,
        })
        const customerId = salesPage.url().split("/").pop()!.split("?")[0]
        await expect(
            salesPage.getByText(customerLegalName).first(),
        ).toBeVisible({ timeout: 20_000 })

        // -----------------------------------------------------------------
        // 2) 销售(xiaoshou)：上传合同 PDF（客户详情 → 合同列表携带 customerId）
        // -----------------------------------------------------------------
        await test.step("2.1 打开上传合同弹窗", async () => {
            // 客户详情页动作为按钮（GuardedBusinessAction 渲染为 button role）
            await salesPage.getByRole("button", { name: "上传合同 PDF" }).click()
            await expect(salesPage).toHaveURL(/\/sales\/contracts\?customerId=/, {
                timeout: 20_000,
            })
            // 合同列表页检测到 customerId 后自动打开上传弹窗
            await expect(
                salesPage
                    .getByRole("dialog")
                    .last()
                    .getByRole("heading", { name: "上传合同 PDF" }),
            ).toBeVisible({ timeout: 20_000 })
        })
        await uploadContractPdf(salesPage, contractNo, customerLegalName)
        // 归档结果卡 + 合同列表出现新行
        await expect(salesPage.getByText("合同 PDF 已归档")).toBeVisible({
            timeout: 20_000,
        })
        await expectTableRow(salesPage, contractNo)

        // -----------------------------------------------------------------
        // 3) 销售(xiaoshou)：创建并提交实物/服务销售单
        // -----------------------------------------------------------------
        await gotoPage(salesPage, `/sales/customers/${customerId}`)
        await salesPage.getByRole("button", { name: "新建销售单" }).click()
        await expect(salesPage).toHaveURL(/\/sales\/orders\?mode=create/, {
            timeout: 20_000,
        })

        // 3.1 选择合同（自动带出客户/结算主体/付款条件）
        await pickOption(salesPage, "合同", contractNo)
        await expect(
            salesPage.getByText(new RegExp(`${contractNo}@v`)).first(),
        ).toBeVisible({ timeout: 20_000 })
        await expect(salesPage.getByText(customerLegalName).first()).toBeVisible()

        // 3.2 单据头：福利场景 / 付款条件（合同 effect 已跑完，再覆盖）/ 履约期限
        await pickOption(salesPage, "福利场景", "年节礼包")
        await pickOption(salesPage, "付款条件", "货到 15 天")
        // 履约方式：供应商直发（代发流程；单据头选择后应用到全部明细行）
        await pickOption(salesPage, "履约方式", "供应商直发")
        const headerSection = salesPage
            .locator("section")
            .filter({ has: salesPage.getByRole("heading", { name: "单据头" }) })
        await headerSection.getByRole("button", { name: /^(选择日期|已选日期)/ }).click()
        await pickCalendarDay(salesPage, futureDay)

        // 3.3 销售明细：SKU（运行时取第一个可售 SKU）/ 含税单价 / 交付日期
        const lineTable = salesPage.getByRole("table", {
            name: "销售单创建明细",
        })
        const skuInput = lineTable.getByLabel("商品").last()
        await skuInput.click()
        const firstSkuOption = salesPage
            .locator('[data-slot="combobox-item"]')
            .first()
        await expect(firstSkuOption).toBeVisible({ timeout: 20_000 })
        await firstSkuOption.click()
        // Base UI Combobox fillInputOnItemPress：选中后输入框值 = 商品名（sellable-skus.name）
        await expect(skuInput).toHaveValue(/.+/)
        const itemName = (await skuInput.inputValue()).trim()
        expect(itemName.length).toBeGreaterThan(0)
        await lineTable.getByLabel("含税单价").fill("100")
        await lineTable.getByRole("button", { name: /^(选择日期|已选日期)/ }).click()
        await pickCalendarDay(salesPage, futureDay)

        // 3.4 提交销售单（确认弹窗 → 跳转详情）
        await clickButton(salesPage, "提交")
        await confirmAlertDialog(salesPage, "确认提交")
        await expect(salesPage).toHaveURL(/\/sales\/orders\/[0-9a-f]{32}/, {
            timeout: 30_000,
        })
        const salesOrderId = salesPage.url().split("/").pop()!.split("?")[0]
        const salesOrderNo = (
            await salesPage.getByText(/XS\d{14}/).first().textContent()
        )!.trim()
        expect(salesOrderNo).toMatch(/^XS\d{14}$/)
        await expect(
            salesPage.getByText("审批中", { exact: true }).first(),
        ).toBeVisible({ timeout: 20_000 })

        // -----------------------------------------------------------------
        // 4) 采购(caigou)：工作台通过"采购确认"节点 → 销售单生效
        // -----------------------------------------------------------------
        const procurementCtx = await newLoggedInContext(browser, "procurement")
        const procurementPage = procurementCtx.page
        await gotoPage(procurementPage, "/workspace")
        await approveFromWorkspace(procurementPage, `work-item-${salesOrderId}`)

        // 销售侧回看：销售单已生效（详情 + 列表）
        await gotoPage(salesPage, `/sales/orders/${salesOrderId}`)
        await expect(
            salesPage.getByText("已生效", { exact: true }).first(),
        ).toBeVisible({ timeout: 20_000 })
        await gotoPage(salesPage, "/sales/orders")
        await expectTableRow(salesPage, salesOrderNo)

        // -----------------------------------------------------------------
        // 5) 采购(caigou)：创建采购单并提交 → 财务审核
        // -----------------------------------------------------------------
        await gotoPage(procurementPage, "/procurement/orders")
        await clickButton(procurementPage, "新建采购单")
        const basisDialog = procurementPage.getByRole("dialog").last()
        await expect(basisDialog).toBeVisible({ timeout: 20_000 })
        await expect(basisDialog.getByText("从采购创建依据建单")).toBeVisible()

        const noBasisText = basisDialog.getByText("当前没有可消费的创建依据")
        if (await noBasisText.isVisible().catch(() => false)) {
            throw new Error(
                "采购创建依据为空：后端 creation_basis_list 恒返回空列表（backend/services/src/purchase_order/creation_basis.rs 已删除旧确认批次），" +
                    "W08 无可用建单入口，流程无法继续。若运行后端为旧构建，请重启后端后重试。",
            )
        }
        // 选择第一个（唯一）创建依据 → 创建草稿并打开（进入编辑态）
        await pickFirstOption(basisDialog, "选择创建依据")
        await expect(basisDialog.getByText("拆单键（不可混拼）")).toBeVisible({
            timeout: 10_000,
        })
        await basisDialog.getByRole("button", { name: "创建草稿并打开" }).click()
        await expect(procurementPage).toHaveURL(
            /\/procurement\/orders\/[0-9a-f]{32}\?mode=edit/,
            { timeout: 30_000 },
        )
        const purchaseOrderId = procurementPage
            .url()
            .split("/")
            .pop()!
            .split("?")[0]
        await expect(procurementPage.getByText("采购草稿编辑")).toBeVisible({
            timeout: 20_000,
        })
        // D15 采购单详情/列表不返回 sales_order_no（后端契约缺口，见
        // backend/services/src/purchase_order/dto.rs 注释），来源以 sales_order_id 展示
        await expect(
            procurementPage.getByText(new RegExp(salesOrderId)).first(),
        ).toBeVisible({ timeout: 20_000 })

        // 明细行（按运行时商品名定位）：数量/含税单价/税率显式填写，付款条件保持依据默认
        const qtyInput = procurementPage.getByLabel(`${itemName} 数量`)
        const costInput = procurementPage.getByLabel(`${itemName} 含税单价`)
        const taxInput = procurementPage.getByLabel(`${itemName} 税率（%）`)
        await expect(qtyInput).toBeVisible({ timeout: 20_000 })
        await qtyInput.fill("1")
        await costInput.fill("80")
        await taxInput.fill("13")

        // 提交审批 → 确认 → 采购单进入审批中
        await clickButton(procurementPage, "提交审批")
        await confirmAlertDialog(procurementPage, "确认提交")
        await expect(procurementPage).toHaveURL(
            new RegExp(`/procurement/orders/${purchaseOrderId}$`),
            { timeout: 30_000 },
        )
        await expect(
            procurementPage.getByText("审批中", { exact: true }).first(),
        ).toBeVisible({ timeout: 20_000 })

        // -----------------------------------------------------------------
        // 6) 财务(caiwu)：工作台通过"采购单财务审核" → 采购单生效
        // -----------------------------------------------------------------
        const financeCtx = await newLoggedInContext(browser, "finance")
        const financePage = financeCtx.page
        await gotoPage(financePage, "/workspace")
        await approveFromWorkspace(financePage, `work-item-${purchaseOrderId}`)

        await gotoPage(procurementPage, `/procurement/orders/${purchaseOrderId}`)
        await expect(
            procurementPage.getByText("已生效", { exact: true }).first(),
        ).toBeVisible({ timeout: 20_000 })

        // -----------------------------------------------------------------
        // 7) 代发前库存台账快照（API）
        // -----------------------------------------------------------------
        const inventoryBefore = await inventorySnapshot(apiCtx, procurementToken)

        // -----------------------------------------------------------------
        // 8) 采购(caigou)：登记代发出库单（供应商直发，不经仓库）
        // -----------------------------------------------------------------
        await gotoPage(procurementPage, "/fulfillment?lane=procurement")
        // W09 队列卡片按「类型标签 + 原始销售单 id」投影（不解析 XS 单号，
        // 见 queue 投影 documents.ts deliveryToOperation）
        const directTask = procurementPage
            .getByRole("button")
            .filter({ hasText: new RegExp(`供应商直发.*${salesOrderId}`) })
            .first()
        await expect(directTask).toBeVisible({ timeout: 30_000 })
        await directTask.click()
        // 工作台标题（CardTitle 为文本而非 heading role）：供应商直发 · {销售单原始 id}
        await expect(
            procurementPage
                .getByText(new RegExp(`供应商直发 · ${salesOrderId}`))
                .first(),
        ).toBeVisible({ timeout: 20_000 })

        // 承运方 + 物流单号（发货数量已由系统带出）
        await procurementPage.locator("#direct-carrier").click()
        await procurementPage
            .getByRole("option")
            .filter({ hasText: "顺丰速运" })
            .first()
            .click()
        await procurementPage.locator("#direct-tracking").fill(trackingNo)

        // 确认发货（影响预览含"不动自己仓库的库存"）
        await clickButton(procurementPage, "确认发货")
        const alertDialog = procurementPage.getByRole("alertdialog").last()
        await expect(alertDialog.getByText("确认发货？")).toBeVisible()
        await alertDialog.getByRole("button", { name: "确认发货" }).click()
        await expect(alertDialog).not.toBeVisible({ timeout: 20_000 }).catch(() => {})

        // 结果面板：已发货 + 待办单据消失
        const resultPanel = procurementPage.locator(
            '[data-slot="formal-action-result"]',
        )
        await expect(resultPanel.getByText("已发货", { exact: true })).toBeVisible({
            timeout: 20_000,
        })
        await expect(directTask).toHaveCount(0, { timeout: 20_000 })

        // -----------------------------------------------------------------
        // 9) 代发后库存台账：无变化（API 对比 + UI 复核）
        // -----------------------------------------------------------------
        const inventoryAfter = await inventorySnapshot(apiCtx, procurementToken)
        expect(inventoryAfter.balances).toEqual(inventoryBefore.balances)
        expect(inventoryAfter.movements).toEqual(inventoryBefore.movements)
        expect(inventoryAfter.movements).toHaveLength(0)
        expect(inventoryAfter.balances).toHaveLength(0)

        await gotoPage(procurementPage, "/inventory")
        await expect(
            procurementPage.getByRole("heading", { name: "库存台账" }).first(),
        ).toBeVisible({ timeout: 20_000 })
        // 余额视图不得出现本流程 SKU 的任何行（无库存变动）
        await expect(
            procurementPage.locator("table").getByText(itemName),
        ).toHaveCount(0)

        // -----------------------------------------------------------------
        // 10) 销售(xiaoshou)：登记客户验收
        // -----------------------------------------------------------------
        await gotoPage(salesPage, `/sales/orders/${salesOrderId}?section=acceptance`)
        await expect(
            salesPage.getByRole("heading", { name: "可验收的交付记录" }),
        ).toBeVisible({
            timeout: 30_000,
        })
        const factCheckbox = salesPage.getByRole("checkbox", {
            name: /代发/,
        })
        await expect(factCheckbox).toBeVisible({ timeout: 20_000 })
        await factCheckbox.check()
        // 选中后出现分配数量输入（默认=本次最多可验收数量）
        await expect(salesPage.locator('input[id^="alloc-qty-"]')).toBeVisible({
            timeout: 10_000,
        })

        await clickButton(salesPage, "确认并完成验收")
        await confirmAlertDialog(salesPage, "确认验收")
        // 验收历史出现"已确认"记录
        await expect(salesPage.getByText("验收历史").first()).toBeVisible({
            timeout: 20_000,
        })
        await expect(
            salesPage.getByText("已确认", { exact: true }).first(),
        ).toBeVisible({ timeout: 20_000 })

        // 收尾：销售单仍为已生效，验收摘要不再显示"还没有验收记录"
        await gotoPage(salesPage, `/sales/orders/${salesOrderId}?section=acceptance`)
        await expect(
            salesPage.getByText("还没有验收记录", { exact: false }),
        ).toHaveCount(0, { timeout: 20_000 })

        await apiCtx.dispose()
        await salesCtx.context.close()
        await procurementCtx.context.close()
        await financeCtx.context.close()
    })
})
