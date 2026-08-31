/**
 * 流程: [flow-02] 供应商直接发客户（代发）
 * 文档: docs/erp-phase-1.md §7.3.2 + §7.4（前段对齐 §7.3.1 到供给分配）
 *
 * 使用账号:
 *   admin    采购责任规则（默认调度人 → caigou）
 *   xiaoshou 客户 / 合同 / 销售单 / 客户验收
 *   caigou   销售单采购确认、供给分配（选供应商直发）、代发履约
 *   caiwu    采购单财务审批
 *   fukuan   若种子供应商为先款，则在付款任务确认入账
 *   cangchu  负向：不得出现采购入库履约任务
 *
 * 文档-代码差异（以代码为准）:
 *   1. 文档 §7.3.1 在采购确认节点「按供应商创建采购单并选源」；
 *      代码：采购确认只通过/驳回，选源只在销售单生效后的供给分配任务。
 *   2. 文档称「代发出库单」；界面履约类型为「供应商直发」，主按钮「确认发货」。
 *   3. 文档 7.3.2 未提先款门槛；种子供应商狮峰茶叶付款条件 PREPAY_50，
 *      直发确认前可能必须先由出纳完成付款任务。
 *   4. 文档写「采购提交采购单」；代码由供给分配确认同一事务创建并立即提交审批。
 *   5. 销售单主状态文案为「审核中」而非文档「审批中」。
 *   6. 验收成功 toast 描述仍可能出现「已过账」；按钮文案是「确认本次验收」。
 *   7. 自动推荐按含税成本优先，实物默认更可能选「入仓」；本流程必须显式改选「供应商直发」。
 */

import { test, expect, type Browser, type Locator, type Page } from "@playwright/test"
import fs from "node:fs"
import path from "node:path"

import { ACCOUNTS } from "../helpers/accounts"
import { loginViaUi, newLoggedInContext } from "../helpers/login"

test.describe.configure({ mode: "serial" })

const SKU_KEYWORD = "狮峰明前龙井"
const SUPPLIER_SHORT = "狮峰茶叶"
const DIRECT_OPTION = `${SUPPLIER_SHORT} · 供应商直发`
const WAREHOUSE_OPTION = `${SUPPLIER_SHORT} · 入仓`

const PNG_1X1 = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    "base64",
)

const MINIMAL_PDF = Buffer.from(
    "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R>>endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000068 00000 n \n0000000125 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n210\n%%EOF\n",
)

type Session = {
    context: { close: () => Promise<void> }
    page: Page
}

function accountOf(
    key:
        | "xiaoshou"
        | "caigou"
        | "caiwu"
        | "cangchu"
        | "fukuan"
        | "admin",
): string {
    const bag = ACCOUNTS as Record<
        string,
        { account?: string; password?: string } | string
    >
    const direct = bag[key]
    if (typeof direct === "string") return direct
    if (direct && typeof direct === "object" && direct.account) {
        return direct.account
    }
    const aliases: Record<string, string[]> = {
        xiaoshou: ["sales"],
        caigou: ["procurement"],
        caiwu: ["finance"],
        cangchu: ["warehouse"],
        fukuan: ["payment"],
        admin: ["admin"],
    }
    for (const alias of aliases[key] ?? []) {
        const nested = bag[alias]
        if (nested && typeof nested === "object" && nested.account) {
            return nested.account
        }
    }
    return key
}

function isoPlusDays(days: number): string {
    const date = new Date()
    date.setDate(date.getDate() + days)
    const pad = (n: number) => String(n).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function creditCode(stamp: string): string {
    const body = `9${stamp}ABCDEFGH`.replace(/[^0-9A-Za-z]/g, "0")
    return body.slice(0, 18).padEnd(18, "X")
}

function contractPdf(): { name: string; mimeType: string; buffer: Buffer } {
    const fixture = path.join(process.cwd(), "fixtures", "sample-contract.pdf")
    if (fs.existsSync(fixture)) {
        return {
            name: "sample-contract.pdf",
            mimeType: "application/pdf",
            buffer: fs.readFileSync(fixture),
        }
    }
    return {
        name: "sample-contract.pdf",
        mimeType: "application/pdf",
        buffer: MINIMAL_PDF,
    }
}

async function openSession(browser: Browser, key: Parameters<typeof accountOf>[0]) {
    const account = accountOf(key)
    const session = (await newLoggedInContext(browser, account)) as Session
    await session.page.waitForLoadState("domcontentloaded")
    if (session.page.url().includes("/login")) {
        await loginViaUi(session.page, account)
        await expect(session.page).not.toHaveURL(/\/login/, { timeout: 20000 })
    }
    return session
}

async function closeSession(session: Session) {
    await session.context.close()
}

async function expectToast(page: Page, title: string) {
    await expect(
        page.locator('[data-slot="toast"]').filter({ hasText: title }),
    ).toBeVisible({ timeout: 20000 })
}

async function chooseOption(
    page: Page,
    input: Locator,
    option: string | RegExp,
    typed?: string,
) {
    await input.click()
    const query = typed ?? (typeof option === "string" ? option : "")
    if (query) {
        await input.fill("")
        await input.fill(query)
    }
    const item = page.getByRole("option", { name: option })
    await expect(item).toBeVisible({ timeout: 20000 })
    await item.click()
}

async function pickIsoDate(page: Page, picker: Locator, iso: string) {
    await picker.click()
    const calendar = page.locator('[data-slot="calendar"]')
    await expect(calendar).toBeVisible({ timeout: 20000 })
    const day = calendar.locator(`[id$="-day-${iso}"]`)
    for (let i = 0; i < 4; i += 1) {
        if (await day.isVisible().catch(() => false)) break
        const next = calendar.locator('[id$="-calendar-next-month"], [id$="-next-month"]')
        if (await next.isVisible().catch(() => false)) {
            await next.click()
        } else {
            break
        }
    }
    await expect(day).toBeVisible({ timeout: 20000 })
    await day.click()
}

async function gotoHeading(page: Page, pathName: string, heading: string | RegExp) {
    await page.goto(pathName)
    await expect(page.getByRole("heading", { name: heading })).toBeVisible({
        timeout: 20000,
    })
}

async function openInboxTask(
    page: Page,
    family: "approval" | "procurement" | "fulfillment" | "finance",
    name: RegExp,
) {
    await page.goto(`/workspace?family=${family}`)
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: 20000,
    })
    const list = page.getByRole("list", { name: "待办列表" })
    const task = list.getByRole("button", { name })
    await expect(task).toBeVisible({ timeout: 20000 })
    await task.click()
    await expect(page.getByLabel("当前工作台任务")).toBeVisible({
        timeout: 20000,
    })
}

async function approveCurrentTask(page: Page) {
    const surface = page.getByLabel("当前工作台任务")
    await expect(surface.getByRole("button", { name: "通过" })).toBeVisible({
        timeout: 20000,
    })
    await surface.getByRole("button", { name: "通过" }).click()
    const dialog = page.getByRole("dialog", { name: "确认通过" })
    await expect(dialog).toBeVisible({ timeout: 20000 })
    await dialog.getByRole("button", { name: "确认通过" }).click()
    await expect(dialog).toBeHidden({ timeout: 20000 })
}

async function readDocumentNumber(page: Page): Promise<string> {
    const number = page.locator("header").locator("span.num.text-foreground")
    await expect(number).toBeVisible({ timeout: 20000 })
    const text = ((await number.innerText()) ?? "").trim()
    expect(text.length).toBeGreaterThan(0)
    return text
}

async function ensureProcurementDispatcher(page: Page) {
    await gotoHeading(page, "/master-data/procurement-responsibilities", "采购责任规则")
    const existing = page.getByText("默认调度人")
    if (await existing.isVisible().catch(() => false)) {
        return
    }
    await page.locator("#procurement-responsibility-rules-create").click()
    const dialog = page.getByRole("dialog", { name: "新增采购责任规则" })
    await expect(dialog).toBeVisible({ timeout: 20000 })
    await chooseOption(
        page,
        dialog.locator("#procurement-responsibility-rules-dialog-rule-type"),
        "默认调度人",
        "默认",
    )
    await chooseOption(
        page,
        dialog.locator("#procurement-responsibility-rules-dialog-owner"),
        /采购 · caigou|caigou/,
        "caigou",
    )
    await dialog.locator("#procurement-responsibility-rules-dialog-save").click()
    await expectToast(page, "采购责任规则已新增")
    await expect(page.getByText("默认调度人")).toBeVisible({ timeout: 20000 })
}

async function assertInventoryUntouched(page: Page) {
    await gotoHeading(page, "/inventory", "库存台账")
    await expect(page.getByRole("tab", { name: "余额" })).toBeVisible({
        timeout: 20000,
    })
    await expect(
        page.getByText("当前仓库尚无 ERP 自有库存记录"),
    ).toBeVisible({ timeout: 20000 })
    await expect(page.getByText("采购入库")).toHaveCount(0)

    await page.locator("#inventory-ledger-view-movement").click()
    await expect(page.getByText("采购入库")).toHaveCount(0)

    await page.locator("#inventory-ledger-view-reservation").click()
    await expect(page.getByText("采购入库")).toHaveCount(0)
}

test("供应商直接发客户（代发）全流程", async ({ browser }) => {
    test.setTimeout(8 * 60 * 1000)

    const stamp = Date.now().toString()
    const legalName = `代发测试客户${stamp}`
    const contractNo = `HT-DS-${stamp}`
    const dueDate = isoPlusDays(90)
    const trackingNo = `SF${stamp.slice(-10)}`
    let salesOrderNo = ""
    let purchaseOrderNo = ""

    // ── 0. 采购责任规则：默认调度人 → caigou（提交销售单前置） ──
    {
        const session = await openSession(browser, "admin")
        await ensureProcurementDispatcher(session.page)
        await closeSession(session)
    }

    // ── 1. 销售：新建客户 ──
    {
        const session = await openSession(browser, "xiaoshou")
        const page = session.page
        await gotoHeading(page, "/sales/customers", "客户中心")
        await page.locator("#customers-directory-create").click()
        const dialog = page.getByRole("dialog", { name: "新建客户" })
        await expect(dialog).toBeVisible({ timeout: 20000 })
        await dialog.locator("#customers-form-legal-name").fill(legalName)
        await dialog.locator("#customers-form-short-name").fill(`代发${stamp.slice(-6)}`)
        await dialog.locator("#customers-form-credit-code").fill(creditCode(stamp))
        await chooseOption(
            page,
            dialog.locator("#customers-form-payment-term"),
            "货到 30 天",
            "货到",
        )
        await dialog.locator("#customers-form-submit").click()
        await expectToast(page, "客户已创建")
        await expect(dialog).toBeHidden({ timeout: 20000 })
        await expect(page.getByRole("link", { name: legalName })).toBeVisible({
            timeout: 20000,
        })
        await closeSession(session)
    }

    // ── 2. 销售：上传合同 PDF ──
    {
        const session = await openSession(browser, "xiaoshou")
        const page = session.page
        await gotoHeading(page, "/sales/contracts", "合同")
        await page.getByRole("button", { name: "上传合同 PDF" }).click()
        const dialog = page.getByRole("dialog", { name: "上传合同 PDF" })
        await expect(dialog).toBeVisible({ timeout: 20000 })
        await dialog.locator("#card-contracts-upload-pdf-input").setInputFiles(contractPdf())
        await dialog.locator("#card-contracts-upload-contract-no").fill(contractNo)
        await chooseOption(
            page,
            dialog.locator("#card-contracts-upload-customer"),
            new RegExp(legalName),
            legalName,
        )
        await expect(dialog.locator("#card-contracts-upload-settlement-party")).toHaveValue(
            new RegExp(legalName),
            { timeout: 20000 },
        )
        await dialog.locator("#card-contracts-upload-submit").click()
        await expectToast(page, "合同 PDF 已归档")
        await expect(dialog).toBeHidden({ timeout: 20000 })
        await expect(page.getByText(contractNo)).toBeVisible({ timeout: 20000 })
        await closeSession(session)
    }

    // ── 3. 销售：创建并提交实物销售单（账期/货到，不选源） ──
    {
        const session = await openSession(browser, "xiaoshou")
        const page = session.page
        await page.goto("/sales/orders?mode=create")
        await expect(page.getByRole("heading", { name: "单据头" })).toBeVisible({
            timeout: 20000,
        })
        await chooseOption(
            page,
            page.locator("#sales-orders-create-contract"),
            new RegExp(`${legalName}|${contractNo}`),
            contractNo,
        )
        await expect(page.getByText(legalName)).toBeVisible({ timeout: 20000 })
        await chooseOption(
            page,
            page.locator("#sales-orders-create-header-welfare-scene"),
            "年节礼包",
            "年节",
        )
        await chooseOption(
            page,
            page.locator("#sales-orders-create-header-payment-terms"),
            "货到 30 天",
            "货到",
        )

        await page.locator('[id^="sales-orders-create-line-"][id$="-pick-sku"]').click()
        const skuDialog = page.getByRole("dialog", { name: /选择商品|更换销售商品/ })
        await expect(skuDialog).toBeVisible({ timeout: 20000 })
        const skuSearch = skuDialog.getByPlaceholder("搜索 SKU、商品名称、编号或规格")
        await skuSearch.fill(SKU_KEYWORD)
        await skuSearch.press("Enter")
        await expect(skuDialog.getByText(SKU_KEYWORD)).toBeVisible({ timeout: 20000 })
        await skuDialog.getByRole("checkbox", { name: new RegExp(SKU_KEYWORD) }).check()
        await skuDialog.locator("#sales-orders-sku-picker-confirm").click()
        await expect(skuDialog).toBeHidden({ timeout: 20000 })
        await expect(page.getByText(SKU_KEYWORD)).toBeVisible({ timeout: 20000 })
        await expect(
            page.locator('[data-testid^="sales-line-procurement-owner-"]'),
        ).not.toContainText("暂未确定", { timeout: 20000 })

        await pickIsoDate(
            page,
            page.locator("#sales-orders-create-batch-due-date"),
            dueDate,
        )
        await page.locator("#sales-orders-create-batch-due-date-apply").click()
        await expectToast(page, "已批量设置交期")

        await page.locator("#sales-orders-create-submit").click()
        const confirm = page.getByRole("dialog", { name: "提交销售单" })
        await expect(confirm).toBeVisible({ timeout: 20000 })
        await confirm.locator("#sales-orders-submit-confirm-confirm").click()
        await expect(page).toHaveURL(/\/sales\/orders\/[A-Za-z0-9]+/, {
            timeout: 30000,
        })
        await expect(page.getByRole("heading", { name: legalName })).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByText(/审核中|审批中|待采购/)).toBeVisible({
            timeout: 20000,
        })
        salesOrderNo = await readDocumentNumber(page)
        expect(salesOrderNo.length).toBeGreaterThan(0)
        await closeSession(session)
    }

    // ── 4. 负向：生效前不得建采购单、不得履约、采购确认不得选源 ──
    {
        const session = await openSession(browser, "caigou")
        const page = session.page
        await gotoHeading(page, "/procurement/orders", "采购单")
        await expect(page.getByText(/0 条|当前没有/)).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByText("供应商直发")).toHaveCount(0)

        await openInboxTask(page, "approval", /销售单审批/)
        await expect(page.getByText("供给来源 / 履约责任")).toHaveCount(0)
        await expect(page.getByRole("button", { name: "预览供给分配" })).toHaveCount(0)
        await expect(page.getByRole("button", { name: "通过" })).toBeVisible({
            timeout: 20000,
        })
        await approveCurrentTask(page)
        await closeSession(session)
    }

    // ── 5. 销售单已生效；仓储此时不得出现入库任务 ──
    {
        const session = await openSession(browser, "xiaoshou")
        const page = session.page
        await page.goto("/sales/orders")
        await expect(page.getByRole("heading", { name: "销售单" })).toBeVisible({
            timeout: 20000,
        })
        const orderLink = page.getByRole("link", { name: salesOrderNo })
        await expect(orderLink).toBeVisible({ timeout: 20000 })
        await expect(orderLink.locator("xpath=ancestor::tr[1]").getByText("已生效")).toBeVisible({
            timeout: 20000,
        })
        await closeSession(session)
    }
    {
        const session = await openSession(browser, "cangchu")
        const page = session.page
        await page.goto("/workspace?family=fulfillment")
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByRole("button", { name: /入库/ })).toHaveCount(0)
        await closeSession(session)
    }

    // ── 6. 采购：供给分配显式选「供应商直发」，不得走入仓 ──
    {
        const session = await openSession(browser, "caigou")
        const page = session.page
        await openInboxTask(
            page,
            "procurement",
            new RegExp(`待供给分配.*${salesOrderNo}|${salesOrderNo}`),
        )
        await expect(page.getByRole("heading", { name: "供给分配" })).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByText("销售明细与供给方案")).toBeVisible({
            timeout: 20000,
        })

        const sourcing = page.locator(
            '[id^="procurement-orders-create-row-"][id$="-sourcing-option"]',
        )
        await expect(sourcing).toBeVisible({ timeout: 20000 })
        await chooseOption(page, sourcing, DIRECT_OPTION, "供应商直发")
        await expect(sourcing).toHaveValue(new RegExp("供应商直发"))
        await expect(sourcing).not.toHaveValue(WAREHOUSE_OPTION)
        await expect(page.getByText("不适用")).toBeVisible()
        await expect(page.getByPlaceholder("选择目标仓")).toHaveCount(0)

        await page.locator("#procurement-orders-create-preview").click()
        const preview = page.getByRole("dialog", { name: "预览供给分配" })
        await expect(preview).toBeVisible({ timeout: 20000 })
        await expect(preview.getByText("本次不占用现有库存")).toBeVisible({
            timeout: 20000,
        })
        await expect(preview.getByText("现有库存分配")).toHaveCount(0)
        await expect(preview.getByText("供应商直发")).toBeVisible()
        await expect(preview.getByText("入仓")).not.toBeVisible()
        await preview.getByRole("button", { name: /确认提交 1 张采购单/ }).click()

        const confirm = page.getByRole("alertdialog", { name: "确认供给分配" })
        await expect(confirm).toBeVisible({ timeout: 20000 })
        await expect(confirm.getByText(/创建 1 张采购单提交审批/)).toBeVisible()
        await confirm.locator("#procurement-orders-create-confirm").click()
        await expectToast(page, "供给分配已完成")
        await closeSession(session)
    }

    // ── 7. 采购单已提交审批：履约责任=供应商直发；不得留草稿 ──
    {
        const session = await openSession(browser, "caigou")
        const page = session.page
        await gotoHeading(page, "/procurement/orders", "采购单")
        const poTable = page.locator("#procurement-orders-list-table")
        await expect(page.getByText("1 条")).toBeVisible({ timeout: 20000 })
        await expect(poTable.getByText("供应商直发")).toBeVisible({ timeout: 20000 })
        await expect(poTable.getByText("入仓")).not.toBeVisible()
        await expect(poTable.getByText("草稿")).not.toBeVisible()
        await expect(poTable.getByText("审批中")).toBeVisible({ timeout: 20000 })
        purchaseOrderNo = (
            (await poTable.getByRole("link", { name: /打开采购单/ }).textContent()) ?? ""
        ).trim()
        expect(purchaseOrderNo.length).toBeGreaterThan(0)
        await closeSession(session)
    }

    // ── 8. 财务总监审批采购单生效 ──
    {
        const session = await openSession(browser, "caiwu")
        const page = session.page
        await openInboxTask(page, "approval", /采购单审批/)
        await expect(page.getByText("供给来源 / 履约责任")).toHaveCount(0)
        await approveCurrentTask(page)
        await closeSession(session)
    }

    // ── 9. 采购单已生效；仓储仍不得入库 ──
    {
        const session = await openSession(browser, "caigou")
        const page = session.page
        await gotoHeading(page, "/procurement/orders", "采购单")
        const poTable = page.locator("#procurement-orders-list-table")
        await expect(poTable.getByText("已生效")).toBeVisible({ timeout: 20000 })
        await expect(poTable.getByText("供应商直发")).toBeVisible()
        await expect(poTable.getByText(purchaseOrderNo)).toBeVisible()
        await closeSession(session)
    }
    {
        const session = await openSession(browser, "cangchu")
        const page = session.page
        await page.goto("/workspace?family=fulfillment")
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByRole("button", { name: /入库/ })).toHaveCount(0)
        await assertInventoryUntouched(page)
        await closeSession(session)
    }

    // ── 10. 若先款门槛拦住直发，出纳先完成付款任务 ──
    {
        const session = await openSession(browser, "caigou")
        const page = session.page
        await openInboxTask(page, "fulfillment", /履约处理|供应商直发|代发/)
        await expect(page.getByText("供应商直发")).toBeVisible({ timeout: 20000 })
        await expect(page.getByLabel("供应商直发表单")).toBeVisible({
            timeout: 20000,
        })
        const confirmBtn = page.locator("#fulfillment-operations-work-surface-confirm")
        const blocked =
            (await page.getByText(/先款未到|暂时不能/).isVisible().catch(() => false)) ||
            !(await confirmBtn.isEnabled())
        await closeSession(session)

        if (blocked) {
            const pay = await openSession(browser, "fukuan")
            await openInboxTask(pay.page, "finance", /供应商付款处理/)
            await expect(
                pay.page.locator("#supplier-payables-allocation-form-amount"),
            ).not.toHaveValue("", { timeout: 20000 })
            await pay.page
                .locator("#supplier-payables-allocation-form-bank-receipt-input")
                .setInputFiles({
                    name: "bank-receipt.png",
                    mimeType: "image/png",
                    buffer: PNG_1X1,
                })
            await pay.page.locator("#supplier-payables-allocation-form-submit").click()
            const payDialog = pay.page.getByRole("alertdialog", { name: "确认付款" })
            await expect(payDialog).toBeVisible({ timeout: 20000 })
            await payDialog
                .locator("#supplier-payables-payment-submit-confirm-confirm")
                .click()
            await expect(payDialog).toBeHidden({ timeout: 20000 })
            await closeSession(pay)
        }
    }

    // ── 11. 采购：登记代发（不经仓库）并确认发货 ──
    {
        const session = await openSession(browser, "caigou")
        const page = session.page
        await openInboxTask(page, "fulfillment", /履约处理|供应商直发|代发/)
        await expect(page.getByLabel("供应商直发表单")).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByText("不走自有仓库，库存不变")).toBeVisible()
        await chooseOption(
            page,
            page.locator("#fulfillment-operations-direct-form-carrier"),
            "顺丰速运",
            "顺丰",
        )
        await page
            .locator("#fulfillment-operations-direct-form-tracking-no")
            .fill(trackingNo)
        await expect(
            page.locator("#fulfillment-operations-work-surface-confirm"),
        ).toBeEnabled({ timeout: 20000 })
        await page.locator("#fulfillment-operations-work-surface-confirm").click()
        const confirm = page.getByRole("alertdialog", { name: "确认发货？" })
        await expect(confirm).toBeVisible({ timeout: 20000 })
        await confirm.locator("#fulfillment-operations-workspace-confirm-confirm").click()
        await expect(page.getByText("已发货")).toBeVisible({ timeout: 20000 })
        await closeSession(session)
    }

    // ── 12. 销售：客户验收通过 ──
    {
        const session = await openSession(browser, "xiaoshou")
        const page = session.page
        await openInboxTask(page, "fulfillment", /客户验收登记/)
        await page.locator("#sales-orders-acceptance-register-open").click()
        const dialog = page.getByRole("dialog", { name: "登记客户验收" })
        await expect(dialog).toBeVisible({ timeout: 20000 })
        await dialog.locator("#sales-orders-acceptance-register-submit").click()
        const confirm = page.getByRole("alertdialog", { name: "确认客户验收" })
        await expect(confirm).toBeVisible({ timeout: 20000 })
        await confirm.locator("#sales-orders-acceptance-confirm-confirm").click()
        await expectToast(page, "客户验收已登记")
        await expect(page.getByText("通过")).toBeVisible({ timeout: 20000 })
        await closeSession(session)
    }

    // ── 13. 终态断言：自有库存无变化、无采购入库单、无入仓履约 ──
    {
        const session = await openSession(browser, "caigou")
        const page = session.page
        await assertInventoryUntouched(page)
        await gotoHeading(page, "/procurement/orders", "采购单")
        const poTable = page.locator("#procurement-orders-list-table")
        await expect(poTable.getByText("供应商直发")).toBeVisible({ timeout: 20000 })
        await expect(poTable.getByText("入仓")).not.toBeVisible()
        await closeSession(session)
    }
    {
        const session = await openSession(browser, "cangchu")
        const page = session.page
        await page.goto("/workspace?family=fulfillment")
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByRole("button", { name: /入库/ })).toHaveCount(0)
        await assertInventoryUntouched(page)
        await closeSession(session)
    }
    {
        const session = await openSession(browser, "xiaoshou")
        const page = session.page
        await page.goto("/sales/orders")
        await expect(page.getByRole("link", { name: salesOrderNo })).toBeVisible({
            timeout: 20000,
        })
        await page.getByRole("link", { name: salesOrderNo }).click()
        await expect(page.getByText("已生效")).toBeVisible({ timeout: 20000 })
        await expect(page.getByText(/已完成|履约/)).toBeVisible({ timeout: 20000 })
        await closeSession(session)
    }
})
