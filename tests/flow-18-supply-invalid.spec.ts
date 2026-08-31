/**
 * 流程: [flow-18] 供给失效不得建采购单，必须走销售变更
 * 文档: docs/erp-phase-1.md §7.4（供给失效/停止可供/资质失效时采购单不得创建，
 *       走销售变更单）+ §4.4（生效后禁止回到审批改选源）+ §6.3（销售变更单）
 * 使用账号: admin（采购责任默认调度人）、xiaoshou（客户/合同/销售单/变更单）、
 *           caigou（销售审批、停止可供、供给分配）、caiwu（变更单财务复核）、
 *           cangchu（库存预占负向核对）
 *
 * 文档-代码差异（以代码为准）:
 * 1. §7.4 要求选源与有效供给不一致时采购单不得创建、按 §6.3 改品或改量后再分配；
 *    代码 qualified_supply 只收录 ACTIVE + 条款有效 + AVAILABLE 的供给，停止可供后
 *    创建依据为空，工作台呈现「当前没有待分配供给」，无法预览/确认建单。
 * 2. 销售变更单创建时克隆当前生效版本，原因写死「销售发起变更」，
 *    PUT /sales-orders/{id}/working-copy 只服务 FirstSubmission，详情页无改品/改量
 *    编辑面；变更审批通过后 SKU/数量仍与 v1 相同，供给仍失效则仍不得建采购单。
 * 3. 文档 §6.5.1 时序由销售「生效」变更单；代码末节点通过即 apply_effective_change。
 * 4. 文档「采购确认履约影响 / 财务复核」已收敛为 SalesChangeOrder 的 DOCUMENT_APPROVAL；
 *    退役类型不再进入 W01。
 * 5. 销售单生效后「撤回审批」不渲染（salesOrderAllowsWithdrawApproval 仅审批中）；
 *    禁止回到审批改选源，由按钮缺失 + 审批区无「通过」共同断言。
 */
import fs from "node:fs"
import path from "node:path"

import {
    expect,
    test,
    type Browser,
    type BrowserContext,
    type Locator,
    type Page,
} from "@playwright/test"

import { ACCOUNTS } from "../helpers/accounts"
import { loginViaUi, newLoggedInContext } from "../helpers/login"
import "../helpers/ui"

test.use({ viewport: { width: 1440, height: 900 } })
test.describe.configure({ mode: "serial" })

const UI_TIMEOUT = 20_000
const FLOW_TIMEOUT = 12 * 60 * 1000
const CONTRACT_PDF = path.resolve(process.cwd(), "fixtures/sample-contract.pdf")
const SKU_KEYWORD = "龙井"
const SKU_NAME = "狮峰明前龙井礼盒"
const SKU_NO = "TEA-SF-LJ-250"
const SUPPLIER_SKU_CODE = "SF-LJ-250"
const SALES_QTY = "2"
const MINIMAL_PDF = Buffer.from(
    "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n",
)

type LoginName = "xiaoshou" | "caigou" | "caiwu" | "cangchu" | "admin"
type Session = { context: BrowserContext; page: Page }

function accountCred(login: LoginName): { account: string; password: string } {
    const bag = ACCOUNTS as Record<
        string,
        { account?: string; password?: string } | undefined
    >
    const aliases: Record<LoginName, string[]> = {
        xiaoshou: ["xiaoshou", "sales"],
        caigou: ["caigou", "procurement"],
        caiwu: ["caiwu", "finance"],
        cangchu: ["cangchu", "warehouse"],
        admin: ["admin"],
    }
    for (const key of aliases[login]) {
        const row = bag[key]
        if (row?.password) {
            return { account: row.account ?? login, password: row.password }
        }
    }
    for (const row of Object.values(bag)) {
        if (row?.account === login && row.password) {
            return { account: row.account, password: row.password }
        }
    }
    return { account: login, password: "123456" }
}

function asSession(raw: unknown): Session {
    if (raw && typeof raw === "object" && "page" in raw && "context" in raw) {
        const session = raw as Session
        if (session.page && session.context) return session
    }
    if (raw && typeof raw === "object" && "goto" in raw) {
        const page = raw as Page
        return { context: page.context(), page }
    }
    throw new Error("newLoggedInContext 必须返回 { context, page } 或 Page")
}

async function openSession(browser: Browser, login: LoginName): Promise<Session> {
    const cred = accountCred(login)
    const raw = await newLoggedInContext(browser, cred as never)
    const session = asSession(raw)
    if (session.page.url().includes("/login")) {
        await loginViaUi(session.page, cred as never)
    }
    await session.page.goto("/workspace")
    await expect(session.page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
    return session
}

function orderTitleRow(page: Page, customerName: string) {
    return page.getByRole("heading", { name: customerName }).locator("xpath=..")
}

function dialogish(page: Page, name: string | RegExp) {
    return page.getByRole("alertdialog", { name }).or(page.getByRole("dialog", { name }))
}

async function expectToast(page: Page, title: string | RegExp) {
    const toast = page.locator('[data-slot="toast-title"]').filter({ hasText: title })
    await expect(toast.first()).toBeVisible({ timeout: UI_TIMEOUT })
}

async function chooseOption(page: Page, input: Locator, option: string | RegExp) {
    await input.click()
    if (typeof option === "string") {
        await input.fill("")
        await input.fill(option)
    }
    const listed = page
        .getByRole("option", { name: option })
        .or(page.locator('[data-slot="combobox-item"]').filter({ hasText: option }))
        .first()
    await expect(listed).toBeVisible({ timeout: UI_TIMEOUT })
    await listed.click()
}

async function pickCalendarDay(page: Page, trigger: Locator, isoDate: string) {
    await trigger.click()
    const calendar = page.locator('[data-slot="calendar"]').last()
    await expect(calendar).toBeVisible({ timeout: UI_TIMEOUT })
    const target = new Date(`${isoDate}T00:00:00`)
    const year = target.getFullYear()
    const month = target.getMonth()
    const day = String(target.getDate())
    const monthTokens = [
        `${month + 1}月`,
        [
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
        ][month]!,
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][
            month
        ]!,
    ]
    for (let i = 0; i < 18; i += 1) {
        const caption = await calendar.innerText()
        const yearOk = caption.includes(String(year))
        const monthOk = monthTokens.some((token) => caption.includes(token))
        if (yearOk && monthOk) break
        const next = calendar.getByRole("button", {
            name: /next month|go to the next month|下个月|下一月/i,
        })
        if (await next.count()) {
            await next.first().click()
        } else {
            await calendar.locator("button").last().click()
        }
    }
    const dayButtons = calendar.getByRole("button", { name: day, exact: true })
    const total = await dayButtons.count()
    for (let i = 0; i < total; i += 1) {
        const button = dayButtons.nth(i)
        const disabled = await button.getAttribute("aria-disabled")
        const outside = await button.getAttribute("data-outside")
        if (disabled === "true" || outside === "true") continue
        await button.click()
        return
    }
    await dayButtons.first().click()
}

async function openWorkspaceTask(
    page: Page,
    typeLabel: string,
    hint?: string,
    family?: "approval" | "procurement" | "fulfillment" | "finance",
) {
    await page.goto("/workspace")
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
    if (family) {
        await page.locator(`#workspace-family-nav-${family}`).click()
    }
    const search = page.locator("#workspace-queue-toolbar-search-input")
    if (hint && (await search.count())) {
        await search.fill(hint)
        await search.press("Enter")
    }
    const hinted = hint
        ? page.getByRole("button", {
              name: new RegExp(`${typeLabel}[\\s\\S]*${hint}|${hint}[\\s\\S]*${typeLabel}`),
          })
        : page.getByRole("button", { name: new RegExp(typeLabel) })
    const fallback = page.getByRole("button", { name: new RegExp(typeLabel) }).first()
    const task = hinted.first()
    try {
        await expect(task).toBeVisible({ timeout: hint ? 8_000 : UI_TIMEOUT })
        await task.click()
        await expect(task).toHaveAttribute("aria-current", "true")
    } catch {
        await expect(fallback).toBeVisible({ timeout: UI_TIMEOUT })
        await fallback.click()
        await expect(fallback).toHaveAttribute("aria-current", "true")
    }
}

async function approveCurrentDocument(page: Page) {
    const approve = page.getByRole("button", { name: "通过", exact: true })
    await expect(approve).toBeVisible({ timeout: UI_TIMEOUT })
    await expect(page.getByRole("button", { name: "驳回", exact: true })).toBeVisible()
    await expect(page.getByLabel("供给来源 / 履约责任")).toHaveCount(0)
    await expect(page.getByLabel("含税成本")).toHaveCount(0)
    await expect(page.getByLabel("预计交付日")).toHaveCount(0)
    await approve.click()
    const dialog = dialogish(page, "确认通过")
    await expect(dialog.first()).toBeVisible({ timeout: UI_TIMEOUT })
    await dialog.getByRole("button", { name: "确认通过" }).click()
    await expect(dialog.first()).toBeHidden({ timeout: UI_TIMEOUT })
}

async function ensureDefaultProcurementOwner(page: Page) {
    await page.goto("/master-data/procurement-responsibilities")
    await expect(page.getByRole("heading", { name: "采购责任规则" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
    if (await page.getByText("默认调度人").count()) {
        return
    }
    await page.getByRole("button", { name: "新增规则" }).click()
    const dialog = page.getByRole("dialog", { name: "新增采购责任规则" })
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT })
    await chooseOption(
        page,
        dialog.locator("#procurement-responsibility-rules-dialog-rule-type"),
        "默认调度人",
    )
    await chooseOption(page, dialog.locator("#procurement-responsibility-rules-dialog-owner"), /采购/)
    await dialog.getByRole("button", { name: "保存规则" }).click()
    await expectToast(page, /采购责任规则已新增|采购责任规则已更新/)
    await expect(dialog).toBeHidden({ timeout: UI_TIMEOUT })
}

function plusDaysIso(days: number): string {
    const date = new Date()
    date.setDate(date.getDate() + days)
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function uniqueCreditCode(stamp: string): string {
    const raw = `91${stamp.replace(/[^0-9A-Za-z]/g, "").toUpperCase()}E2ESUPPLYINV`
    return raw.slice(0, 18).padEnd(18, "0")
}

function pdfUpload(): { name: string; mimeType: string; buffer: Buffer } | string {
    if (fs.existsSync(CONTRACT_PDF)) return CONTRACT_PDF
    return {
        name: "sample-contract.pdf",
        mimeType: "application/pdf",
        buffer: MINIMAL_PDF,
    }
}

async function expectNoPurchaseOrders(page: Page, salesOrderNo: string) {
    await page.goto("/procurement/orders")
    await expect(page.getByRole("heading", { name: "采购单" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
    const search = page.getByLabel("搜索采购单")
    await search.fill(salesOrderNo)
    await search.press("Enter")
    await expect(
        page.getByText("暂无采购单").or(page.getByText("当前筛选无结果")),
    ).toBeVisible({ timeout: UI_TIMEOUT })
    await expect(page.getByRole("link", { name: /打开采购单/ })).toHaveCount(0)
}

async function expectAllocationCannotCreatePurchase(page: Page, salesOrderNo: string) {
    await page.getByRole("button", { name: "刷新" }).click()
    await openWorkspaceTask(page, "待供给分配", salesOrderNo, "procurement")
    await expect(
        page.getByRole("heading", { name: "供给分配" }).or(page.getByText("销售明细与供给方案")),
    ).toBeVisible({ timeout: UI_TIMEOUT })

    const empty = page.getByText("当前没有待分配供给")
    const table = page.getByRole("heading", { name: "销售明细与供给方案" })
    await expect(empty.or(table)).toBeVisible({ timeout: UI_TIMEOUT })

    if (await empty.isVisible()) {
        await expect(
            page.getByText(/既无可用库存也无合格采购供给|请检查已生效销售单、库存余额和供应商供给/),
        ).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(page.getByRole("button", { name: "预览供给分配" })).toHaveCount(0)
        await expect(page.getByRole("button", { name: "确认提交" })).toHaveCount(0)
        return
    }

    const rematch = page.getByRole("button", { name: "重新自动分配" })
    if (await rematch.count()) {
        await rematch.click()
        await expectToast(page, /没有可匹配的供给方案|已重新分配供给/)
    }
    await expect(page.getByText("将创建采购单").locator("xpath=..")).toContainText("0 张")
    await expect(page.getByText("将建立库存预留").locator("xpath=..")).toContainText("0 条")
    await page.getByRole("button", { name: "预览供给分配" }).click()
    await expectToast(page, "无法预览供给分配")
    await expect(
        page.getByText(/请选择履约方案|请至少选择一条本次供给分配明细|该履约方案已失效|没有可匹配的供给方案/),
    ).toBeVisible({ timeout: UI_TIMEOUT })
    await expect(page.getByRole("dialog", { name: "预览供给分配" })).toHaveCount(0)
    await expect(dialogish(page, "确认供给分配")).toHaveCount(0)
}

test("flow-18 停止可供后供给分配不得建采购单，必须走销售变更且禁止回到审批", async ({
    browser,
}) => {
    test.setTimeout(FLOW_TIMEOUT)
    const stamp = Date.now().toString(36).toUpperCase()
    const customerName = `E2E供给失效客户${stamp}`
    const contractNo = `HT-E2E-18-${stamp}`
    const dueDate = plusDaysIso(21)
    let session: Session | undefined
    let salesOrderId = ""
    let salesOrderNo = ""

    const switchTo = async (login: LoginName) => {
        await session?.context.close()
        session = await openSession(browser, login)
        return session.page
    }

    try {
        // 0) 采购责任默认调度人：销售提交实物单前必须能解析采购负责人
        let page = await switchTo("admin")
        await ensureDefaultProcurementOwner(page)

        // 1) W03 客户创建
        page = await switchTo("xiaoshou")
        await page.goto("/sales/customers")
        await expect(page.getByRole("heading", { name: "客户中心" })).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await page.locator("#customers-directory-create").click()
        const customerDialog = page.getByRole("dialog", { name: "新建客户" })
        await expect(customerDialog).toBeVisible({ timeout: UI_TIMEOUT })
        await customerDialog.locator("#customers-form-legal-name").fill(customerName)
        await customerDialog.locator("#customers-form-short-name").fill(`失效${stamp}`)
        await customerDialog.locator("#customers-form-credit-code").fill(uniqueCreditCode(stamp))
        await chooseOption(page, customerDialog.locator("#customers-form-payment-term"), "货到 15 天")
        await customerDialog.locator("#customers-form-submit").click()
        await expectToast(page, "客户已创建")
        await expect(customerDialog).toBeHidden({ timeout: UI_TIMEOUT })
        await page.locator("#customers-directory-search").fill(customerName)
        await page.locator("#customers-directory-search").press("Enter")
        await expect(page.getByText(customerName).first()).toBeVisible({ timeout: UI_TIMEOUT })

        // 2) W05 销售单：上传合同 + 实物 SKU，提交后进入采购确认（审批中禁止改单）
        await page.goto("/sales/orders")
        await expect(page.getByRole("heading", { name: "销售单" })).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await page.locator("#sales-orders-list-header-create").click()
        await expect(page.getByRole("heading", { name: "销售明细" })).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await expect(page.getByLabel("供应商")).toHaveCount(0)
        await expect(page.getByLabel("履约责任")).toHaveCount(0)
        await expect(page.getByLabel("采购成本")).toHaveCount(0)

        await page.locator("#sales-orders-create-contract-upload").click()
        const upload = page.getByRole("dialog", { name: "上传合同 PDF" })
        await expect(upload).toBeVisible({ timeout: UI_TIMEOUT })
        await upload.locator("#card-contracts-upload-pdf-input").setInputFiles(pdfUpload())
        await upload.locator("#card-contracts-upload-contract-no").fill(contractNo)
        await chooseOption(page, upload.locator("#card-contracts-upload-customer"), customerName)
        await expect
            .poll(async () => upload.locator("#card-contracts-upload-settlement-party").inputValue(), {
                timeout: UI_TIMEOUT,
            })
            .not.toEqual("")
        await chooseOption(page, upload.locator("#card-contracts-upload-payment-terms"), "货到 15 天")
        await upload.locator("#card-contracts-upload-submit").click()
        await expect(upload).toBeHidden({ timeout: UI_TIMEOUT })
        await expect(page.getByText(customerName).first()).toBeVisible({ timeout: UI_TIMEOUT })

        await chooseOption(page, page.locator("#sales-orders-create-header-welfare-scene"), "年节礼包")
        await chooseOption(page, page.locator("#sales-orders-create-header-payment-terms"), "货到 15 天")
        await page.getByRole("button", { name: "选择商品" }).click()
        const skuDialog = page.getByRole("dialog", { name: "选择商品" })
        await expect(skuDialog).toBeVisible({ timeout: UI_TIMEOUT })
        await skuDialog
            .getByPlaceholder("搜索 SKU、商品名称、编号或规格")
            .fill(SKU_KEYWORD)
        await skuDialog.getByPlaceholder("搜索 SKU、商品名称、编号或规格").press("Enter")
        const skuRow = skuDialog.getByRole("checkbox", { name: new RegExp(`选择.*${SKU_NAME}`) })
        await expect(skuRow.first()).toBeVisible({ timeout: UI_TIMEOUT })
        await skuRow.first().check()
        await skuDialog.locator("#sales-orders-sku-picker-confirm").click()
        await expect(skuDialog).toBeHidden({ timeout: UI_TIMEOUT })
        await expect(page.getByText(SKU_NAME).first()).toBeVisible({ timeout: UI_TIMEOUT })
        await page.getByLabel("数量").fill(SALES_QTY)
        await pickCalendarDay(page, page.locator("#sales-orders-create-batch-due-date"), dueDate)
        await page.locator("#sales-orders-create-batch-due-date-apply").click()
        await expectToast(page, "已批量设置交期")
        await expect(page.getByText("暂未确定采购负责人")).toHaveCount(0, { timeout: UI_TIMEOUT })
        await page.locator("#sales-orders-create-submit").click()
        const submitDialog = page.getByRole("dialog", { name: "提交销售单" })
        await expect(submitDialog).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(submitDialog.getByText("审批中")).toBeVisible()
        await submitDialog.locator("#sales-orders-submit-confirm-confirm").click()
        await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, { timeout: UI_TIMEOUT })
        salesOrderId = page.url().split("/sales/orders/")[1]?.split("?")[0] ?? ""
        expect(salesOrderId).toBeTruthy()
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await expect(orderTitleRow(page, customerName).getByText("审批中")).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        salesOrderNo = (await page.locator("span.num.text-foreground").first().innerText()).trim()
        expect(salesOrderNo).toBeTruthy()
        await expect(page.locator("#sales-orders-detail-start-change")).toBeDisabled()
        await expect(page.getByText(/采购单 0 笔/)).toBeVisible()
        await expect(page.locator("#sales-orders-create-submit")).toHaveCount(0)

        // 3) W01 采购确认：只通过/驳回，不选源、不录入成本/交期
        page = await switchTo("caigou")
        await openWorkspaceTask(page, "单据审批", salesOrderNo, "approval")
        await expect(page.getByText("采购确认").first()).toBeVisible({ timeout: UI_TIMEOUT })
        await approveCurrentDocument(page)

        // 4) 销售单生效后供给分配出现且当时仍有合格采购供给（先核对，不确认）
        await page.getByRole("button", { name: "刷新" }).click()
        await openWorkspaceTask(page, "待供给分配", salesOrderNo, "procurement")
        await expect(page.getByRole("heading", { name: "供给分配" })).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await expect(page.getByRole("heading", { name: "销售明细与供给方案" })).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await expect(page.getByLabel(/履约方案/)).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(page.getByText(/入仓|直发/).first()).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(page.getByText("将创建采购单").locator("xpath=..")).toContainText(/[1-9]\s*张/)
        await expect(page.getByRole("button", { name: "预览供给分配" })).toBeVisible()

        // 5) 供给分配确认前：采购停止该 SKU 的有效供给（停止可供）
        await page.goto(`/procurement/supplier-offerings?q=${encodeURIComponent(SUPPLIER_SKU_CODE)}`)
        await expect(page.getByRole("heading", { name: "供应商供给" })).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        const offeringSearch = page.getByLabel("搜索供给")
        await offeringSearch.fill(SUPPLIER_SKU_CODE)
        await offeringSearch.press("Enter")
        const offeringRow = page.getByRole("row").filter({ hasText: SKU_NAME })
        await expect(offeringRow.first()).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(offeringRow.getByText("可供").first()).toBeVisible()
        const quantityBefore = await offeringRow.locator("td").filter({ hasText: /数量/ }).innerText()
        await offeringRow.getByRole("button", { name: "更新可供" }).click()
        const availabilityDialog = page.getByRole("dialog", { name: "更新当前可供情况" })
        await expect(availabilityDialog).toBeVisible({ timeout: UI_TIMEOUT })
        await chooseOption(
            page,
            availabilityDialog.locator("#supplier-offerings-dialog-availability-status"),
            "停止供应",
        )
        await availabilityDialog.locator("#supplier-offerings-dialog-availability-reason").fill(
            "E2E 销售生效后停止可供，验证不得建采购单",
        )
        await availabilityDialog.getByRole("button", { name: "保存可供情况" }).click()
        await expect(availabilityDialog).toBeHidden({ timeout: UI_TIMEOUT })
        await expect(offeringRow.getByText("停止供应").first()).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(offeringRow.getByText("可供", { exact: true })).toHaveCount(0)
        const quantityAfter = await offeringRow.locator("td").filter({ hasText: /数量/ }).innerText()
        expect(quantityAfter).toBe(quantityBefore)

        // 6) 负向：供给分配不得创建采购单，不得预览确认，不得虚增库存预留
        await expectAllocationCannotCreatePurchase(page, salesOrderNo)
        await expectNoPurchaseOrders(page, salesOrderNo)

        await page.goto("/procurement/orders?mode=create")
        await expect(page.getByText("当前没有待分配供给")).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(page.getByRole("button", { name: "预览供给分配" })).toHaveCount(0)

        // 7) 仓储侧：不得虚增库存预占
        page = await switchTo("cangchu")
        await page.goto("/inventory?view=reservation")
        await expect(page.getByRole("heading", { name: "库存台账" })).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await page.getByRole("tab", { name: "销售预占" }).click()
        const inventorySearch = page.getByLabel("搜索库存")
        await inventorySearch.fill(salesOrderNo)
        await inventorySearch.press("Enter")
        await expect(page.getByText(salesOrderNo)).toHaveCount(0)
        await inventorySearch.fill(SKU_NO)
        await inventorySearch.press("Enter")
        await expect(page.getByText(salesOrderNo)).toHaveCount(0)

        // 8) 财务工作台不得出现采购单审批实例；履约任务不得出现
        page = await switchTo("caiwu")
        await page.goto("/workspace")
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await page.locator("#workspace-family-nav-approval").click()
        await page.locator("#workspace-queue-toolbar-search-input").fill(salesOrderNo)
        await page.locator("#workspace-queue-toolbar-search-input").press("Enter")
        await expect(
            page.getByText("当前筛选没有待办").or(page.getByText("当前没有待处理事项")),
        ).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(page.getByRole("button", { name: /采购单审批/ })).toHaveCount(0)
        await page.locator("#workspace-family-nav-fulfillment").click()
        await expect(page.getByRole("button", { name: /履约处理|客户验收登记/ })).toHaveCount(0)

        // 9) 销售单保持已生效：禁止撤回审批改选源、不得关闭、不得履约；必须走变更单
        page = await switchTo("xiaoshou")
        await page.goto(`/sales/orders/${salesOrderId}`)
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await expect(
            page.getByText("版本 v1", { exact: true }).or(page.getByText("v1", { exact: true })),
        ).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(page.getByText("未开始").first()).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(page.getByText("已关闭", { exact: true })).toHaveCount(0)
        await expect(page.getByText(/采购单 0 笔/)).toBeVisible()
        await expect(page.locator("#sales-orders-detail-cancel-approval-trigger")).toHaveCount(0)
        await expect(page.locator("#sales-orders-create-submit")).toHaveCount(0)
        await expect(page.getByRole("button", { name: "通过", exact: true })).toHaveCount(0)
        await expect(page.getByLabel("供给来源 / 履约责任")).toHaveCount(0)
        const startChange = page.locator("#sales-orders-detail-start-change")
        await expect(startChange).toBeEnabled({ timeout: UI_TIMEOUT })

        await page.getByRole("tab", { name: /审批/ }).click()
        await expect(page.getByRole("button", { name: "通过", exact: true })).toHaveCount(0)
        await expect(page.getByRole("button", { name: "驳回", exact: true })).toHaveCount(0)

        await page.getByRole("tab", { name: /采购/ }).click()
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("采购单 0 笔")

        // 10) 发起销售变更单（代码无改品/改量编辑面，工作副本克隆当前版本）
        await startChange.click()
        const startDialog = dialogish(page, "发起改单")
        await expect(startDialog.first()).toBeVisible({ timeout: UI_TIMEOUT })
        await startDialog.locator("#sales-orders-detail-change-confirm").click()
        await expect(page.getByText("改单已创建")).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(startDialog.first()).toBeHidden({ timeout: UI_TIMEOUT })
        await expect(page.locator("#sales-orders-detail-start-change")).toBeDisabled()
        await expect(page.getByRole("tab", { name: /版本/ })).toContainText("改单中")
        await expect(page.getByRole("button", { name: "选择商品" })).toHaveCount(0)
        await expect(page.getByRole("heading", { name: "销售明细" })).toHaveCount(0)

        await page.getByRole("tab", { name: /版本/ }).click()
        await expect(page.getByRole("button", { name: "提交改单" })).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await expect(page.getByText(SKU_NAME).first()).toBeVisible()
        await page.locator("#sales-orders-change-submit").click()
        const changeSubmit = dialogish(page, /提交改单/)
        await expect(changeSubmit.first()).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(changeSubmit.getByText("采购确认履约影响")).toBeVisible()
        await expect(changeSubmit.getByText("财务复核金额与应收")).toBeVisible()
        await changeSubmit.locator("#sales-orders-change-submit-confirm-confirm").click()
        await expect(page.getByText("改单已提交审批")).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible()
        await expect(page.getByText("审批中").first()).toBeVisible({ timeout: UI_TIMEOUT })

        // 11) 变更审批期间：销售单不得回到草稿改选源，采购单仍为零
        await expect(page.locator("#sales-orders-detail-cancel-approval-trigger")).toHaveCount(0)
        await expect(page.locator("#sales-orders-create-submit")).toHaveCount(0)
        await page.getByRole("tab", { name: /采购/ }).click()
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("采购单 0 笔")

        // 12) W01 采购确认履约影响 → 财务复核金额与应收 → 自动生效 v2
        page = await switchTo("caigou")
        await openWorkspaceTask(page, "单据审批", undefined, "approval")
        await expect(page.getByText("采购确认履约影响").first()).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await expect(page.getByLabel("供给来源 / 履约责任")).toHaveCount(0)
        await approveCurrentDocument(page)

        page = await switchTo("caiwu")
        await openWorkspaceTask(page, "单据审批", undefined, "approval")
        await expect(page.getByText("财务复核金额与应收").first()).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await approveCurrentDocument(page)
        await expect(page.getByRole("button", { name: /采购单审批/ })).toHaveCount(0)

        // 13) 变更生效后：版本递增，SKU 未改，供给仍失效，仍不得建采购单/履约/关闭
        page = await switchTo("xiaoshou")
        await page.goto(`/sales/orders/${salesOrderId}`)
        await expect(orderTitleRow(page, customerName).getByText("已生效")).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await expect(
            page.getByText("版本 v2", { exact: true }).or(page.getByText("v2", { exact: true })),
        ).toBeVisible({ timeout: UI_TIMEOUT })
        await expect(page.getByText(SKU_NAME).first()).toBeVisible()
        await expect(page.getByText("未开始").first()).toBeVisible()
        await expect(page.getByText("已关闭", { exact: true })).toHaveCount(0)
        await expect(page.locator("#sales-orders-detail-cancel-approval-trigger")).toHaveCount(0)
        await expect(page.getByRole("button", { name: "通过", exact: true })).toHaveCount(0)
        await page.getByRole("tab", { name: /采购/ }).click()
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("采购单 0 笔")
        await page.getByRole("tab", { name: /版本/ }).click()
        await expect(page.getByText("当前 v2").or(page.getByText("当前在用"))).toBeVisible({
            timeout: UI_TIMEOUT,
        })
        await expect(page.getByText("销售变更单").first()).toBeVisible()
        await expect(page.getByRole("button", { name: "提交改单" })).toHaveCount(0)
        await expect(page.getByRole("button", { name: "选择商品" })).toHaveCount(0)

        page = await switchTo("caigou")
        await expectAllocationCannotCreatePurchase(page, salesOrderNo)
        await expectNoPurchaseOrders(page, salesOrderNo)
    } finally {
        await session?.context.close()
    }
})
