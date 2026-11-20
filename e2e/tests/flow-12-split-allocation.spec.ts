/**
 * 流程: [flow-12] 同一销售明细拆分：库存直配 + 采购缺口
 * 文档: docs/erp-phase-1.md §7.4（同一明细允许库存与采购拆分满足）
 * 使用账号: cangchu（盘盈/入库/仓发）、caiwu（库存调整审批）、caigoushenpi（采购单审批）、
 *           xiaoshou（客户/合同/销售单/验收）、caigou（销售审批与供给分配）
 *
 * 文档-代码差异（以代码为准）:
 * - 库存调整 POSTED 徽标文案是「已过账」，docs/erp-phase-1.md §10.1 要求资金库存确认不用「过账」；
 *   本流程只匹配按钮「确认入库/确认发货」，状态徽标按代码「已过账」断言。
 * - 盘盈必须基于已有 stock_balance 行；空台账无法从 UI 创建余额维度
 *   （后端：请先建立期初或入库）。本流程在无余额时只插入数量为 0 的维度行，
 *   可用量仍由仓储盘盈 + 财务审批产生。仓储角色无库存默认范围时不渲染搜索框。
 * - 供给分配确认后采购单立即提交审批，状态为「审批中」，不会留下未提交草稿。
 * - 待办只在 /workspace 原地处理；供给分配嵌入 PurchaseOrderCreatePage。
 * - 选源是 OptionCombobox。关闭时输入框不一定显示选项文案，以打开后
 *   `data-selected` 的选项标签区分「可用」库存和「入仓」采购。
 */
import { workPage } from "../helpers/workspace-page"
import { setBeijingShippingAddress } from "../helpers/sales-orders"
import { archiveContractViaUi } from "../helpers/contracts"
import { fillSalesLineUnitPrice } from "../helpers/sales-orders"
import fs from "node:fs"
import path from "node:path"
import { expect, test, type BrowserContext, type Locator, type Page } from "../helpers/test"

import { createCustomerViaUi } from "../helpers/customers"
import { apiGet } from "../helpers/api"
import { fillFirstDeliveryParcel, uploadAcceptanceEvidence } from "../helpers/fulfillment"
import { FRONTEND_BASE_URL } from "../helpers/env"
import {
    ensureWarehouseSalesSourceAccess,
    ensureWarehouseStockScope,
    ensureZeroBalanceDimension,
    singleSalesLineId,
} from "../helpers/inventory"
import { openLoggedInWorkspace } from "../helpers/login"
import { payOnlySupplierTask } from "../helpers/payments"
import { confirmSupplyAllocation, expandSourcingEditor } from "../helpers/sourcing"
import {
    approveCurrentDocument,
    chooseOption,
    expectToast,
    openFulfillmentWorkspaceForm,
    openWorkspaceTask,
    openWorkspaceTaskRow,
    pickCalendarDay,
    readHeaderDocumentNumber,
    selectWorkspaceFamily,
    expectSourcingCounts,
} from "../helpers/ui"

const FRONTEND_BASE = FRONTEND_BASE_URL
const TIMEOUT = 20_000

const SKU_NO = "TEA-SF-LJ-250"
const SKU_NAME = "狮峰明前龙井礼盒"
const WAREHOUSE_CODE = "BJ-TZ-01"
const WAREHOUSE_NAME = "北京通州仓"
const STOCK_QTY = "3"
const SALES_QTY = "10"
const PURCHASE_QTY = "7"

const CONTRACT_PDF = path.join(process.cwd(), "fixtures", "sample-contract.pdf")
const MINIMAL_PDF = Buffer.from(
    "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n",
)

type Session = {
    context: BrowserContext
    page: Page
}

async function closeSession(session: Session | undefined) {
    if (!session) return
    await session.context.close()
}

async function searchAndSubmit(input: Locator, query: string) {
    await input.fill(query)
    await input.press("Enter")
}

async function searchInventoryLedger(
    page: Page,
    query: string,
    view: "balance" | "reservation" = "balance",
) {
    const tab = workPage(page).locator(`#inventory-ledger-view-${view}`)
    await expect(tab).toBeVisible({ timeout: TIMEOUT })
    if ((await tab.getAttribute("aria-pressed")) !== "true") {
        await tab.click()
        await page.waitForURL(url => url.searchParams.get("view") === view, {
            waitUntil: "load", timeout: TIMEOUT,
        })
        await expect(tab).toHaveAttribute("aria-pressed", "true", { timeout: TIMEOUT })
    }

    // 旧行在搜索导航期间仍可见；必须等本次查询落到当前页面，才能创建调整草稿。
    const [response] = await Promise.all([
        page.waitForResponse(response => {
            const url = new URL(response.url())
            return response.request().method() === "GET"
                && url.pathname === `/admin/stock-${view === "balance" ? "balances" : "reservations"}`
                && url.searchParams.get("q") === query
                && url.searchParams.get("page_size") === "20"
        }, { timeout: TIMEOUT }),
        page.waitForURL(url => url.searchParams.get("q") === query
            && (url.searchParams.get("view") ?? "balance") === view, {
            waitUntil: "load", timeout: TIMEOUT,
        }),
        searchAndSubmit(workPage(page).locator("#inventory-ledger-search"), query),
    ])
    expect(response.ok(), "库存台账查询应成功").toBe(true)
    expect(await response.finished(), "库存台账查询响应应完整返回").toBeNull()
    await expect(workPage(page).getByText(`搜索：${query}`, { exact: true })).toBeVisible({ timeout: TIMEOUT })
    await expect(workPage(page).getByText("正在加载库存…", { exact: true })).toBeHidden({ timeout: TIMEOUT })
}

function uniqueCreditCode(): string {
    const stamp = Date.now().toString()
    return `91E2E${stamp}`.replace(/[^0-9A-Za-z]/g, "0").padEnd(18, "0").slice(0, 18)
}

function plusDaysIso(days: number): string {
    const date = new Date()
    date.setDate(date.getDate() + days)
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function pdfUpload(): { name: string; mimeType: string; buffer: Buffer } | string {
    if (fs.existsSync(CONTRACT_PDF)) return CONTRACT_PDF
    return {
        name: "sample-contract.pdf",
        mimeType: "application/pdf",
        buffer: MINIMAL_PDF,
    }
}

async function submitInventoryCountGain(page: Page): Promise<string> {
    await page.goto(`${FRONTEND_BASE}/inventory`)
    await expect(workPage(page).getByRole("heading", { name: "库存台账" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await searchInventoryLedger(page, SKU_NO)
    const warehouseRow = workPage(page).getByRole("row").filter({ hasText: WAREHOUSE_NAME }).filter({ hasText: SKU_NO })
    await expect(warehouseRow).toBeVisible({ timeout: TIMEOUT })
    await warehouseRow.getByRole("button", { name: "库存调整" }).click()
    const dialog = workPage(page).getByRole("dialog", { name: "发起库存调整" })
    await expect(dialog).toBeVisible({ timeout: TIMEOUT })
    await chooseOption(page, dialog.getByLabel("原因类型"), /盘盈/)
    await dialog.getByLabel(/调整数量/).fill(STOCK_QTY)
    await dialog.getByLabel("原因说明").fill("E2E flow-12 盘盈准备少于销售数量的库存")
    const adjustmentNo = (await dialog.locator("span.num").filter({ hasText: /^TZ/ }).innerText()).trim()
    expect(adjustmentNo.length, "发起弹窗应显示本张调整单号").toBeGreaterThan(2)
    await dialog.getByRole("button", { name: "提交审批" }).click()
    await expect(workPage(page).getByText("调整已提交审批")).toBeVisible({ timeout: TIMEOUT })
    return adjustmentNo
}

async function assertAvailableQuantity(page: Page, quantity: string) {
    await page.goto(`${FRONTEND_BASE}/inventory`)
    await expect(workPage(page).getByRole("heading", { name: "库存台账" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await searchInventoryLedger(page, SKU_NO)
    const row = workPage(page).getByRole("row").filter({ hasText: WAREHOUSE_NAME }).filter({ hasText: SKU_NO })
    await expect(row).toBeVisible({ timeout: TIMEOUT })
    await expect(row.locator('[data-column-id="available"] .num')).toHaveText(
        new RegExp(`^${quantity}(?:\\s*盒)?$`), { timeout: TIMEOUT },
    )
    await expect(row.getByText("零可用", { exact: true })).toHaveCount(0)
}

async function createSalesOrderWithContract(
    page: Page,
    customerName: string,
    contractNo: string,
): Promise<{ salesOrderId: string; salesOrderNo: string }> {
    await page.goto(`${FRONTEND_BASE}/sales/orders?mode=create`)
    await archiveContractViaUi(page, { contractNo, customerName, pdf: pdfUpload() })
    await expect(workPage(page).getByText("销售明细")).toBeVisible({ timeout: TIMEOUT })
    await expect(workPage(page).locator("#sales-orders-create-customer")).toContainText(new RegExp(customerName))

    await chooseOption(page, workPage(page).getByLabel("福利场景"), "礼包")
    await workPage(page).locator("#sales-orders-create-line-items-add").click()
    const skuDialog = workPage(page).getByRole("dialog", { name: "添加商品" })
    await expect(skuDialog).toBeVisible({ timeout: TIMEOUT })
    await searchAndSubmit(skuDialog.locator("#sales-orders-sku-picker-filter-search"), SKU_NO)
    await expect(skuDialog.getByText(SKU_NO, { exact: true })).toBeVisible({ timeout: TIMEOUT })
    await skuDialog.getByRole("checkbox", { name: new RegExp(SKU_NAME) }).check()
    await skuDialog.getByRole("button", { name: /加入所选/ }).click()
    await expect(skuDialog).toBeHidden({ timeout: TIMEOUT })
    await expect(workPage(page).getByRole("button", { name: new RegExp(`更换销售项目 ${SKU_NAME}`) })).toBeVisible({ timeout: TIMEOUT })

    await workPage(page).getByLabel("数量").fill(SALES_QTY)
    await fillSalesLineUnitPrice(page, "1288.00")
    await setBeijingShippingAddress(page)
    await workPage(page).locator("#sales-orders-create-batch-due-date-open").click()
    await pickCalendarDay(page, workPage(page).locator("#sales-orders-create-batch-due-date"), plusDaysIso(21))
    await workPage(page).getByRole("button", { name: "应用到全部明细" }).click()
    await expectToast(page, "已批量设置交期")

    await workPage(page).getByRole("button", { name: "提交审批", exact: true }).click()
    const submit = workPage(page).getByRole("dialog", { name: "提交销售单" })
    await expect(submit).toBeVisible({ timeout: TIMEOUT })
    await submit.getByRole("button", { name: "确认提交" }).click()
    await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, { timeout: TIMEOUT })
    const salesOrderId = page.url().match(/\/sales\/orders\/([^/?]+)/)?.[1]
    if (!salesOrderId) throw new Error("未能从 URL 读取销售单身份")
    await expect(workPage(page).locator('[data-slot="badge"]').filter({ hasText: /^审批中$/ }).first()).toBeVisible({ timeout: TIMEOUT })
    const salesOrderNo = await readHeaderDocumentNumber(page)
    return { salesOrderId, salesOrderNo }
}

async function sourcingEditorControl(
    editor: Locator,
    route: RegExp,
    field: "quantity" | "warehouse" | "sourcing-option",
) {
    const source = editor.getByRole("region", { name: "供给来源列表" })
        .getByRole("button", { name: route })
    await expect(source).toHaveCount(1, { timeout: TIMEOUT })
    await source.click()
    const input = editor.getByRole("region", { name: "当前来源编辑" })
        .locator(`[id^="procurement-orders-create-row-"][id$="-${field}"]`)
    await expect(input).toBeVisible({ timeout: TIMEOUT })
    return input
}

async function confirmSplitAllocation(page: Page, salesOrderNo: string) {
    await openWorkspaceTask(page, "待安排采购", salesOrderNo, "procurement")
    await expect(workPage(page).getByRole("heading", { name: "供给分配", exact: true })).toBeVisible({ timeout: TIMEOUT })
    await workPage(page).getByRole("button", { name: "重新自动分配" }).click()
    await expectToast(page, /已重新分配供给|没有可匹配的供给方案/)
    await expandSourcingEditor(page)
    const editor = workPage(page).getByRole("dialog", { name: /^供给分配 · / })
    const stockRoute = /^编辑 .*现有库存/
    const purchaseRoute = /^编辑 .*(?:代发直发|集采直发|集采入仓)/
    const stockQty = await sourcingEditorControl(editor, stockRoute, "quantity")
    await expect(stockQty).toHaveValue(STOCK_QTY)
    // 将采购缺口改为同一供应商的入仓方案，保留库存拆分。
    const purchaseSource = await sourcingEditorControl(editor, purchaseRoute, "sourcing-option")
    await chooseOption(page, purchaseSource, /杭州狮峰.*入仓/, "入仓")
    const purchaseQty = await sourcingEditorControl(editor, purchaseRoute, "quantity")
    await purchaseQty.fill(PURCHASE_QTY)
    await expect(purchaseQty).toHaveValue(PURCHASE_QTY)
    const warehouseInput = await sourcingEditorControl(editor, purchaseRoute, "warehouse")
    await chooseOption(page, warehouseInput, new RegExp(WAREHOUSE_CODE), WAREHOUSE_CODE)

    const done = editor.locator("#sourcing-product-adjust-done")
    await (await sourcingEditorControl(editor, stockRoute, "quantity")).fill("4")
    await done.click()
    await expect(editor.locator("#sourcing-product-adjust-source-errors")).toContainText(
        /拆分数量合计不能超过|库存分配合计不能超过|本次分配数量不能超过/,
    )
    await expect(editor).toBeVisible()

    await (await sourcingEditorControl(editor, stockRoute, "quantity")).fill(STOCK_QTY)
    await done.click()
    await expect(editor).toBeHidden()
    await expectSourcingCounts(page, 1, 1)
    await workPage(page).locator("#procurement-orders-create-preview").click()
    const preview = workPage(page).getByRole("dialog", { name: "预览供给分配", exact: true })
    await expect(preview).toBeVisible({ timeout: TIMEOUT })
    await expect(preview.getByText("现有库存分配")).toBeVisible({ timeout: TIMEOUT })
    await expect(preview.getByRole("listitem").filter({ hasText: WAREHOUSE_NAME })).toContainText(new RegExp(`·\\s*${STOCK_QTY}\\s*盒$`))
    await expect(preview.getByRole("heading", { name: "采购单", exact: true })).toBeVisible({ timeout: TIMEOUT })
    const committed = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/admin/purchase-orders/from-sourcing"), { timeout: 60_000 })
    await confirmSupplyAllocation(page, /供给分配已完成|本次供给分配已保存/)
    const response = await committed
    expect(response.ok(), await response.text()).toBe(true)
    const result = (await response.json()).data
    expect(result.orders).toHaveLength(1)
    expect(result.stock_reservations).toHaveLength(1)
    expect(Number(result.stock_reservations[0].quantity)).toBe(Number(STOCK_QTY))
    expect(result.work_item_status).toBe("COMPLETED")
    expect(result.orders[0].purchase_order_id).toBeTruthy()
    return result.orders[0] as { purchase_order_id: string; purchase_no: string }
}

async function assertReservationAndPurchase(page: Page, salesOrderNo: string, salesLineId: string) {
    await page.goto(`${FRONTEND_BASE}/inventory?view=reservation`)
    await expect(workPage(page).getByRole("heading", { name: "库存台账" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await searchInventoryLedger(page, SKU_NO, "reservation")
    const reserved = workPage(page).getByRole("row").filter({ hasText: salesLineId })
    await expect(reserved).toBeVisible({ timeout: TIMEOUT })
    await expect(reserved.locator('[data-column-id="qty"] .num')).toHaveText(`${STOCK_QTY} / ${STOCK_QTY}`)
    await expect(reserved.getByText("有效", { exact: true })).toBeVisible({ timeout: TIMEOUT })

    await page.goto(`${FRONTEND_BASE}/procurement/orders`)
    await expect(workPage(page).getByRole("heading", { name: "采购单", exact: true })).toBeVisible({ timeout: TIMEOUT })
    await searchAndSubmit(workPage(page).getByLabel("搜索采购单"), salesOrderNo)
    await expect(workPage(page).getByText(salesOrderNo)).toBeVisible({ timeout: TIMEOUT })
    await expect(workPage(page).getByRole("row").filter({ hasText: salesOrderNo }).getByText("审批中", { exact: true })).toBeVisible({ timeout: TIMEOUT })
    await expect(workPage(page).getByRole("table").getByText("草稿", { exact: true })).toHaveCount(0)
    await expect(workPage(page).locator('[aria-label^="打开采购单"]')).toHaveCount(1)
    const poLinks = workPage(page).getByRole("row").filter({ hasText: salesOrderNo }).locator('[aria-label^="打开采购单"]')
    await expect(poLinks).toHaveCount(1)
    const fromAria = ((await poLinks.getAttribute("aria-label")) ?? "").replace(/^打开采购单\s*/, "").trim()
    const fromText = ((await poLinks.innerText()) || "").trim()
    const purchaseNo = /^PO-/.test(fromAria) ? fromAria : fromText
    expect(purchaseNo).toMatch(/^PO-/)
    return purchaseNo
}

async function completeFulfillment(
    page: Page,
    kind: "入库" | "仓发",
    extra: { quantity: string; trackingNo?: string; purchaseOrderId?: string; salesOrderId: string },
) {
    const operationType = kind === "入库" ? "RECEIPT" : "WAREHOUSE_SHIP"
    const token = await page.evaluate(() => localStorage.getItem("erp.token"))
    expect(token).toBeTruthy()
    const source = await apiGet<{
        total: number;
        items: Array<{ work_item_id: string; operation_id: string; operation_type: string; sales_order_id: string; purchase_order_id?: string }>;
    }>(token!, "/admin/work-items/fulfillment-queue", {
        sales_order_id: extra.salesOrderId, operation_types: operationType,
        purchase_order_id: kind === "入库" ? extra.purchaseOrderId : undefined, page: 1, page_size: 10,
    })
    expect(source.total).toBe(1)
    expect(source.items).toHaveLength(1)
    expect(source.items[0]).toMatchObject({ sales_order_id: extra.salesOrderId, operation_type: operationType })
    const workItemId = source.items[0].work_item_id
    await page.goto(`${FRONTEND_BASE}/workspace/tasks`)
    await selectWorkspaceFamily(page, "fulfillment")
    await openWorkspaceTaskRow(page, workItemId)
    const surface = await openFulfillmentWorkspaceForm(page)
    if (kind === "入库") {
        expect(source.items[0].purchase_order_id).toBe(extra.purchaseOrderId)
        const receipt = surface.getByRole("region", { name: "入库表单", exact: true })
        await expect(receipt).toBeVisible({ timeout: TIMEOUT })
        await receipt.getByLabel("到货数量").fill(extra.quantity)
        await chooseOption(page, receipt.getByLabel("质量结果"), /^合格$/)
        await surface.locator("#fulfillment-operations-work-surface-confirm").click()
        const confirm = workPage(page).getByRole("alertdialog", { name: "确认入库？" })
        await expect(confirm).toBeVisible({ timeout: TIMEOUT })
        const received = page.waitForResponse(response => response.request().method() === "POST" && /\/purchase-receipts\/[^/]+\/post$/.test(response.url()))
        await confirm.getByRole("button", { name: "确认入库" }).click()
        const receivedResponse = await received
        expect(receivedResponse.ok()).toBeTruthy()
        expect((await receivedResponse.json()).data).toMatchObject({
            id: source.items[0].operation_id, purchase_order_id: extra.purchaseOrderId, status: "POSTED",
        })
        await expect(confirm).toBeHidden({ timeout: TIMEOUT })
        return
    }

    const trackingNo = extra?.trackingNo ?? `SF12-${Date.now()}`
    await fillFirstDeliveryParcel(page, trackingNo, "顺丰速运")
    const task = await apiGet<{
        id: string; business_object_type: string; business_object_id: string; status: string;
    }>(token!, `/admin/work-items/${workItemId}`)
    expect(task).toMatchObject({ id: workItemId, business_object_type: "delivery", status: "OPEN", business_object_id: source.items[0].operation_id })
    const shipped = page.waitForResponse(response => response.request().method() === "POST" &&
        new URL(response.url()).pathname === `/admin/deliveries/${task.business_object_id}/post`,
        { timeout: TIMEOUT })
    await surface.locator("#fulfillment-operations-work-surface-confirm").click()
    const shippedResponse = await shipped
    expect(shippedResponse.ok(), await shippedResponse.text()).toBe(true)
    const delivery = (await shippedResponse.json()).data
    expect(delivery).toMatchObject({
        id: task.business_object_id, status: "SHIPPED", delivery_type: "WAREHOUSE_SHIP",
        sales_order_id: extra!.salesOrderId, purchase_order_id: null,
    })
    expect(delivery.tracking_entries).toHaveLength(1)
    expect(delivery.tracking_entries[0]).toMatchObject({ tracking_no: trackingNo, carrier: "顺丰速运" })
    const { packing } = shippedResponse.request().postDataJSON()
    expect(packing.packages).toEqual([expect.objectContaining({ tracking_no: trackingNo, carrier: "顺丰速运" })])
    if (extra?.quantity) expect(packing.packages[0].items.map((line: { quantity: string }) => Number(line.quantity))).toEqual([Number(extra.quantity)])
    expect(await apiGet(token!, `/admin/work-items/${workItemId}`)).toMatchObject({
        id: workItemId, business_object_id: task.business_object_id, status: "COMPLETED",
    })
    await expect(surface).toBeHidden({ timeout: TIMEOUT })
}

async function registerAcceptance(page: Page, salesOrderId: string) {
    const token = await page.evaluate(() => localStorage.getItem("erp.token"))
    expect(token).toBeTruthy()
    const queue = await apiGet<{
        items: Array<{ id: string; business_object_id: string; work_item_type: string; status: string; allowed_actions: string[] }>
    }>(token!, "/admin/work-items", {
        scope: "mine", work_item_type: "CUSTOMER_ACCEPTANCE_REGISTRATION", sales_order_ids: salesOrderId,
        timezone: "Asia/Shanghai", page: 1, page_size: 10,
    })
    expect(queue.items).toHaveLength(1)
    const task = queue.items[0]!
    expect(task).toMatchObject({ business_object_id: salesOrderId, work_item_type: "CUSTOMER_ACCEPTANCE_REGISTRATION", status: "OPEN" })
    expect(task.allowed_actions).toContain("PROCESS")
    await page.goto(`${FRONTEND_BASE}/workspace/tasks`)
    await selectWorkspaceFamily(page, "fulfillment")
    await openWorkspaceTaskRow(page, task.id)
    const open = workPage(page)
        .locator("#sales-orders-acceptance-register-open")
        .or(workPage(page).getByRole("button", { name: "登记客户验收" }))
        .first()
    await open.click()
    const register = workPage(page).getByRole("dialog", { name: "登记客户验收" })
    await expect(register).toBeVisible({ timeout: TIMEOUT })
    await uploadAcceptanceEvidence(page)
    await register.locator("#sales-orders-acceptance-register-submit").click()
    const confirm = workPage(page).getByRole("alertdialog", { name: "确认客户验收" })
    await expect(confirm).toBeVisible({ timeout: TIMEOUT })
    await confirm.getByRole("button", { name: "确认本次验收" }).click()
    await expect(confirm).toBeHidden({ timeout: TIMEOUT })
}

test.describe.configure({ mode: "serial" })

test("同一销售明细拆分：库存直配 + 采购缺口", async ({ browser }) => {
    test.setTimeout(8 * 60 * 1000)

    const customerName = `拆分配客户${Date.now()}`
    const creditCode = uniqueCreditCode()
    const contractNo = `HT-E2E-12-${Date.now()}`
    let salesOrderNo = ""
    let salesOrderId = ""
    let adjustmentNo = ""
    let purchaseNo = ""
    let purchaseOrderId = ""

    // 1. 销售创建客户（合同在建单时上传）
    const sales = await openLoggedInWorkspace(browser, "xiaoshou")
    try {
        await createCustomerViaUi(sales.page, {
            legalName: customerName,
            shortName: "拆分配客户",
            creditCode,
            paymentTermLabel: "按合同约定",
        })
        await searchAndSubmit(workPage(sales.page).getByLabel("搜索客户"), customerName)
        await expect(workPage(sales.page).getByRole("link", { name: "拆分配客户", exact: true })).toBeVisible({
            timeout: TIMEOUT,
        })
    } finally {
        await closeSession(sales)
    }

    // 2. 仓储盘盈少于销售数量的库存
    await ensureZeroBalanceDimension(WAREHOUSE_CODE, SKU_NO)
    await ensureWarehouseSalesSourceAccess()
    await ensureWarehouseStockScope(WAREHOUSE_CODE)
    const warehouse = await openLoggedInWorkspace(browser, "cangchu")
    try {
        adjustmentNo = await submitInventoryCountGain(warehouse.page)
    } finally {
        await closeSession(warehouse)
    }

    // 3. 财务审批这一张盘盈。无单号会通过队列里另一张库存调整单。
    const financeAdj = await openLoggedInWorkspace(browser, "caiwu")
    try {
        await openWorkspaceTask(financeAdj.page, "库存调整单审批", adjustmentNo, "approval")
        await approveCurrentDocument(financeAdj.page)
    } finally {
        await closeSession(financeAdj)
    }

    const warehouseCheck = await openLoggedInWorkspace(browser, "cangchu")
    try {
        await assertAvailableQuantity(warehouseCheck.page, STOCK_QTY)
    } finally {
        await closeSession(warehouseCheck)
    }

    // 4. 销售开单：数量大于现有库存
    const salesOrder = await openLoggedInWorkspace(browser, "xiaoshou")
    try {
        const created = await createSalesOrderWithContract(salesOrder.page, customerName, contractNo)
        salesOrderId = created.salesOrderId
        salesOrderNo = created.salesOrderNo
        expect(salesOrderId).toBeTruthy()
    } finally {
        await closeSession(salesOrder)
    }

    // 5. 采购审批销售单（选源不在本节点）
    const procurement = await openLoggedInWorkspace(browser, "caigou")
    try {
        await openWorkspaceTask(procurement.page, "销售单审批", salesOrderNo, "approval")
        const approvalPane = workPage(procurement.page).getByRole("region", { name: "当前工作台任务" })
        await expect(approvalPane.getByRole("combobox", { name: /履约方案|供给/ })).toHaveCount(0)
        await expect(approvalPane.getByRole("button", { name: /预览供给分配|确认供给分配/ })).toHaveCount(0)
        await expect(approvalPane.getByLabel(/采购成本|本次分配数量/)).toHaveCount(0)
        await approveCurrentDocument(procurement.page)

        // 6. 供给分配：同一明细拆成现有库存 + 采购缺口；负向超分配必须被拦住
        const purchase = await confirmSplitAllocation(procurement.page, salesOrderNo)
        purchaseOrderId = purchase.purchase_order_id
        purchaseNo = await assertReservationAndPurchase(procurement.page, salesOrderNo, await singleSalesLineId(salesOrderId))
        expect(purchaseNo).toBe(purchase.purchase_no)
    } finally {
        await closeSession(procurement)
    }

    // 7. 采购部门审批采购单（创建时已提交，不得再走草稿送审）
    const financePo = await openLoggedInWorkspace(browser, "caigoushenpi")
    try {
        await openWorkspaceTask(financePo.page, "采购单审批", purchaseNo, "approval")
        await approveCurrentDocument(financePo.page)
    } finally {
        await closeSession(financePo)
    }

    // 8. 仓发第一段（库存直配预占）+ 采购入库沿销售分配预占 + 仓发第二段
    const fulfillment = await openLoggedInWorkspace(browser, "cangchu")
    try {
        await completeFulfillment(fulfillment.page, "仓发", {
            salesOrderId,
            quantity: STOCK_QTY,
            trackingNo: `SF-STOCK-${Date.now()}`,
        })
        await completeFulfillment(fulfillment.page, "入库", {
            salesOrderId,
            quantity: PURCHASE_QTY,
            purchaseOrderId,
        })
        await fulfillment.page.goto(`${FRONTEND_BASE}/inventory?view=reservation`)
        await expect(workPage(fulfillment.page).getByRole("heading", { name: "库存台账" })).toBeVisible({
            timeout: TIMEOUT,
        })
        await searchInventoryLedger(fulfillment.page, SKU_NO, "reservation")
        const purchasedReservation = workPage(fulfillment.page).getByRole("row").filter({ hasText: await singleSalesLineId(salesOrderId) }).filter({ hasText: "有效" })
        await expect(purchasedReservation).toBeVisible({ timeout: TIMEOUT })
        await expect(purchasedReservation.locator('[data-column-id="qty"] .num')).toHaveText(`${PURCHASE_QTY} / ${PURCHASE_QTY}`)
        await completeFulfillment(fulfillment.page, "仓发", {
            salesOrderId,
            quantity: PURCHASE_QTY,
            trackingNo: `SF-PO-${Date.now()}`,
        })
    } finally {
        await closeSession(fulfillment)
    }

    // 手工入库和仓发不受先款门禁；另按采购单号执行正式付款。
    const payment = await openLoggedInWorkspace(browser, "fukuan")
    try {
        await payOnlySupplierTask(payment.page, purchaseNo)
    } finally {
        await closeSession(payment)
    }

    // 9. 销售验收：两段履约待验合计等于销售明细
    const acceptance = await openLoggedInWorkspace(browser, "xiaoshou")
    try {
        await registerAcceptance(acceptance.page, salesOrderId)
        await acceptance.page.goto(`${FRONTEND_BASE}/sales/orders/${salesOrderId}`)
        await expect(workPage(acceptance.page).getByText(`单号：${salesOrderNo}`, { exact: true })).toBeVisible({ timeout: TIMEOUT })
        await workPage(acceptance.page).getByRole("tab", { name: /^交付与验收/ }).click()
        await workPage(acceptance.page).locator("#sales-order-section-acceptance").click()
        await expect(workPage(acceptance.page).getByText("验收进度")).toBeVisible({ timeout: TIMEOUT })
        await expect(workPage(acceptance.page).getByText(new RegExp(`${SALES_QTY}\\s*盒`)).first()).toBeVisible({
            timeout: TIMEOUT,
        })
        await expect(workPage(acceptance.page).getByText(/已通过/).first()).toBeVisible({ timeout: TIMEOUT })
        await expect(workPage(acceptance.page).getByRole("button", { name: "登记客户验收" })).toHaveCount(0)
    } finally {
        await closeSession(acceptance)
    }
})
