/**
 * 流程: [flow-12] 同一销售明细拆分：库存直配 + 采购缺口
 * 文档: docs/erp-phase-1.md §7.4（同一明细允许库存与采购拆分满足）
 * 使用账号: cangchu（盘盈/入库/仓发）、caiwu（库存调整与采购单审批）、
 *           xiaoshou（客户/合同/销售单/验收）、caigou（销售审批与供给分配）
 *
 * 文档-代码差异（以代码为准）:
 * - 库存调整 POSTED 徽标文案是「已过账」，文档/ui-glossary 要求资金库存确认不用「过账」；
 *   本流程只匹配按钮「确认入库/确认发货」，状态徽标按代码「已过账」断言。
 * - 盘盈必须基于已有 stock_balance 行；空台账无法从 UI 创建余额维度
 *   （后端：请先建立期初或入库）。本流程在无余额时只插入数量为 0 的维度行，
 *   可用量仍由仓储盘盈 + 财务审批产生。
 * - 供给分配确认后采购单立即提交审批，状态为「审批中」，不会留下未提交草稿。
 * - 待办只在 /workspace 原地处理；供给分配嵌入 PurchaseOrderCreatePage。
 */
import { execFileSync } from "node:child_process"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { expect, test, type Browser, type BrowserContext, type Locator, type Page } from "@playwright/test"

import { ACCOUNTS } from "../helpers/accounts"
import { loginViaUi, newLoggedInContext } from "../helpers/login"

test.use({ viewport: { width: 1440, height: 900 } })

const API_BASE = process.env.API_BASE ?? "http://127.0.0.1:10001"
const FRONTEND_BASE = process.env.E2E_BASE_URL ?? "http://localhost:3000"
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

type AccountCred = {
    account: string
    password: string
    name?: string
}

type Session = {
    context: BrowserContext
    page: Page
}

function resolveAccount(loginName: string): AccountCred {
    const bag = ACCOUNTS as unknown as Record<string, unknown>
    const direct = bag[loginName]
    if (isCred(direct)) return direct
    for (const value of Object.values(bag)) {
        if (isCred(value) && value.account === loginName) return value
        if (value && typeof value === "object") {
            const nested = Object.values(value as Record<string, unknown>).find(
                (item) => isCred(item) && item.account === loginName,
            )
            if (isCred(nested)) return nested
        }
    }
    return { account: loginName, password: "123456" }
}

function isCred(value: unknown): value is AccountCred {
    return Boolean(
        value &&
            typeof value === "object" &&
            typeof (value as AccountCred).account === "string" &&
            typeof (value as AccountCred).password === "string",
    )
}

async function waitWorkspaceHome(page: Page) {
    if (!(await page.getByRole("heading", { name: "我的工作台" }).isVisible().catch(() => false))) {
        await page.goto(`${FRONTEND_BASE}/workspace`)
    }
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: TIMEOUT,
    })
}

async function openSession(browser: Browser, loginName: string): Promise<Session> {
    const cred = resolveAccount(loginName)
    try {
        const result = await newLoggedInContext(browser, cred as never)
        if (result && typeof result === "object" && "page" in result && "context" in result) {
            const session = result as Session
            await waitWorkspaceHome(session.page)
            return session
        }
    } catch {
        // 回退到 loginViaUi，兼容 helper 尚未封装独立 context 的情况。
    }
    const context = await browser.newContext()
    const page = await context.newPage()
    await page.goto(`${FRONTEND_BASE}/login`)
    await loginViaUi(page, cred as never)
    await waitWorkspaceHome(page)
    return { context, page }
}

async function closeSession(session: Session | undefined) {
    if (!session) return
    await session.context.close()
}

async function expectToast(page: Page, title: string | RegExp) {
    const toast = page.locator('[data-slot="toast"]').filter({ hasText: title }).first()
    await expect(toast).toBeVisible({ timeout: TIMEOUT })
}

async function chooseOption(page: Page, input: Locator, optionName: string | RegExp) {
    await input.click()
    const option = page.getByRole("option", { name: optionName })
    await expect(option).toBeVisible({ timeout: TIMEOUT })
    await option.click()
}

async function searchAndSubmit(input: Locator, query: string) {
    await input.fill(query)
    await input.press("Enter")
}

async function pickVisibleDay(page: Page, trigger: Locator, dayOfMonth: number) {
    await trigger.click()
    const dayButton = page.getByRole("button", { name: String(dayOfMonth), exact: true })
    await expect(dayButton).toBeVisible({ timeout: TIMEOUT })
    await dayButton.click()
}

async function openWorkspaceTask(
    page: Page,
    options: {
        name: string | RegExp
        family?: "审批" | "采购" | "履约"
        query?: string
    },
) {
    await page.goto(`${FRONTEND_BASE}/workspace`)
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: TIMEOUT,
    })
    if (options.family) {
        await page.getByRole("button", { name: new RegExp(`^${options.family}`) }).click()
    }
    if (options.query) {
        await searchAndSubmit(page.getByLabel("搜索待办"), options.query)
    }
    const task = page.getByRole("button", { name: options.name })
    await expect(task).toBeVisible({ timeout: TIMEOUT })
    await task.click()
    await expect(task).toHaveAttribute("aria-current", "true")
}

async function approveOpenTask(page: Page) {
    await page.getByRole("button", { name: "通过", exact: true }).click()
    const dialog = page.getByRole("dialog", { name: "确认通过" })
    await expect(dialog).toBeVisible({ timeout: TIMEOUT })
    await dialog.getByRole("button", { name: "确认通过" }).click()
    await expect(dialog).toBeHidden({ timeout: TIMEOUT })
}

function uniqueCreditCode(): string {
    const stamp = Date.now().toString()
    return `91E2E${stamp}`.replace(/[^0-9A-Za-z]/g, "0").padEnd(18, "0").slice(0, 18)
}

function futureDayOfMonth(): number {
    const date = new Date()
    date.setDate(date.getDate() + 21)
    return date.getDate()
}

function pdfUpload(): { name: string; mimeType: string; buffer: Buffer } | string {
    if (fs.existsSync(CONTRACT_PDF)) return CONTRACT_PDF
    return {
        name: "sample-contract.pdf",
        mimeType: "application/pdf",
        buffer: MINIMAL_PDF,
    }
}

type ApiEnvelope<T> = {
    success?: boolean
    data?: T
}

async function apiLogin(account: AccountCred): Promise<string> {
    const response = await fetch(`${API_BASE}/login`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
            account: account.account,
            password: account.password,
            account_kind: "admin",
        }),
    })
    const payload = (await response.json()) as ApiEnvelope<{ token?: string }>
    const token = payload.data?.token
    if (!response.ok || !token) {
        throw new Error(`API 登录失败: ${account.account}`)
    }
    return token
}

async function apiGet<T>(token: string, pathName: string): Promise<T> {
    const response = await fetch(`${API_BASE}${pathName}`, {
        headers: { Authorization: `Bearer ${token}` },
    })
    const payload = (await response.json()) as ApiEnvelope<T>
    if (!response.ok || payload.success === false) {
        throw new Error(`API GET ${pathName} 失败`)
    }
    return payload.data as T
}

function mongoSettings(): { uri: string; dbName: string } {
    const configPath = path.resolve(
        path.dirname(fileURLToPath(import.meta.url)),
        "../../backend/config.toml",
    )
    const raw = execFileSync(
        "python3",
        [
            "-c",
            "import pathlib, tomllib, json, sys; cfg=tomllib.loads(pathlib.Path(sys.argv[1]).read_bytes().decode()); print(json.dumps({'uri': cfg['database']['uri'], 'dbName': cfg['database']['db_name']}))",
            configPath,
        ],
        { encoding: "utf8" },
    ).trim()
    return JSON.parse(raw) as { uri: string; dbName: string }
}

/**
 * 盘盈入口绑定已有余额行。空库时只写入数量为 0 的维度，不把可用量当成期初库存。
 */
async function ensureZeroBalanceDimension() {
    const token = await apiLogin(resolveAccount("admin"))
    const warehouses = await apiGet<{ items?: Array<{ id: string; warehouse_code: string }> }>(
        token,
        `/admin/warehouses?warehouse_code=${encodeURIComponent(WAREHOUSE_CODE)}&page=1&page_size=20`,
    )
    const warehouse = (warehouses.items ?? []).find((row) => row.warehouse_code === WAREHOUSE_CODE)
    const skus = await apiGet<{ items?: Array<{ id: string; sku_no: string }> }>(
        token,
        `/admin/skus?page=1&page_size=100`,
    )
    const sku = (skus.items ?? []).find((row) => row.sku_no === SKU_NO)
    if (!warehouse || !sku) {
        throw new Error(`主数据缺少仓库 ${WAREHOUSE_CODE} 或 SKU ${SKU_NO}`)
    }
    const balances = await apiGet<{ items?: unknown[]; total?: number }>(
        token,
        `/admin/stock-balances?sku_id=${encodeURIComponent(sku.id)}&warehouse_id=${encodeURIComponent(warehouse.id)}&page=1&page_size=5`,
    )
    if ((balances.total ?? balances.items?.length ?? 0) > 0) return

    const now = Math.floor(Date.now() / 1000)
    const id = `${now.toString(16)}${"0".repeat(24)}`.slice(0, 24)
    const { uri, dbName } = mongoSettings()
    const script = `
      const db = db.getSiblingDB(${JSON.stringify(dbName)});
      db.stock_balances.insertOne({
        id: ${JSON.stringify(id)},
        version: NumberLong(1),
        created_at: NumberLong(${now}),
        updated_at: NumberLong(${now}),
        deleted_at: NumberLong(0),
        warehouse_id: ${JSON.stringify(warehouse.id)},
        sku_id: ${JSON.stringify(sku.id)},
        on_hand_quantity: NumberDecimal("0"),
        reserved_quantity: NumberDecimal("0"),
        available_quantity: NumberDecimal("0"),
        last_movement_id: null
      });
    `
    execFileSync("mongosh", ["--norc", "--quiet", uri, "--eval", script], {
        stdio: "pipe",
        timeout: 30_000,
    })
}

async function submitInventoryCountGain(page: Page) {
    await page.goto(`${FRONTEND_BASE}/inventory`)
    await expect(page.getByRole("heading", { name: "库存台账" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await searchAndSubmit(page.getByLabel("搜索库存"), SKU_NO)
    const warehouseRow = page.getByRole("row").filter({ hasText: WAREHOUSE_NAME })
    await expect(warehouseRow).toBeVisible({ timeout: TIMEOUT })
    await warehouseRow.getByRole("button", { name: "库存调整" }).click()
    const dialog = page.getByRole("dialog", { name: "发起库存调整" })
    await expect(dialog).toBeVisible({ timeout: TIMEOUT })
    await chooseOption(page, dialog.getByLabel("原因类型"), /盘盈/)
    await dialog.getByLabel(/调整数量/).fill(STOCK_QTY)
    await dialog.getByLabel("原因说明").fill("E2E flow-12 盘盈准备少于销售数量的库存")
    await dialog.getByRole("button", { name: "提交审批" }).click()
    const confirm = page.getByRole("alertdialog", { name: /确认提交库存调整/ })
    await expect(confirm).toBeVisible({ timeout: TIMEOUT })
    await confirm.getByRole("button", { name: "确认提交" }).click()
    await expect(page.getByText("调整已提交审批")).toBeVisible({ timeout: TIMEOUT })
}

async function assertAvailableQuantity(page: Page, quantity: string) {
    await page.goto(`${FRONTEND_BASE}/inventory`)
    await expect(page.getByRole("heading", { name: "库存台账" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await searchAndSubmit(page.getByLabel("搜索库存"), SKU_NO)
    await expect(page.getByText(WAREHOUSE_NAME)).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText(quantity, { exact: true }).or(page.getByText(`${quantity} `))).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(page.getByText("零可用")).toHaveCount(0)
}

async function createCustomer(page: Page, customerName: string, creditCode: string) {
    await page.goto(`${FRONTEND_BASE}/sales/customers`)
    await expect(page.getByRole("heading", { name: "客户中心" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.getByRole("button", { name: "新建客户" }).click()
    const dialog = page.getByRole("dialog", { name: "新建客户" })
    await expect(dialog).toBeVisible({ timeout: TIMEOUT })
    await dialog.getByLabel("法定名称").fill(customerName)
    await dialog.getByLabel("客户简称").fill("拆分配客户")
    await dialog.getByLabel("统一社会信用代码").fill(creditCode)
    await chooseOption(page, dialog.getByLabel("默认付款条件"), "按合同约定")
    await dialog.getByRole("button", { name: "创建客户" }).click()
    await expectToast(page, "客户已创建")
    await expect(dialog).toBeHidden({ timeout: TIMEOUT })
    await searchAndSubmit(page.getByLabel("搜索客户"), customerName)
    await expect(page.getByText(customerName)).toBeVisible({ timeout: TIMEOUT })
}

async function createSalesOrderWithContract(
    page: Page,
    customerName: string,
    contractNo: string,
): Promise<{ salesOrderId: string; salesOrderNo: string }> {
    await page.goto(`${FRONTEND_BASE}/sales/orders?mode=create`)
    await expect(page.getByText("销售明细")).toBeVisible({ timeout: TIMEOUT })
    await page.getByRole("button", { name: "上传合同 PDF" }).click()
    const upload = page.getByRole("dialog", { name: "上传合同 PDF" })
    await expect(upload).toBeVisible({ timeout: TIMEOUT })
    await upload.locator("#card-contracts-upload-pdf-input").setInputFiles(pdfUpload())
    await upload.getByLabel("合同编号").fill(contractNo)
    const customerInput = upload.getByLabel("客户")
    await customerInput.click()
    await customerInput.fill(customerName)
    await expect(page.getByRole("option", { name: new RegExp(customerName) })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.getByRole("option", { name: new RegExp(customerName) }).click()
    await expect(upload.getByLabel("结算主体")).not.toHaveValue("")
    await upload.getByRole("button", { name: "上传并归档" }).click()
    await expect(upload).toBeHidden({ timeout: TIMEOUT })
    await expect(page.getByText(customerName)).toBeVisible({ timeout: TIMEOUT })

    await chooseOption(page, page.getByLabel("福利场景"), "年节礼包")
    await page.getByRole("button", { name: "选择商品" }).click()
    const skuDialog = page.getByRole("dialog", { name: "选择商品" })
    await expect(skuDialog).toBeVisible({ timeout: TIMEOUT })
    await searchAndSubmit(skuDialog.getByPlaceholder("搜索 SKU、商品名称、编号或规格"), SKU_NO)
    await expect(skuDialog.getByText(SKU_NO)).toBeVisible({ timeout: TIMEOUT })
    await skuDialog.getByRole("checkbox", { name: new RegExp(SKU_NAME) }).check()
    await skuDialog.getByRole("button", { name: /加入所选/ }).click()
    await expect(skuDialog).toBeHidden({ timeout: TIMEOUT })
    await expect(page.getByText(SKU_NAME)).toBeVisible({ timeout: TIMEOUT })

    await page.getByLabel("数量").fill(SALES_QTY)
    await pickVisibleDay(page, page.locator("#sales-orders-create-batch-due-date"), futureDayOfMonth())
    await page.getByRole("button", { name: "应用到全部" }).click()
    await expectToast(page, "已批量设置交期")

    await page.getByRole("button", { name: "提交", exact: true }).click()
    const submit = page.getByRole("dialog", { name: "提交销售单" })
    await expect(submit).toBeVisible({ timeout: TIMEOUT })
    await submit.getByRole("button", { name: "确认提交" }).click()
    await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, { timeout: TIMEOUT })
    const salesOrderId = page.url().match(/\/sales\/orders\/([^/?]+)/)?.[1]
    if (!salesOrderId) throw new Error("未能从 URL 读取销售单身份")
    await expect(page.getByText("审批中")).toBeVisible({ timeout: TIMEOUT })
    const salesOrderNo = (await page.locator("span", { hasText: "单号" }).locator(".num").textContent())?.trim()
    if (!salesOrderNo) throw new Error("未能读取销售单号")
    return { salesOrderId, salesOrderNo }
}

async function confirmSplitAllocation(page: Page, salesOrderNo: string) {
    await openWorkspaceTask(page, {
        family: "采购",
        query: salesOrderNo,
        name: new RegExp(`待供给分配.*${salesOrderNo}|${salesOrderNo}`),
    })
    await expect(page.getByRole("heading", { name: "供给分配" }).or(page.getByText("销售明细与供给方案"))).toBeVisible({
        timeout: TIMEOUT,
    })
    if ((await page.locator("tr").filter({ hasText: "现有库存" }).count()) === 0) {
        await page.getByRole("button", { name: "重新自动分配" }).click()
        await expectToast(page, /已重新分配供给|没有可匹配的供给方案/)
    }
    await expect(page.getByText("现有库存")).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText(/入仓/)).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText("将建立库存预留")).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText("1 条")).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText("将创建采购单")).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText("1 张")).toBeVisible({ timeout: TIMEOUT })

    const stockRow = page.locator("tr").filter({ hasText: "现有库存" })
    const purchaseRow = page.locator("tr").filter({ hasText: /入仓/ })
    await stockRow.getByLabel("本次分配数量").fill("4")
    await purchaseRow.getByLabel("本次分配数量").fill(PURCHASE_QTY)
    await page.getByRole("button", { name: "预览供给分配" }).click()
    await expectToast(page, "无法预览供给分配")
    await expect(page.getByText(/拆分数量合计不能超过|库存分配合计不能超过/)).toBeVisible({
        timeout: TIMEOUT,
    })

    await stockRow.getByLabel("本次分配数量").fill(STOCK_QTY)
    await purchaseRow.getByLabel("本次分配数量").fill(PURCHASE_QTY)
    const warehouseInput = purchaseRow.getByLabel(/采购入库目标仓/)
    await warehouseInput.click()
    await warehouseInput.fill("通州")
    await expect(page.getByRole("option", { name: new RegExp(WAREHOUSE_NAME) })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.getByRole("option", { name: new RegExp(WAREHOUSE_NAME) }).click()

    await page.getByRole("button", { name: "预览供给分配" }).click()
    const preview = page.getByRole("dialog", { name: "预览供给分配" })
    await expect(preview).toBeVisible({ timeout: TIMEOUT })
    await expect(preview.getByText("现有库存分配")).toBeVisible({ timeout: TIMEOUT })
    await expect(preview.getByText(new RegExp(`${STOCK_QTY}`))).toBeVisible({ timeout: TIMEOUT })
    await expect(preview.getByText("采购单")).toBeVisible({ timeout: TIMEOUT })
    await preview.getByRole("button", { name: /确认库存分配并提交 1 张采购单/ }).click()
    const confirm = page.getByRole("alertdialog", { name: "确认供给分配" })
    await expect(confirm.getByText("确认供给分配")).toBeVisible({ timeout: TIMEOUT })
    await expect(confirm.getByText(new RegExp(`将建立 1 条库存预留，并为剩余缺口创建 1 张采购单`))).toBeVisible({
        timeout: TIMEOUT,
    })
    await confirm.getByRole("button", { name: "确认提交" }).click()
    await expectToast(page, "供给分配已完成")
    await expect(page.getByText(/已建立 1 条库存预留，并将缺口拆成 1 张采购单提交审批/)).toBeVisible({
        timeout: TIMEOUT,
    })
}

async function assertReservationAndPurchase(page: Page, salesOrderNo: string) {
    await page.goto(`${FRONTEND_BASE}/inventory?view=reservation`)
    await page.getByRole("tab", { name: "销售预占" }).click()
    await searchAndSubmit(page.getByLabel("搜索库存"), SKU_NO)
    await expect(page.getByText(salesOrderNo)).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText(new RegExp(`${STOCK_QTY} / ${STOCK_QTY}`))).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(page.getByText("有效")).toBeVisible({ timeout: TIMEOUT })

    await page.goto(`${FRONTEND_BASE}/procurement/orders`)
    await expect(page.getByRole("heading", { name: "采购单" })).toBeVisible({ timeout: TIMEOUT })
    await searchAndSubmit(page.getByLabel("搜索采购单"), salesOrderNo)
    await expect(page.getByText(salesOrderNo)).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText("审批中")).toBeVisible({ timeout: TIMEOUT })
    await expect(page.getByText("草稿")).toHaveCount(0)
    const poLinks = page.getByRole("link", { name: /打开采购单/ })
    await expect(poLinks).toHaveCount(1)
}

async function completeFulfillment(
    page: Page,
    salesOrderNo: string,
    kind: "入库" | "仓发",
    extra?: { quantity?: string; trackingNo?: string },
) {
    await page.goto(`${FRONTEND_BASE}/workspace`)
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.getByRole("button", { name: /^履约/ }).click()
    await searchAndSubmit(page.getByLabel("搜索待办"), salesOrderNo)
    const queue = page.getByRole("list", { name: "待办列表" }).getByRole("button")
    const count = await queue.count()
    expect(count).toBeGreaterThan(0)
    const wantInbound = kind === "入库"
    let matched = false
    for (let index = 0; index < count; index += 1) {
        await queue.nth(index).click()
        const inboundForm = page.getByLabel("入库表单").or(page.getByText("入库作业"))
        const shipForm = page.getByLabel("公司仓发表单").or(page.getByText("物流信息"))
        const inboundVisible = await inboundForm.isVisible().catch(() => false)
        const shipVisible = await shipForm.isVisible().catch(() => false)
        if ((wantInbound && inboundVisible) || (!wantInbound && shipVisible)) {
            matched = true
            break
        }
    }
    expect(matched, `工作台未找到销售单 ${salesOrderNo} 的${kind}任务`).toBe(true)
    if (kind === "入库") {
        await expect(page.getByLabel("入库表单").or(page.getByText("入库作业"))).toBeVisible({
            timeout: TIMEOUT,
        })
        if (extra?.quantity) {
            await page.getByLabel("到货数量").fill(extra.quantity)
        }
        const quality = page.getByLabel("质量结果")
        if (await quality.count()) {
            const current = await quality.inputValue()
            if (!current) await chooseOption(page, quality, "合格")
        }
        await page.getByRole("button", { name: "确认入库" }).click()
        const confirm = page.getByRole("alertdialog", { name: "确认入库？" })
        await expect(confirm).toBeVisible({ timeout: TIMEOUT })
        await confirm.getByRole("button", { name: "确认入库" }).click()
        await expect(page.getByText("已入库")).toBeVisible({ timeout: TIMEOUT })
        return
    }

    await expect(page.getByLabel("公司仓发表单").or(page.getByText("物流信息"))).toBeVisible({
        timeout: TIMEOUT,
    })
    await chooseOption(page, page.getByLabel("承运方"), "顺丰速运")
    await page.getByLabel("物流单号").fill(extra?.trackingNo ?? `SF12-${Date.now()}`)
    if (extra?.quantity) {
        await page.getByLabel("本次发货数量").fill(extra.quantity)
    }
    await page.getByRole("button", { name: "确认发货" }).click()
    const confirm = page.getByRole("alertdialog", { name: "确认发货？" })
    await expect(confirm).toBeVisible({ timeout: TIMEOUT })
    await confirm.getByRole("button", { name: "确认发货" }).click()
    await expect(page.getByText("已发货")).toBeVisible({ timeout: TIMEOUT })
}

async function registerAcceptance(page: Page, salesOrderNo: string) {
    await openWorkspaceTask(page, {
        family: "履约",
        query: salesOrderNo,
        name: new RegExp(`客户验收登记.*${salesOrderNo}|${salesOrderNo}`),
    })
    await page.getByRole("button", { name: "登记客户验收" }).click()
    const register = page.getByRole("dialog", { name: "登记客户验收" })
    await expect(register).toBeVisible({ timeout: TIMEOUT })
    await register.getByRole("button", { name: /全部通过并确认|确认本次验收/ }).click()
    const confirm = page.getByRole("alertdialog", { name: "确认客户验收" })
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

    // 1. 销售创建客户（合同在建单时上传）
    const sales = await openSession(browser, "xiaoshou")
    try {
        await createCustomer(sales.page, customerName, creditCode)
    } finally {
        await closeSession(sales)
    }

    // 2. 仓储盘盈少于销售数量的库存
    await ensureZeroBalanceDimension()
    const warehouse = await openSession(browser, "cangchu")
    try {
        await submitInventoryCountGain(warehouse.page)
    } finally {
        await closeSession(warehouse)
    }

    // 3. 财务审批库存调整，可用量生效
    const financeAdj = await openSession(browser, "caiwu")
    try {
        await openWorkspaceTask(financeAdj.page, {
            family: "审批",
            name: /库存调整单审批/,
        })
        await approveOpenTask(financeAdj.page)
    } finally {
        await closeSession(financeAdj)
    }

    const warehouseCheck = await openSession(browser, "cangchu")
    try {
        await assertAvailableQuantity(warehouseCheck.page, STOCK_QTY)
    } finally {
        await closeSession(warehouseCheck)
    }

    // 4. 销售开单：数量大于现有库存
    const salesOrder = await openSession(browser, "xiaoshou")
    try {
        const created = await createSalesOrderWithContract(salesOrder.page, customerName, contractNo)
        salesOrderId = created.salesOrderId
        salesOrderNo = created.salesOrderNo
        expect(salesOrderId).toBeTruthy()
    } finally {
        await closeSession(salesOrder)
    }

    // 5. 采购审批销售单（选源不在本节点）
    const procurement = await openSession(browser, "caigou")
    try {
        await openWorkspaceTask(procurement.page, {
            family: "审批",
            query: salesOrderNo,
            name: new RegExp(`销售单审批.*${salesOrderNo}`),
        })
        await expect(procurement.page.getByText(/供给|库存|采购成本|履约方案/)).toHaveCount(0)
        await approveOpenTask(procurement.page)

        // 6. 供给分配：同一明细拆成现有库存 + 采购缺口；负向超分配必须被拦住
        await confirmSplitAllocation(procurement.page, salesOrderNo)
        await assertReservationAndPurchase(procurement.page, salesOrderNo)
    } finally {
        await closeSession(procurement)
    }

    // 7. 财务审批采购单（创建时已提交，不得再走草稿送审）
    const financePo = await openSession(browser, "caiwu")
    try {
        await openWorkspaceTask(financePo.page, {
            family: "审批",
            query: salesOrderNo,
            name: /采购单审批/,
        })
        await approveOpenTask(financePo.page)
    } finally {
        await closeSession(financePo)
    }

    // 8. 仓发第一段（库存直配预占）+ 采购入库沿销售分配预占 + 仓发第二段
    const fulfillment = await openSession(browser, "cangchu")
    try {
        await completeFulfillment(fulfillment.page, salesOrderNo, "仓发", {
            quantity: STOCK_QTY,
            trackingNo: `SF-STOCK-${Date.now()}`,
        })
        await completeFulfillment(fulfillment.page, salesOrderNo, "入库", {
            quantity: PURCHASE_QTY,
        })
        await fulfillment.page.goto(`${FRONTEND_BASE}/inventory?view=reservation`)
        await fulfillment.page.getByRole("tab", { name: "销售预占" }).click()
        await searchAndSubmit(fulfillment.page.getByLabel("搜索库存"), salesOrderNo)
        await expect(fulfillment.page.getByText(salesOrderNo)).toBeVisible({ timeout: TIMEOUT })
        await expect(fulfillment.page.getByText(new RegExp(PURCHASE_QTY))).toBeVisible({
            timeout: TIMEOUT,
        })
        await completeFulfillment(fulfillment.page, salesOrderNo, "仓发", {
            quantity: PURCHASE_QTY,
            trackingNo: `SF-PO-${Date.now()}`,
        })
    } finally {
        await closeSession(fulfillment)
    }

    // 9. 销售验收：两段履约待验合计等于销售明细
    const acceptance = await openSession(browser, "xiaoshou")
    try {
        await registerAcceptance(acceptance.page, salesOrderNo)
        await acceptance.page.goto(`${FRONTEND_BASE}/sales/orders/${salesOrderId}`)
        await expect(acceptance.page.getByText(salesOrderNo)).toBeVisible({ timeout: TIMEOUT })
        await expect(acceptance.page.getByText(/已验收|通过/)).toBeVisible({ timeout: TIMEOUT })
        await expect(acceptance.page.getByRole("button", { name: "登记客户验收" })).toHaveCount(0)
    } finally {
        await closeSession(acceptance)
    }
})
