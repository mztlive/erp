/**
 * 流程: [flow-10] 库存调整：盘盈、盘亏、损坏与驳回
 * 文档: docs/erp-phase-1.md §6.5.5；审批政策 docs/approval-workflow-contract.md §4.3/§4.4；
 *       工作台原地处理 docs/workbench-workitem-contract.md 第 3 节
 * 账号: cangchu（仓储提交）→ caiwu（财务审批成本影响）；禁止 caiwu 自己提交
 *
 * 文档-代码差异（以代码为准）：
 * 1. 文档/术语表要求资金库存终态用「已确认入账」，不用「过账」；库存调整状态徽标映射为「已过账」。
 * 2. 用户流程写「caiwu 审批通过 → 确认入账」；代码里最终通过即 `on_final_approve=post_stock_adjustment`，
 *    HTTP `/admin/stock-adjustments/{id}/post` 恒返回冲突，页面没有独立「确认入账」按钮。
 * 3. 文档写盘盈可登记事实；代码创建/过账都要求已有 stock_balance，「请先建立期初或入库」。
 *    空台账入口是「前往导入与期初」。本流程用 0 数量占位余额只为打开「库存调整」按钮，
 *    正式数量变化全部走盘盈/盘亏/损坏 UI。
 * 4. 流水类型后端有盘盈/盘亏/损坏；前端全部映射为「库存调整」。
 * 5. 调整单列表 `postedAt` 未从 DTO 映射，列「确认入账」可能一直是「—」，入账以状态「已过账」和流水为准。
 *
 * helpers 约定（loginViaUi / newLoggedInContext / apiLogin / apiGet / ACCOUNTS）：
 *   newLoggedInContext(browser, 登录名) => { context, page }
 *   loginViaUi(page, 登录名)
 *   apiLogin(登录名) => JWT
 *   apiGet(token, path, query?) => 已解包 data
 *   ACCOUNTS.cangchu.account 等（缺省回落到登录名）
 */
import { execFileSync } from "node:child_process"
import { randomUUID } from "node:crypto"
import { readFileSync } from "node:fs"
import path from "node:path"

import { expect, test, type Browser, type Locator, type Page } from "@playwright/test"

import { ACCOUNTS } from "../helpers/accounts"
import { apiGet, apiLogin } from "../helpers/api"
import { loginViaUi, newLoggedInContext } from "../helpers/login"
import "../helpers/ui"

test.describe.configure({ mode: "serial" })

const VISIBLE = { timeout: 20_000 } as const
const SKU_NO = "TEA-SF-LJ-250"
const SKU_NAME = "狮峰明前龙井礼盒 250g"
const WAREHOUSE_CODE = "BJ-TZ-01"
const WAREHOUSE_NAME = "北京通州仓"
const GAIN_QTY = "100"
const LOSS_QTY = "10"
const DAMAGE_QTY = "5"
const REJECT_QTY = "8"
const APPROVAL_NODE = "财务审批成本影响"

type ApiPage<T> = {
    items?: T[]
    total?: number
}

type StockBalance = {
    id: string
    warehouse_id: string
    warehouse_code?: string
    warehouse_name?: string
    sku_id: string
    sku_code?: string
    sku_name?: string
    on_hand_quantity: string
    available_quantity: string
    version?: number
}

type StockMovement = {
    id: string
    movement_type: string
    direction: string
    quantity: string
    source_document_id?: string
    source_document_no?: string | null
}

type StockAdjustment = {
    id: string
    adjustment_no: string
    reason_type: string
    status: string
}

type Session = { context: { close(): Promise<void> }; page: Page }

function loginName(kind: "cangchu" | "caiwu" | "caigou" | "admin"): string {
    const bag = ACCOUNTS as Record<
        string,
        { account?: string; username?: string } | string
    >
    const aliases: Record<string, string[]> = {
        cangchu: ["cangchu", "warehouse"],
        caiwu: ["caiwu", "finance"],
        caigou: ["caigou", "procurement"],
        admin: ["admin"],
    }
    for (const key of aliases[kind] ?? [kind]) {
        const row = bag[key]
        if (typeof row === "string" && row.trim()) return row
        if (row && typeof row === "object") {
            const name = row.account ?? row.username
            if (name?.trim()) return name
        }
    }
    return kind
}

function qtyOf(value: string | number | undefined | null): number {
    const parsed = Number.parseFloat(String(value ?? "0"))
    return Number.isFinite(parsed) ? parsed : 0
}

async function openSession(
    browser: Browser,
    kind: "cangchu" | "caiwu" | "caigou" | "admin",
): Promise<Session> {
    return newLoggedInContext(browser, loginName(kind))
}

async function closeSession(session: Session | undefined): Promise<void> {
    if (!session) return
    await session.context.close()
}

async function expectHeading(page: Page, name: string | RegExp): Promise<void> {
    await expect(page.getByRole("heading", { name })).toBeVisible(VISIBLE)
}

async function gotoInventory(page: Page): Promise<void> {
    const nav = page.getByRole("link", { name: "库存台账" })
    if (await nav.isVisible().catch(() => false)) {
        await nav.click()
    } else {
        await page.goto("/inventory")
    }
    await expectHeading(page, "库存台账")
}

async function gotoWorkspace(page: Page): Promise<void> {
    const nav = page.getByRole("link", { name: "我的工作台" })
    if (await nav.isVisible().catch(() => false)) {
        await nav.click()
    } else {
        await page.goto("/workspace")
    }
    await expectHeading(page, "我的工作台")
}

async function searchInventorySku(page: Page): Promise<void> {
    const search = page.getByLabel("搜索库存")
    await search.fill(SKU_NO)
    await search.press("Enter")
    await expect(page.getByText(SKU_NO)).toBeVisible(VISIBLE)
}

function skuBalanceRow(page: Page): Locator {
    return page.getByRole("row").filter({ hasText: SKU_NO }).filter({
        hasText: new RegExp(`${WAREHOUSE_NAME}|${WAREHOUSE_CODE}`),
    })
}

async function openBalanceTab(page: Page): Promise<void> {
    await page.getByRole("tab", { name: "余额" }).click()
    await expect(page.locator("#inventory-ledger-balance-table")).toBeVisible(
        VISIBLE,
    )
}

async function refreshLedger(page: Page): Promise<void> {
    await page.getByRole("button", { name: "刷新" }).click()
}

async function selectReasonType(page: Page, optionLabel: string): Promise<void> {
    const combo = page.locator("#inventory-adjustment-dialog-reason-type")
    await expect(combo).toBeVisible(VISIBLE)
    await combo.click()
    await page.getByRole("option", { name: optionLabel }).click()
}

async function submitStockAdjustment(
    page: Page,
    input: { reasonLabel: string; quantity: string; note: string },
): Promise<string> {
    await expectHeading(page, "发起库存调整")
    await selectReasonType(page, input.reasonLabel)
    await page.getByLabel(/调整数量/).fill(input.quantity)
    await page.getByLabel("原因说明").fill(input.note)
    await page.getByRole("button", { name: "提交审批" }).click()
    await expectHeading(page, "确认提交库存调整")
    await expect(page.getByText("确认后启动审批。余额在审批通过前不会变化。")).toBeVisible(
        VISIBLE,
    )
    await page.getByRole("button", { name: "确认提交" }).click()
    await expect(page.getByText("调整已提交审批")).toBeVisible(VISIBLE)
    const banner = page.getByText(/单号\s+\S+/)
    await expect(banner).toBeVisible(VISIBLE)
    const text = (await banner.innerText()).replace(/\s+/g, " ")
    const matched = text.match(/单号\s+(\S+)/)
    const adjustmentNo = matched?.[1]?.replace(/[。．.]+$/, "") ?? ""
    expect(adjustmentNo.length, `未能从提交结果解析单号：${text}`).toBeGreaterThan(2)
    await expect(page.getByText(APPROVAL_NODE)).toBeVisible(VISIBLE)
    await expect(page.getByRole("dialog")).toHaveCount(0)
    return adjustmentNo
}

async function startAdjustmentFromBalance(page: Page): Promise<void> {
    await openBalanceTab(page)
    await searchInventorySku(page)
    const row = skuBalanceRow(page)
    await expect(row).toBeVisible(VISIBLE)
    await row.getByRole("button", { name: "库存调整" }).click()
    await expectHeading(page, "发起库存调整")
}

async function searchWorkspaceTask(page: Page, documentNo: string): Promise<void> {
    await gotoWorkspace(page)
    const search = page.getByLabel(/搜索待办|搜索我发起的审批/)
    await search.fill(documentNo)
    await search.press("Enter")
    const task = page.getByRole("button", {
        name: new RegExp(`库存调整单审批[\\s\\S]*${documentNo}|${documentNo}`),
    })
    await expect(task).toBeVisible(VISIBLE)
    await task.click()
    await expect(page.getByText(documentNo)).toBeVisible(VISIBLE)
}

async function decideCurrentTask(
    page: Page,
    decision: "approve" | "reject",
    reason: string,
): Promise<void> {
    if (decision === "approve") {
        await page.getByRole("button", { name: "通过", exact: true }).click()
        await expectHeading(page, "确认通过")
        const reasonBox = page.getByLabel("原因（可选）")
        if (await reasonBox.isVisible().catch(() => false)) {
            await reasonBox.fill(reason)
        }
        await page.getByRole("button", { name: "确认通过" }).click()
    } else {
        await page.getByRole("button", { name: "驳回", exact: true }).click()
        await expectHeading(page, "确认驳回")
        await page.getByLabel("驳回原因").fill(reason)
        await page.getByRole("button", { name: "确认驳回" }).click()
    }
    await expect(page.getByRole("heading", { name: /确认通过|确认驳回/ })).toHaveCount(
        0,
        VISIBLE,
    )
}

async function readOnHand(): Promise<number> {
    const row = findTargetBalance(await listBalances(await tokenOf("cangchu")))
    expect(row, `未找到 ${WAREHOUSE_CODE} / ${SKU_NO} 余额`).toBeTruthy()
    return qtyOf(row?.on_hand_quantity)
}

async function expectOnHandUi(page: Page, expected: number): Promise<void> {
    await openBalanceTab(page)
    await searchInventorySku(page)
    const row = skuBalanceRow(page)
    await expect(row).toBeVisible(VISIBLE)
    // SKU 名称含「250g」，不能用 toContainText("5"/"0") 对数量做子串匹配。
    if (expected === 0) {
        await expect(row).toContainText("零可用")
    } else {
        await expect(row).toContainText("有可用")
    }
}

async function tokenOf(kind: "cangchu" | "caiwu" | "caigou" | "admin"): Promise<string> {
    return apiLogin(loginName(kind))
}

async function listBalances(token: string): Promise<StockBalance[]> {
    const page = await apiGet<ApiPage<StockBalance>>(token, "/admin/stock-balances", {
        page: 1,
        page_size: 100,
    })
    return page.items ?? []
}

async function listMovements(token: string): Promise<StockMovement[]> {
    const page = await apiGet<ApiPage<StockMovement>>(token, "/admin/stock-movements", {
        page: 1,
        page_size: 100,
        sort_by: "occurred_at",
        sort_dir: "desc",
    })
    return page.items ?? []
}

async function listAdjustments(token: string): Promise<StockAdjustment[]> {
    const page = await apiGet<ApiPage<StockAdjustment>>(
        token,
        "/admin/stock-adjustments",
        { page: 1, page_size: 100, sort_by: "created_at", sort_dir: "desc" },
    )
    return page.items ?? []
}

function findTargetBalance(rows: StockBalance[]): StockBalance | undefined {
    const skuRows = rows.filter(
        (row) =>
            row.sku_code === SKU_NO ||
            (row.sku_name ?? "").includes(SKU_NAME.slice(0, 6)),
    )
    return (
        skuRows.find(
            (row) =>
                row.warehouse_code === WAREHOUSE_CODE ||
                row.warehouse_name === WAREHOUSE_NAME,
        ) ?? skuRows[0]
    )
}

function parseTomlString(text: string, key: string): string {
    const matched = text.match(new RegExp(`^${key}\\s*=\\s*"(.*)"`, "m"))
    return matched?.[1] ?? ""
}

function seedZeroBalanceViaMongosh(warehouseId: string, skuId: string): void {
    const configPath = path.join(process.cwd(), "backend", "config.toml")
    const toml = readFileSync(configPath, "utf8")
    const uri = parseTomlString(toml, "uri")
    const dbName = parseTomlString(toml, "db_name") || "erp"
    if (!uri) {
        throw new Error("backend/config.toml 缺少 database.uri，无法写入 0 数量占位余额")
    }
    const id = randomUUID().replace(/-/g, "")
    const now = Math.floor(Date.now() / 1000)
    const script = `
      const dbx = db.getSiblingDB(${JSON.stringify(dbName)});
      const existing = dbx.stock_balances.findOne({
        warehouse_id: ${JSON.stringify(warehouseId)},
        sku_id: ${JSON.stringify(skuId)},
        deleted_at: 0
      });
      if (!existing) {
        dbx.stock_balances.insertOne({
          id: ${JSON.stringify(id)},
          version: NumberLong("1"),
          created_at: NumberLong(${JSON.stringify(String(now))}),
          updated_at: NumberLong(${JSON.stringify(String(now))}),
          deleted_at: NumberLong("0"),
          warehouse_id: ${JSON.stringify(warehouseId)},
          sku_id: ${JSON.stringify(skuId)},
          on_hand_quantity: NumberDecimal("0"),
          reserved_quantity: NumberDecimal("0"),
          available_quantity: NumberDecimal("0"),
          last_movement_id: null
        });
      }
    `
    execFileSync("mongosh", [uri, "--quiet", "--eval", script], {
        stdio: ["ignore", "pipe", "pipe"],
        timeout: 30_000,
    })
}

async function ensurePhysicalBalanceRow(): Promise<StockBalance> {
    const token = await tokenOf("cangchu")
    const existing = findTargetBalance(await listBalances(token))
    if (existing) return existing

    const warehouses = await apiGet<ApiPage<{ id: string; warehouse_code?: string }>>(
        token,
        "/admin/warehouses",
        { page: 1, page_size: 50, sort_by: "warehouse_code", sort_dir: "asc" },
    )
    const warehouse = (warehouses.items ?? []).find(
        (row) => row.warehouse_code === WAREHOUSE_CODE,
    )
    const skus = await apiGet<ApiPage<{ id: string; sku_no?: string; name?: string }>>(
        token,
        "/admin/skus",
        { q: SKU_NO, page: 1, page_size: 20, sort_by: "sku_no", sort_dir: "asc" },
    )
    const sku = (skus.items ?? []).find((row) => row.sku_no === SKU_NO)
    expect(warehouse, `未找到仓库 ${WAREHOUSE_CODE}`).toBeTruthy()
    expect(sku, `未找到 SKU ${SKU_NO}`).toBeTruthy()
    seedZeroBalanceViaMongosh(warehouse!.id, sku!.id)

    let created: StockBalance | undefined
    await expect
        .poll(async () => {
            created = findTargetBalance(await listBalances(token))
            return created
        }, VISIBLE)
        .toBeTruthy()
    expect(created, "写入 0 数量占位余额后仍未出现库存行").toBeTruthy()
    return created as StockBalance
}

async function expectEmptyPurchaseAndFulfillment(): Promise<void> {
    const warehouseToken = await tokenOf("cangchu")
    const procurementToken = await tokenOf("caigou")
    const purchaseOrders = await apiGet<ApiPage<unknown>>(
        procurementToken,
        "/admin/purchase-orders",
        { page: 1, page_size: 20 },
    )
    expect(purchaseOrders.total ?? (purchaseOrders.items ?? []).length).toBe(0)

    const receipts = await apiGet<ApiPage<unknown>>(
        warehouseToken,
        "/admin/purchase-receipts",
        { page: 1, page_size: 20 },
    )
    expect(receipts.total ?? (receipts.items ?? []).length).toBe(0)

    const deliveries = await apiGet<ApiPage<unknown>>(
        warehouseToken,
        "/admin/deliveries",
        { page: 1, page_size: 20 },
    )
    expect(deliveries.total ?? (deliveries.items ?? []).length).toBe(0)
}

async function expectCaiwuCannotSubmit(browser: Browser): Promise<void> {
    const context = await browser.newContext()
    const page = await context.newPage()
    try {
        await loginViaUi(page, loginName("caiwu"))
        await expectHeading(page, "我的工作台")
        await expect(page.getByRole("link", { name: "库存台账" })).toHaveCount(0)
        await page.goto("/inventory")
        await expect(
            page.getByText(/当前角色未配置仓库数据范围|权限已收回/),
        ).toBeVisible(VISIBLE)
        await expect(page.getByRole("button", { name: "库存调整" })).toHaveCount(0)
        await expect(page.getByRole("button", { name: "发起库存调整" })).toHaveCount(0)
        await expect(page.getByRole("button", { name: "提交审批" })).toHaveCount(0)
    } finally {
        await context.close()
    }

    const financeToken = await tokenOf("caiwu")
    const apiBase = process.env.API_BASE || "http://127.0.0.1:10001"
    const createRes = await fetch(`${apiBase}/admin/stock-adjustments`, {
        method: "POST",
        headers: {
            Authorization: `Bearer ${financeToken}`,
            "Content-Type": "application/json",
        },
        body: JSON.stringify({
            adjustment_no: `TZ-FORBIDDEN-${Date.now()}`,
            warehouse_id: "x",
            reason_type: "STOCK_GAIN",
            lines: [{ sku_id: "x", quantity: "1", direction: "INCREASE" }],
        }),
    })
    expect(createRes.status, "caiwu 不得创建库存调整单").toBeGreaterThanOrEqual(400)
}

test("库存调整：盘盈、盘亏、损坏入账与驳回", async ({ browser }) => {
    test.setTimeout(240_000)

    // 0. 财务不得自己提交库存调整（岗位分离）
    await expectCaiwuCannotSubmit(browser)
    await expectEmptyPurchaseAndFulfillment()

    await ensurePhysicalBalanceRow()

    // 1. 仓储盘盈 100 → 提交审批（库存尚未变化）
    let warehouse = await openSession(browser, "cangchu")
    await gotoInventory(warehouse.page)
    const openingOnHand = await readOnHand()
    await expectOnHandUi(warehouse.page, openingOnHand)
    const openingMovements = await listMovements(await tokenOf("cangchu"))
    await startAdjustmentFromBalance(warehouse.page)
    const gainNo = await submitStockAdjustment(warehouse.page, {
        reasonLabel: "盘盈（增加）",
        quantity: GAIN_QTY,
        note: "盘盈入库，准备可用量",
    })
    await refreshLedger(warehouse.page)
    expect(await readOnHand()).toBe(openingOnHand)
    await expectOnHandUi(warehouse.page, openingOnHand)
    await closeSession(warehouse)

    // 2. 财务通过 → 系统确认入账（无独立入账按钮）
    let finance = await openSession(browser, "caiwu")
    await searchWorkspaceTask(finance.page, gainNo)
    await expect(finance.page.getByText("库存调整单审批")).toBeVisible(VISIBLE)
    await expect(finance.page.getByText(APPROVAL_NODE)).toBeVisible(VISIBLE)
    await expect(finance.page.getByText("第 1 轮")).toBeVisible(VISIBLE)
    await decideCurrentTask(finance.page, "approve", "核对盘盈数量无误")
    await closeSession(finance)

    warehouse = await openSession(browser, "cangchu")
    await gotoInventory(warehouse.page)
    const afterGain = openingOnHand + qtyOf(GAIN_QTY)
    await expect.poll(async () => readOnHand(), VISIBLE).toBe(afterGain)
    await expectOnHandUi(warehouse.page, afterGain)
    await warehouse.page.getByRole("tab", { name: "流水" }).click()
    const movementTable = warehouse.page.locator("#inventory-ledger-movement-table")
    await expect(movementTable).toBeVisible(VISIBLE)
    await expect(movementTable).toContainText("库存调整")
    await expect(movementTable).toContainText("增加")
    await warehouse.page.getByRole("tab", { name: "调整记录" }).click()
    const adjustmentTable = warehouse.page.locator("#inventory-ledger-adjustment-table")
    await expect(adjustmentTable).toBeVisible(VISIBLE)
    await expect(adjustmentTable).toContainText(gainNo)
    await expect(adjustmentTable).toContainText("盘盈")
    await expect(adjustmentTable).toContainText("已过账")
    await closeSession(warehouse)

    const afterGainMovements = await listMovements(await tokenOf("cangchu"))
    expect(afterGainMovements.length).toBe(openingMovements.length + 1)
    expect(afterGainMovements.some((row) => row.movement_type === "STOCK_GAIN")).toBe(
        true,
    )

    // 3. 盘亏 10 → 审批通过 → 库存减少，原盘盈流水保留
    warehouse = await openSession(browser, "cangchu")
    await gotoInventory(warehouse.page)
    await startAdjustmentFromBalance(warehouse.page)
    const lossNo = await submitStockAdjustment(warehouse.page, {
        reasonLabel: "盘亏（减少）",
        quantity: LOSS_QTY,
        note: "盘点短少，登记盘亏",
    })
    await closeSession(warehouse)

    finance = await openSession(browser, "caiwu")
    await searchWorkspaceTask(finance.page, lossNo)
    await decideCurrentTask(finance.page, "approve", "核对盘亏数量")
    await closeSession(finance)

    warehouse = await openSession(browser, "cangchu")
    await gotoInventory(warehouse.page)
    const afterLoss = afterGain - qtyOf(LOSS_QTY)
    await expect.poll(async () => readOnHand(), VISIBLE).toBe(afterLoss)
    await expectOnHandUi(warehouse.page, afterLoss)
    await warehouse.page.getByRole("tab", { name: "流水" }).click()
    await expect(warehouse.page.locator("#inventory-ledger-movement-table")).toContainText(
        "减少",
    )
    await warehouse.page.getByRole("tab", { name: "调整记录" }).click()
    await expect(warehouse.page.locator("#inventory-ledger-adjustment-table")).toContainText(
        lossNo,
    )
    await expect(warehouse.page.locator("#inventory-ledger-adjustment-table")).toContainText(
        "盘亏",
    )
    await closeSession(warehouse)

    const afterLossMovements = await listMovements(await tokenOf("cangchu"))
    expect(afterLossMovements.length).toBe(afterGainMovements.length + 1)
    expect(afterLossMovements.some((row) => row.movement_type === "STOCK_GAIN")).toBe(
        true,
    )
    expect(afterLossMovements.some((row) => row.movement_type === "STOCK_LOSS")).toBe(
        true,
    )

    // 4. 损坏 5 → 审批通过 → 库存再减，原出入库记录仍在
    warehouse = await openSession(browser, "cangchu")
    await gotoInventory(warehouse.page)
    await startAdjustmentFromBalance(warehouse.page)
    const damageNo = await submitStockAdjustment(warehouse.page, {
        reasonLabel: "损坏（减少）",
        quantity: DAMAGE_QTY,
        note: "仓储损坏报损",
    })
    await closeSession(warehouse)

    finance = await openSession(browser, "caiwu")
    await searchWorkspaceTask(finance.page, damageNo)
    await decideCurrentTask(finance.page, "approve", "核对损坏报损")
    await closeSession(finance)

    warehouse = await openSession(browser, "cangchu")
    await gotoInventory(warehouse.page)
    const afterDamage = afterLoss - qtyOf(DAMAGE_QTY)
    await expect.poll(async () => readOnHand(), VISIBLE).toBe(afterDamage)
    await expectOnHandUi(warehouse.page, afterDamage)
    await warehouse.page.getByRole("tab", { name: "调整记录" }).click()
    await expect(warehouse.page.locator("#inventory-ledger-adjustment-table")).toContainText(
        damageNo,
    )
    await expect(warehouse.page.locator("#inventory-ledger-adjustment-table")).toContainText(
        "损坏",
    )
    await closeSession(warehouse)

    const afterDamageMovements = await listMovements(await tokenOf("cangchu"))
    expect(afterDamageMovements.length).toBe(afterLossMovements.length + 1)
    expect(afterDamageMovements.some((row) => row.movement_type === "DAMAGE")).toBe(true)
    expect(afterDamageMovements.map((row) => row.id).sort()).toEqual(
        expect.arrayContaining(afterLossMovements.map((row) => row.id).sort()),
    )

    // 5. 再开一张盘盈并驳回：轮次加一回到首节点，库存与流水不变
    warehouse = await openSession(browser, "cangchu")
    await gotoInventory(warehouse.page)
    await startAdjustmentFromBalance(warehouse.page)
    const rejectNo = await submitStockAdjustment(warehouse.page, {
        reasonLabel: "盘盈（增加）",
        quantity: REJECT_QTY,
        note: "拟盘盈后由财务驳回，验证库存不变",
    })
    await closeSession(warehouse)

    finance = await openSession(browser, "caiwu")
    await searchWorkspaceTask(finance.page, rejectNo)
    await decideCurrentTask(finance.page, "reject", "数量依据不足，驳回重报")
    await searchWorkspaceTask(finance.page, rejectNo)
    await expect(finance.page.getByText("第 2 轮")).toBeVisible(VISIBLE)
    await expect(finance.page.getByText(APPROVAL_NODE)).toBeVisible(VISIBLE)
    await expect(finance.page.getByText(/最近驳回/)).toBeVisible(VISIBLE)
    await expect(finance.page.getByText("数量依据不足，驳回重报")).toBeVisible(VISIBLE)
    await closeSession(finance)

    warehouse = await openSession(browser, "cangchu")
    await gotoInventory(warehouse.page)
    expect(await readOnHand()).toBe(afterDamage)
    await expectOnHandUi(warehouse.page, afterDamage)
    await warehouse.page.getByRole("tab", { name: "调整记录" }).click()
    const rejectRow = warehouse.page
        .locator("#inventory-ledger-adjustment-table")
        .getByRole("row")
        .filter({ hasText: rejectNo })
    await expect(rejectRow).toBeVisible(VISIBLE)
    await expect(rejectRow).toContainText("审批中")
    await rejectRow.click()
    await expect(warehouse.page.getByText("第 2 轮")).toBeVisible(VISIBLE)
    await expect(warehouse.page.getByRole("button", { name: "关闭" })).toBeVisible(
        VISIBLE,
    )
    await warehouse.page.locator("#inventory-adjustment-detail-close").click()
    await closeSession(warehouse)

    const afterRejectMovements = await listMovements(await tokenOf("cangchu"))
    expect(afterRejectMovements.length).toBe(afterDamageMovements.length)
    const posted = await listAdjustments(await tokenOf("cangchu"))
    expect(posted.filter((row) => row.adjustment_no === rejectNo)[0]?.status).toBe(
        "IN_APPROVAL",
    )

    await expectEmptyPurchaseAndFulfillment()
    const instances = await apiGet<ApiPage<{ document_type?: string; status?: string }>>(
        await tokenOf("caiwu"),
        "/admin/approval-instances",
        { view: "mine", document_type: "stock_adjustment", limit: 20 },
    )
    expect((instances.items ?? []).length).toBeGreaterThan(0)
})
