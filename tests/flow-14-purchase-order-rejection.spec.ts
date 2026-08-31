/**
 * 流程: [flow-14] 采购单审批驳回
 * 文档: docs/erp-phase-1.md §7.1/§7.2（采购审批驳回：轮次加一回到首节点）；
 *       approval-workflow-contract.md §4.3/§4.4、§11；workbench-workitem-contract.md 第 3 节
 * 账号: admin（补采购责任默认调度人）→ xiaoshou（客户/合同/销售单）
 *       → caigou（销售单采购确认、供给分配）→ caiwu（采购单审批驳回后再通过）
 *       → cangchu / fukuan 仅负向断言（驳回未生效前不得履约、不得付款）
 *
 * 文档-代码差异（以代码为准）:
 * 1. 文档 7.1 把「确认供给分配创建采购单」和「采购提交采购单」分成两步；
 *    代码在供给分配确认同一事务内建单并立即提交，不得留下未提交草稿。
 * 2. 文档写采购单 subject_version=approval_subject_version，驳回不改变它；
 *    页头「版本」展示的是 revision_no（尚未生效 / vN），详情 DTO 未透出
 *    approval_subject_version。本流程用提交身份 current_submission_id、
 *    明细数量/含税合计与审批实例 id 断言内容与版本冻结。
 * 3. 文档禁止对尚未生效的驳回单开采购变更单；代码 START_CHANGE 仅 EFFECTIVE/
 *    PARTIAL。审批中页头不渲染「发起采购变更」，变更分区渲染 disabled 按钮，
 *    actionBlockers 映射为空，回落文案「当前状态下不能发起变更，可先完成前置条件。」
 * 4. 合同 4.4.2 删除 PurchaseReviewStatus；前端仍并列「审批」轨（审批中/已通过/
 *    已驳回）。驳回后主状态与审核轨都保持「审批中」，不会变成「已驳回」。
 */
import fs from "node:fs"
import path from "node:path"

import {
    test,
    expect,
    type Browser,
    type BrowserContext,
    type Locator,
    type Page,
} from "@playwright/test"

import { ACCOUNTS } from "../helpers/accounts"
import { apiGet, apiLogin } from "../helpers/api"
import { loginViaUi, newLoggedInContext } from "../helpers/login"
import "../helpers/ui"

const VISIBLE = { timeout: 20_000 } as const
const FLOW_TIMEOUT = 12 * 60 * 1000
const API_BASE = process.env.API_BASE ?? "http://127.0.0.1:10001"
const SKU_KEYWORD = "龙井"
const SKU_NAME = "狮峰明前龙井礼盒"
const WAREHOUSE_NAME = "北京通州仓"
const SALES_QTY = "2"
const REJECT_REASON = "供应商报价超预算，本轮采购单不通过"

const CONTRACT_PDF = path.resolve(process.cwd(), "fixtures/sample-contract.pdf")
const MINIMAL_PDF = Buffer.from(
    "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000052 00000 n \n0000000101 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n178\n%%EOF\n",
)

type LoginName = "xiaoshou" | "caigou" | "cangchu" | "caiwu" | "fukuan" | "admin"

type Session = { context: BrowserContext; page: Page }

type PurchaseCenter = {
    id?: string
    purchase_no?: string
    status?: string
    review_status?: string
    current_submission_id?: string | null
    current_revision_id?: string | null
    revision_no?: number | null
    content_source?: string
    lines?: Array<{
        product_name?: string | null
        quantity?: string | null
        unit_cost_gross?: string | null
        gross_amount?: string
    }>
    totals?: { gross?: string; net?: string; tax?: string }
    payable_summary?: {
        payable_open_amount?: string
        paid_allocated_amount?: string
    } | null
    approval?: {
        instance?: {
            id?: string
            status?: string
            current_round_no?: number
            current_node_name?: string | null
            current_node?: string | null
            latest_rejection?: string | null
        } | null
        recent_history?: Array<{
            round_no?: number
            node_name?: string
            result?: string
            decision_reason?: string | null
        }>
    } | null
    changes?: Array<{ change_id?: string; status?: string }>
}

type PurchaseSnapshot = {
    id: string
    purchaseNo: string
    submissionId: string
    instanceId: string
    quantity: string
    unitCostGross: string
    gross: string
    net: string
    tax: string
}

test.describe.configure({ mode: "serial" })
test.use({ viewport: { width: 1440, height: 900 } })

function accountCred(login: LoginName): { account: string; password: string } {
    const bag = ACCOUNTS as Record<
        string,
        { account?: string; password?: string } | undefined
    >
    const aliases: Record<LoginName, string[]> = {
        xiaoshou: ["xiaoshou", "sales"],
        caigou: ["caigou", "procurement"],
        cangchu: ["cangchu", "warehouse"],
        caiwu: ["caiwu", "finance"],
        fukuan: ["fukuan", "payment"],
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
    try {
        const raw = await newLoggedInContext(browser, cred as never)
        const session = asSession(raw)
        if (session.page.url().includes("/login")) {
            await loginViaUi(session.page, cred as never)
        }
        await session.page.goto("/workspace")
        await expect(session.page.getByRole("heading", { name: "我的工作台" })).toBeVisible(
            VISIBLE,
        )
        return session
    } catch {
        const context = await browser.newContext()
        const page = await context.newPage()
        await loginViaUi(page, cred as never)
        await page.goto("/workspace")
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible(VISIBLE)
        return { context, page }
    }
}

async function closeSession(session: Session | undefined): Promise<void> {
    if (!session) return
    await session.context.close()
}

function contractFile(): string | { name: string; mimeType: string; buffer: Buffer } {
    if (fs.existsSync(CONTRACT_PDF)) return CONTRACT_PDF
    return {
        name: "sample-contract.pdf",
        mimeType: "application/pdf",
        buffer: MINIMAL_PDF,
    }
}

function plusDaysIso(days: number): string {
    const date = new Date()
    date.setDate(date.getDate() + days)
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function uniqueCreditCode(stamp: string): string {
    const raw = `91${stamp.replace(/[^0-9A-Za-z]/g, "").toUpperCase()}E2EPOREJ`
    return raw.slice(0, 18).padEnd(18, "0")
}

function unwrapData<T>(raw: unknown): T {
    if (raw && typeof raw === "object" && "data" in raw) {
        const data = (raw as { data?: T | null }).data
        if (data != null) return data
    }
    return raw as T
}

async function helperToken(login: LoginName): Promise<string> {
    const cred = accountCred(login)
    const attempts: unknown[] = [cred.account, cred, login]
    for (const input of attempts) {
        try {
            const raw = await (apiLogin as (value: unknown) => Promise<unknown>)(input)
            if (typeof raw === "string" && raw.trim()) return raw
            if (raw && typeof raw === "object" && "token" in raw) {
                const token = String((raw as { token?: string }).token ?? "")
                if (token) return token
            }
        } catch {
            // 兼容 helpers/api 入参是登录名或凭据对象
        }
    }
    const response = await fetch(`${API_BASE}/login`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
            account: cred.account,
            password: cred.password,
            account_kind: "admin",
        }),
    })
    const payload = (await response.json()) as { data?: { token?: string } }
    const token = payload.data?.token
    if (!response.ok || !token) {
        throw new Error(`API 登录失败: ${cred.account}`)
    }
    return token
}

async function helperGet<T>(token: string, apiPath: string): Promise<T> {
    try {
        const raw = await (
            apiGet as (auth: string, pathName: string, query?: unknown) => Promise<unknown>
        )(token, apiPath)
        return unwrapData<T>(raw)
    } catch {
        const response = await fetch(`${API_BASE}${apiPath}`, {
            headers: { Authorization: `Bearer ${token}` },
        })
        const payload = (await response.json()) as { data?: T; success?: boolean }
        if (!response.ok || payload.success === false) {
            throw new Error(`API GET ${apiPath} 失败 HTTP ${response.status}`)
        }
        return unwrapData<T>(payload)
    }
}

async function fetchPurchaseCenter(token: string, purchaseOrderId: string): Promise<PurchaseCenter> {
    return helperGet<PurchaseCenter>(
        token,
        `/admin/purchase-orders/${encodeURIComponent(purchaseOrderId)}`,
    )
}

async function listPurchasesBySalesOrder(
    token: string,
    salesOrderId: string,
): Promise<Array<{ id: string; purchase_no?: string; status?: string }>> {
    const raw = await helperGet<
        | { items?: Array<{ id: string; purchase_no?: string; status?: string }> }
        | Array<{ id: string; purchase_no?: string; status?: string }>
    >(
        token,
        `/admin/purchase-orders?sales_order_id=${encodeURIComponent(salesOrderId)}&page=1&page_size=20`,
    )
    if (Array.isArray(raw)) return raw
    return raw.items ?? []
}

async function expectToast(page: Page, title: string | RegExp): Promise<void> {
    const toast = page.locator('[data-slot="toast"]').filter({ hasText: title })
    await expect(toast.first()).toBeVisible(VISIBLE)
}

async function chooseOption(
    page: Page,
    input: Locator,
    option: string | RegExp,
): Promise<void> {
    await input.click()
    if (typeof option === "string") {
        await input.fill(option)
    }
    const listed = page.getByRole("option", { name: option }).first()
    if (await listed.count()) {
        await expect(listed).toBeVisible(VISIBLE)
        await listed.click()
        return
    }
    await page
        .locator('[data-slot="combobox-item"]')
        .filter({ hasText: option })
        .first()
        .click()
}

async function pickCalendarDay(page: Page, trigger: Locator, isoDate: string): Promise<void> {
    await trigger.click()
    const calendar = page.locator('[data-slot="calendar"]').last()
    await expect(calendar).toBeVisible(VISIBLE)
    const dayId = calendar.locator(`[id$="-day-${isoDate}"]`).first()
    if ((await dayId.count()) === 0) {
        const next = calendar.getByRole("button", {
            name: /next month|go to the next month|下个月|下一月/i,
        })
        if (await next.count()) {
            await next.first().click()
        } else {
            const fieldId = await trigger.getAttribute("id")
            if (fieldId) {
                const nextById = page.locator(`#${fieldId}-next-month`)
                if (await nextById.count()) await nextById.click()
            }
        }
    }
    const target = page.locator(`[id$="-day-${isoDate}"]`).first()
    if (await target.count()) {
        await expect(target).toBeVisible(VISIBLE)
        await target.click()
        return
    }
    const day = String(new Date(`${isoDate}T00:00:00`).getDate())
    await calendar.getByRole("button", { name: day, exact: true }).first().click()
}

async function fillEmptyDatePickers(page: Page, isoDate: string): Promise<void> {
    const empty = page.getByRole("button", { name: "选择日期" })
    const total = await empty.count()
    for (let index = 0; index < total; index += 1) {
        const remaining = page.getByRole("button", { name: "选择日期" })
        if ((await remaining.count()) === 0) break
        await pickCalendarDay(page, remaining.first(), isoDate)
    }
}

function documentHeader(page: Page): Locator {
    return page.locator('[data-slot="document-header"]')
}

async function readDocumentNumber(page: Page): Promise<string> {
    const text = (await documentHeader(page).locator("span.num").first().innerText()).trim()
    expect(text.length).toBeGreaterThan(2)
    return text
}

async function openWorkspaceTask(
    page: Page,
    family: "审批" | "采购" | "履约" | "财务",
    name: RegExp,
    query?: string,
): Promise<void> {
    await page.goto("/workspace")
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible(VISIBLE)
    const familyId =
        family === "审批"
            ? "approval"
            : family === "采购"
              ? "procurement"
              : family === "履约"
                ? "fulfillment"
                : "finance"
    await page.locator(`#workspace-family-nav-${familyId}`).click()
    if (query) {
        const search = page.locator("#workspace-queue-toolbar-search-input")
        await search.fill(query)
        await search.press("Enter")
    }
    const list = page.getByRole("list", { name: "待办列表" })
    const task = list.getByRole("button", { name })
    await expect(task).toBeVisible(VISIBLE)
    await task.click()
    await expect(
        page
            .getByRole("region", { name: "当前供给分配任务", exact: true })
            .or(page.getByRole("region", { name: "当前任务", exact: true })),
    ).toBeVisible(VISIBLE)
}

function approvalPane(page: Page): Locator {
    return page.getByRole("region", { name: "当前任务", exact: true })
}

async function approveCurrentDocument(page: Page): Promise<void> {
    const pane = approvalPane(page)
    await expect(pane.getByRole("button", { name: "通过", exact: true })).toBeVisible(VISIBLE)
    await expect(pane.getByRole("button", { name: "驳回", exact: true })).toBeVisible()
    await pane.getByRole("button", { name: "通过", exact: true }).click()
    const dialog = page.getByRole("dialog", { name: "确认通过" })
    await expect(dialog).toBeVisible(VISIBLE)
    await dialog.getByRole("button", { name: "确认通过" }).click()
    await expect(dialog).toBeHidden(VISIBLE)
}

async function rejectCurrentDocument(page: Page, reason: string): Promise<void> {
    const pane = approvalPane(page)
    await expect(pane.getByRole("button", { name: "驳回", exact: true })).toBeVisible(VISIBLE)
    await pane.getByRole("button", { name: "驳回", exact: true }).click()
    const dialog = page.getByRole("dialog", { name: "确认驳回" })
    await expect(dialog).toBeVisible(VISIBLE)
    await expect(dialog.getByText("确认驳回后将重启审批流程。")).toBeVisible()
    await dialog.getByLabel("驳回原因").fill(reason)
    await dialog.getByRole("button", { name: "确认驳回" }).click()
    await expect(dialog).toBeHidden(VISIBLE)
}

async function ensureDefaultProcurementOwner(page: Page): Promise<void> {
    await page.goto("/master-data/procurement-responsibilities")
    await expect(page.getByRole("heading", { name: "采购责任规则" })).toBeVisible(VISIBLE)
    if (await page.getByText("默认调度人").count()) return
    await page.locator("#procurement-responsibility-rules-create").click()
    const dialog = page.getByRole("dialog", { name: "新增采购责任规则" })
    await expect(dialog).toBeVisible(VISIBLE)
    await chooseOption(page, dialog.getByLabel("规则类型"), "默认调度人")
    await chooseOption(page, dialog.getByLabel("采购负责人"), /采购|caigou/)
    await dialog.getByRole("button", { name: "保存规则" }).click()
    await expectToast(page, /采购责任规则已新增|采购责任规则已更新/)
    await expect(dialog).toBeHidden(VISIBLE)
}

async function snapshotFromCenter(center: PurchaseCenter): Promise<PurchaseSnapshot> {
    const line = center.lines?.find((item) => item.product_name?.includes(SKU_NAME)) ?? center.lines?.[0]
    const id = String(center.id ?? "")
    const purchaseNo = String(center.purchase_no ?? "")
    const submissionId = String(center.current_submission_id ?? "")
    const instanceId = String(center.approval?.instance?.id ?? "")
    expect(id.length).toBeGreaterThan(8)
    expect(purchaseNo.length).toBeGreaterThan(2)
    expect(submissionId.length).toBeGreaterThan(2)
    expect(instanceId.length).toBeGreaterThan(2)
    return {
        id,
        purchaseNo,
        submissionId,
        instanceId,
        quantity: String(line?.quantity ?? ""),
        unitCostGross: String(line?.unit_cost_gross ?? ""),
        gross: String(center.totals?.gross ?? ""),
        net: String(center.totals?.net ?? ""),
        tax: String(center.totals?.tax ?? ""),
    }
}

function expectSameContent(live: PurchaseCenter, snap: PurchaseSnapshot): void {
    expect(String(live.status)).toMatch(/IN_APPROVAL|PENDING_FINANCE_REVIEW/)
    expect(String(live.current_submission_id ?? "")).toBe(snap.submissionId)
    expect(String(live.approval?.instance?.id ?? "")).toBe(snap.instanceId)
    expect(live.revision_no ?? null).toBeNull()
    expect(live.current_revision_id ?? null).toBeNull()
    expect(live.payable_summary ?? null).toBeNull()
    expect(live.changes ?? []).toEqual([])
    const line = live.lines?.find((item) => item.product_name?.includes(SKU_NAME)) ?? live.lines?.[0]
    expect(String(line?.quantity ?? "")).toBe(snap.quantity)
    expect(String(line?.unit_cost_gross ?? "")).toBe(snap.unitCostGross)
    expect(String(live.totals?.gross ?? "")).toBe(snap.gross)
    expect(String(live.totals?.net ?? "")).toBe(snap.net)
    expect(String(live.totals?.tax ?? "")).toBe(snap.tax)
}

test("[flow-14] 采购单审批驳回后轮次加一，不改单再通过才生效并形成应付", async ({
    browser,
}) => {
    test.setTimeout(FLOW_TIMEOUT)
    const stamp = Date.now().toString(36).toUpperCase()
    const customerName = `E2E采购驳回客户${stamp}`
    const contractNo = `HT-E2E-PO-REJ-${stamp}`
    const dueDate = plusDaysIso(21)
    let salesOrderId = ""
    let salesOrderNo = ""
    let purchaseHref = ""
    let snap: PurchaseSnapshot | undefined
    let session: Session | undefined

    const switchTo = async (login: LoginName) => {
        await closeSession(session)
        session = await openSession(browser, login)
        return session.page
    }

    try {
        // 0) 销售提交实物单前必须能解析采购负责人
        let page = await switchTo("admin")
        await ensureDefaultProcurementOwner(page)

        // 1) 客户 + 合同 PDF + 实物销售单提交（付款条件用货到，避免本流程走先款履约）
        page = await switchTo("xiaoshou")
        await page.goto("/sales/customers")
        await expect(page.getByRole("heading", { name: "客户中心" })).toBeVisible(VISIBLE)
        await page.locator("#customers-directory-create").click()
        const customerDialog = page.getByRole("dialog", { name: "新建客户" })
        await expect(customerDialog).toBeVisible(VISIBLE)
        await customerDialog.getByLabel("法定名称").fill(customerName)
        await customerDialog.getByLabel("客户简称").fill(`驳回${stamp.slice(-6)}`)
        await customerDialog.getByLabel("统一社会信用代码").fill(uniqueCreditCode(stamp))
        const paymentTerm = customerDialog.getByLabel("默认付款条件")
        if (await paymentTerm.count()) {
            await chooseOption(page, paymentTerm, /货到 15 天|按合同约定/)
        }
        await customerDialog.locator("#customers-form-submit").click()
        await expectToast(page, "客户已创建")
        await expect(customerDialog).toBeHidden(VISIBLE)

        await page.goto("/sales/orders?mode=create")
        await expect(page.getByText("单据头")).toBeVisible(VISIBLE)
        await page.getByRole("button", { name: "上传合同 PDF" }).click()
        const contractDialog = page.getByRole("dialog", { name: "上传合同 PDF" })
        await expect(contractDialog).toBeVisible(VISIBLE)
        await contractDialog.locator("#card-contracts-upload-pdf-input").setInputFiles(contractFile())
        await contractDialog.getByLabel("合同编号").fill(contractNo)
        await chooseOption(
            page,
            contractDialog.locator("#card-contracts-upload-customer"),
            new RegExp(customerName),
        )
        const settlement = contractDialog.locator("#card-contracts-upload-settlement-party")
        if (await settlement.count()) {
            await expect(settlement).not.toHaveValue("", VISIBLE)
        }
        const contractPayment = contractDialog.getByLabel("付款条件")
        if (await contractPayment.count()) {
            await chooseOption(page, contractPayment, /货到 15 天|按合同约定/)
        }
        await contractDialog.locator("#card-contracts-upload-submit").click()
        await expectToast(page, "合同 PDF 已归档")
        await expect(contractDialog).toBeHidden(VISIBLE)
        await expect(page.getByText(customerName).first()).toBeVisible(VISIBLE)

        await chooseOption(page, page.getByLabel("福利场景"), "年节礼包")
        const salesPayment = page.locator("#sales-orders-create-header-payment-terms")
        if (!(await salesPayment.inputValue().catch(() => "")).trim()) {
            await chooseOption(page, salesPayment, /货到 15 天|按合同约定/)
        }
        await expect(page.getByLabel("供应商")).toHaveCount(0)
        await expect(page.getByLabel("履约责任")).toHaveCount(0)
        await expect(page.getByLabel("采购成本")).toHaveCount(0)

        await page.getByRole("button", { name: "选择商品" }).click()
        const skuDialog = page.getByRole("dialog", { name: "选择商品" })
        await expect(skuDialog).toBeVisible(VISIBLE)
        const skuSearch = skuDialog.getByPlaceholder("搜索 SKU、商品名称、编号或规格")
        await skuSearch.fill(SKU_KEYWORD)
        await skuSearch.press("Enter")
        const skuRow = skuDialog.getByRole("checkbox", { name: new RegExp(SKU_NAME) })
        await expect(skuRow.first()).toBeVisible(VISIBLE)
        await skuRow.first().check()
        await skuDialog.locator("#sales-orders-sku-picker-confirm").click()
        await expect(skuDialog).toBeHidden(VISIBLE)
        await expect(page.getByText(SKU_NAME).first()).toBeVisible(VISIBLE)
        await page.getByLabel("数量").fill(SALES_QTY)
        await pickCalendarDay(page, page.locator("#sales-orders-create-batch-due-date"), dueDate)
        await page.locator("#sales-orders-create-batch-due-date-apply").click()
        await expectToast(page, "已批量设置交期")
        await expect(page.getByText("暂未确定采购负责人")).toHaveCount(0)

        await page.locator("#sales-orders-create-submit").click()
        const submitDialog = page.getByRole("dialog", { name: "提交销售单" })
        await expect(submitDialog).toBeVisible(VISIBLE)
        await expect(submitDialog.getByText("审批中")).toBeVisible()
        await submitDialog.locator("#sales-orders-submit-confirm-confirm").click()
        await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, VISIBLE)
        salesOrderId = page.url().split("/sales/orders/")[1]?.split("?")[0] ?? ""
        expect(salesOrderId).toBeTruthy()
        await expect(page.getByRole("heading", { name: customerName })).toBeVisible(VISIBLE)
        await expect(documentHeader(page).getByText("审批中")).toBeVisible(VISIBLE)
        salesOrderNo = await readDocumentNumber(page)
        await expect(page.getByText(/采购单 0 笔/)).toBeVisible()

        // 2) 采购确认节点：只通过/驳回，不选源
        page = await switchTo("caigou")
        await openWorkspaceTask(page, "审批", new RegExp(`销售单审批[\\s\\S]*${salesOrderNo}`), salesOrderNo)
        await expect(page.getByRole("heading", { name: /销售单/ })).toBeVisible(VISIBLE)
        await expect(page.getByText("第 1 轮").first()).toBeVisible(VISIBLE)
        await expect(page.getByText("采购确认").first()).toBeVisible(VISIBLE)
        await expect(page.getByLabel("供给来源 / 履约责任")).toHaveCount(0)
        await expect(page.getByLabel("含税成本")).toHaveCount(0)
        await expect(page.getByLabel("预计交付日")).toHaveCount(0)
        await approveCurrentDocument(page)

        // 3) 供给分配：创建采购单并立即提交审批
        await page.locator("#workspace-home-refresh").click()
        await openWorkspaceTask(
            page,
            "采购",
            new RegExp(`待供给分配[\\s\\S]*${salesOrderNo}`),
            salesOrderNo,
        )
        await expect(page.getByRole("heading", { name: "供给分配" })).toBeVisible(VISIBLE)
        await expect(page.getByRole("heading", { name: "销售明细与供给方案" })).toBeVisible(VISIBLE)
        await page.getByTestId("purchase-create-match-best").click()
        await expectToast(page, /已重新分配供给|没有可匹配的供给方案/)

        const warehouseField = page.getByLabel(/采购入库目标仓/)
        if ((await warehouseField.count()) === 0) {
            const sourcing = page.getByLabel(/履约方案/)
            await sourcing.click()
            const inbound = page
                .getByRole("option", { name: /入仓/ })
                .or(page.locator('[data-slot="combobox-item"]').filter({ hasText: "入仓" }))
            await expect(inbound.first()).toBeVisible(VISIBLE)
            await inbound.first().click()
        }
        await expect(page.getByLabel(/采购入库目标仓/)).toBeVisible(VISIBLE)
        if (await page.getByPlaceholder("选择目标仓").count()) {
            await chooseOption(page, page.getByLabel(/采购入库目标仓/), WAREHOUSE_NAME)
        }
        await fillEmptyDatePickers(page, dueDate)
        await expect(page.getByText("将创建采购单").locator("xpath=..")).toContainText("1 张")
        await expect(page.getByText("将建立库存预留").locator("xpath=..")).toContainText("0 条")

        await page.locator("#procurement-orders-create-preview").click()
        const preview = page.getByRole("dialog", { name: "预览供给分配" })
        await expect(preview).toBeVisible(VISIBLE)
        await expect(preview.getByText("现有库存分配")).toHaveCount(0)
        await expect(preview.getByText("无需创建采购单")).toHaveCount(0)
        await expect(preview.getByText(/确认提交 1 张采购单/)).toBeVisible()
        await preview.locator("#procurement-orders-create-preview-confirm").click()
        const confirmAlloc = page.getByRole("alertdialog").or(page.getByRole("dialog")).filter({
            hasText: "确认供给分配",
        })
        await expect(confirmAlloc.first()).toBeVisible(VISIBLE)
        await expect(confirmAlloc.getByText(/创建 1 张采购单提交审批/)).toBeVisible()
        await page.locator("#procurement-orders-create-confirm").click()
        await expectToast(page, /供给分配已完成|已创建 1 张采购单并提交审批/)
        await expect(
            page.locator('[data-slot="toast-description"]').filter({ hasText: /无需采购/ }),
        ).toHaveCount(0)

        const caigouToken = await helperToken("caigou")
        let listed = await listPurchasesBySalesOrder(caigouToken, salesOrderId)
        if (listed.length === 0) {
            await page.goto("/procurement/orders")
            await expect(page.getByRole("heading", { name: "采购单" })).toBeVisible(VISIBLE)
            await page.locator("#procurement-orders-list-search").fill(salesOrderNo)
            await page.locator("#procurement-orders-list-search").press("Enter")
            const fallbackOpen = page.getByRole("link", { name: /打开采购单/ })
            await expect(fallbackOpen).toBeVisible(VISIBLE)
            await fallbackOpen.click()
            await expect(page).toHaveURL(/\/procurement\/orders\/[^/?#]+/, VISIBLE)
            const fallbackId = page.url().split("/procurement/orders/")[1]?.split("?")[0] ?? ""
            listed = [{ id: fallbackId, status: "IN_APPROVAL" }]
        }
        expect(listed.length, "供给分配必须创建恰好 1 张采购单").toBe(1)
        expect(String(listed[0]?.status ?? "IN_APPROVAL")).toMatch(
            /IN_APPROVAL|PENDING_FINANCE_REVIEW/,
        )
        const created = await fetchPurchaseCenter(caigouToken, listed[0]!.id)
        snap = await snapshotFromCenter(created)
        if (!snap) throw new Error("采购单快照失败")
        expect(String(created.status)).toMatch(/IN_APPROVAL|PENDING_FINANCE_REVIEW/)
        expect(created.payable_summary ?? null).toBeNull()
        expect(created.approval?.instance?.current_round_no).toBe(1)
        expect(
            created.approval?.instance?.current_node_name ?? created.approval?.instance?.current_node,
        ).toMatch(/财务总监审批/)
        expect(created.content_source).toBe("SUBMISSION")

        page = await switchTo("xiaoshou")
        await page.goto(`/sales/orders/${salesOrderId}`)
        await expect(documentHeader(page).getByText("已生效")).toBeVisible(VISIBLE)
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("采购单 1 笔")
        await page.getByRole("tab", { name: /^采购/ }).click()
        await expect(page.getByText("草稿")).toHaveCount(0)
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("审批中")

        // 4) 财务在采购单审批首节点驳回
        page = await switchTo("caiwu")
        await openWorkspaceTask(
            page,
            "审批",
            new RegExp(`采购单审批[\\s\\S]*${snap.purchaseNo}|采购单审批[\\s\\S]*${salesOrderNo}`),
            snap.purchaseNo,
        )
        const roundOne = approvalPane(page)
        await expect(roundOne.getByText("第 1 轮")).toBeVisible(VISIBLE)
        await expect(roundOne.getByText("财务总监审批")).toBeVisible(VISIBLE)
        await expect(roundOne.getByText(SKU_NAME).first()).toBeVisible()
        await rejectCurrentDocument(page, REJECT_REASON)

        await page.locator("#workspace-home-refresh").click()
        await openWorkspaceTask(
            page,
            "审批",
            new RegExp(`采购单审批[\\s\\S]*${snap.purchaseNo}|采购单审批[\\s\\S]*${salesOrderNo}`),
            snap.purchaseNo,
        )
        const roundTwo = approvalPane(page)
        await expect(roundTwo.getByText("第 2 轮")).toBeVisible(VISIBLE)
        await expect(roundTwo.getByText("财务总监审批")).toBeVisible(VISIBLE)
        await expect(roundTwo.getByText("最近驳回")).toBeVisible(VISIBLE)
        await expect(roundTwo.getByText(REJECT_REASON)).toBeVisible(VISIBLE)

        // 5) 驳回后：不生效、不形成应付、内容不变、禁止变更单/履约/付款
        page = await switchTo("caigou")
        await page.goto("/procurement/orders")
        await expect(page.getByRole("heading", { name: "采购单" })).toBeVisible(VISIBLE)
        await page.locator("#procurement-orders-list-search").fill(salesOrderNo)
        await page.locator("#procurement-orders-list-search").press("Enter")
        const openPo = page.getByRole("link", { name: new RegExp(`打开采购单 ${snap.purchaseNo}`) })
        await expect(openPo).toBeVisible(VISIBLE)
        await openPo.click()
        await expect(page).toHaveURL(/\/procurement\/orders\/[^/?#]+/, VISIBLE)
        purchaseHref = page.url().split("?")[0] ?? page.url()
        await expect(documentHeader(page).getByText("审批中").first()).toBeVisible(VISIBLE)
        await expect(documentHeader(page).getByText("版本 尚未生效")).toBeVisible(VISIBLE)
        await expect(documentHeader(page).getByText("已生效")).toHaveCount(0)
        await expect(documentHeader(page).getByText("未付")).toBeVisible()
        await expect(documentHeader(page).getByText("未开始")).toBeVisible()
        await expect(page.locator("#procurement-orders-detail-change")).toHaveCount(0)
        await expect(page.locator("#procurement-orders-detail-submit")).toHaveCount(0)
        await expect(page.getByRole("button", { name: "去交付" })).toHaveCount(0)
        await expect(page.getByRole("button", { name: "提交审批" })).toHaveCount(0)

        await page.getByRole("tab", { name: /^概览/ }).click()
        await expect(page.getByText(SKU_NAME).first()).toBeVisible(VISIBLE)
        await expect(page.getByText(new RegExp(`${snap.quantity}`)).first()).toBeVisible()
        await expect(page.getByText("已提交内容")).toBeVisible()
        await expect(page.getByText("生效版本")).toHaveCount(0)

        await page.getByRole("tab", { name: /^审批/ }).click()
        await expect(page.getByText("第 2 轮").first()).toBeVisible(VISIBLE)
        await expect(page.getByText("财务总监审批").first()).toBeVisible(VISIBLE)
        await expect(page.getByText("最近驳回")).toBeVisible(VISIBLE)
        await expect(page.getByText(REJECT_REASON)).toBeVisible(VISIBLE)
        await expect(page.getByText("已驳回").first()).toBeVisible()
        await expect(page.getByText("第 1 轮").first()).toBeVisible()

        await page.getByRole("tab", { name: /^应付与票款/ }).click()
        await expect(page.getByText("尚未形成应付（需审批通过）。")).toBeVisible(VISIBLE)
        await expect(page.getByText("应付未结")).toHaveCount(0)

        await page.getByRole("tab", { name: /^履约/ }).click()
        await expect(page.getByRole("button", { name: "履约入口未开放" })).toBeDisabled()
        await expect(page.getByText("当前状态下不能进入交付，可先完成前置条件。")).toBeVisible()

        await page.getByRole("tab", { name: /^变更与异常/ }).click()
        await expect(page.getByText("暂无采购变更。")).toBeVisible(VISIBLE)
        const disabledChange = page.locator('[id^="procurement-orders-detail-changes-disabled-"]')
        await expect(disabledChange).toBeVisible(VISIBLE)
        await expect(disabledChange).toBeDisabled()
        await expect(page.getByText("当前状态下不能发起变更，可先完成前置条件。")).toBeVisible()

        const rejected = await fetchPurchaseCenter(caigouToken, snap.id)
        expectSameContent(rejected, snap)
        expect(rejected.approval?.instance?.status).toBe("RUNNING")
        expect(rejected.approval?.instance?.current_round_no).toBe(2)
        expect(rejected.approval?.instance?.latest_rejection).toBe(REJECT_REASON)
        expect(
            rejected.approval?.instance?.current_node_name ?? rejected.approval?.instance?.current_node,
        ).toMatch(/财务总监审批/)
        const roundOneHistory = (rejected.approval?.recent_history ?? []).filter(
            (item) => item.round_no === 1,
        )
        expect(roundOneHistory.some((item) => item.result === "REJECTED")).toBeTruthy()
        expect(
            roundOneHistory.some((item) => item.decision_reason === REJECT_REASON),
        ).toBeTruthy()

        page = await switchTo("cangchu")
        await page.goto("/workspace")
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible(VISIBLE)
        await page.locator("#workspace-family-nav-fulfillment").click()
        const warehouseSearch = page.locator("#workspace-queue-toolbar-search-input")
        await warehouseSearch.fill(salesOrderNo)
        await warehouseSearch.press("Enter")
        await expect(
            page.getByRole("list", { name: "待办列表" }).getByRole("button", { name: /履约处理/ }),
        ).toHaveCount(0)

        page = await switchTo("fukuan")
        await page.goto("/workspace")
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible(VISIBLE)
        await page.locator("#workspace-family-nav-finance").click()
        const cashierSearch = page.locator("#workspace-queue-toolbar-search-input")
        await cashierSearch.fill(salesOrderNo)
        await cashierSearch.press("Enter")
        await expect(
            page
                .getByRole("list", { name: "待办列表" })
                .getByRole("button", { name: /供应商付款处理/ }),
        ).toHaveCount(0)
        await page.locator("#workspace-family-nav-approval").click()
        await expect(page.getByText("供应商付款单审批")).toHaveCount(0)

        // 6) 不改单、不撤回：财务在下一轮首节点通过 → 采购单生效并形成应付
        page = await switchTo("caiwu")
        await openWorkspaceTask(
            page,
            "审批",
            new RegExp(`采购单审批[\\s\\S]*${snap.purchaseNo}|采购单审批[\\s\\S]*${salesOrderNo}`),
            snap.purchaseNo,
        )
        await expect(approvalPane(page).getByText("第 2 轮")).toBeVisible(VISIBLE)
        await approveCurrentDocument(page)

        page = await switchTo("caigou")
        await page.goto(purchaseHref)
        await expect(documentHeader(page).getByText("已生效").first()).toBeVisible(VISIBLE)
        await expect(documentHeader(page).getByText(/版本\s+v1/)).toBeVisible(VISIBLE)
        await expect(page.locator("#procurement-orders-detail-pay")).toBeVisible(VISIBLE)
        await expect(page.locator("#procurement-orders-detail-change")).toBeVisible(VISIBLE)
        await expect(page.locator("#procurement-orders-detail-change")).toBeEnabled()
        await page.getByRole("tab", { name: /^应付与票款/ }).click()
        await expect(page.getByText("应付未结")).toBeVisible(VISIBLE)
        await expect(page.getByText("尚未形成应付（需审批通过）。")).toHaveCount(0)

        const effective = await fetchPurchaseCenter(caigouToken, snap.id)
        expect(String(effective.status)).toBe("EFFECTIVE")
        expect(effective.revision_no).toBe(1)
        expect(String(effective.current_submission_id ?? "")).toBe(snap.submissionId)
        expect(String(effective.approval?.instance?.id ?? "")).toBe(snap.instanceId)
        expect(effective.approval?.instance?.status).toBe("APPROVED")
        expect(effective.approval?.instance?.current_round_no).toBe(2)
        expect(effective.payable_summary).toBeTruthy()
        expect(Number(effective.payable_summary?.payable_open_amount ?? "0")).toBeGreaterThan(0)
        expect(String(effective.totals?.gross ?? "")).toBe(snap.gross)
        const effectiveLine =
            effective.lines?.find((item) => item.product_name?.includes(SKU_NAME)) ??
            effective.lines?.[0]
        expect(String(effectiveLine?.quantity ?? "")).toBe(snap.quantity)

        page = await switchTo("xiaoshou")
        await page.goto(`/sales/orders/${salesOrderId}`)
        await expect(documentHeader(page).getByText("已生效")).toBeVisible(VISIBLE)
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("采购单 1 笔")
        await expect(page.getByTestId("sales-order-purchase-status")).toContainText("已生效")
    } finally {
        await closeSession(session)
    }
})
