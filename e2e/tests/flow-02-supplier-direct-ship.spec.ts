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

import { archiveContractViaUi } from "../helpers/contracts"
import { test, expect, type Page } from "../helpers/test"
import fs from "node:fs"
import path from "node:path"

import { apiGet, apiToken } from "../helpers/api"
import { createCustomerViaUi } from "../helpers/customers"
import { fillDeliveryTrackingEntries, uploadAcceptanceEvidence } from "../helpers/fulfillment"
import { ensureWarehouseStockScope } from "../helpers/inventory"
import { openLoggedInWorkspace, type LoggedInSession } from "../helpers/login"
import { payOnlySupplierTask } from "../helpers/payments"
import { ensureDefaultProcurementOwner } from "../helpers/procurement"
import { confirmSupplyAllocation, expandSourcingEditor } from "../helpers/sourcing"
import {
    approveCurrentDocument,
    chooseOption,
    expectToast,
    openFulfillmentWorkspaceForm,
    openWorkspaceTask,
    pickCalendarDay,
    readHeaderDocumentNumber,
} from "../helpers/ui"

test.describe.configure({ mode: "serial" })

const SKU_KEYWORD = "狮峰明前龙井"
const SECOND_SKU_KEYWORD = "狮峰陈皮普洱"
const SUPPLIER_SHORT = "狮峰茶叶"
const DIRECT_OPTION = /狮峰茶叶.* · 供应商直发|供应商直发/
const WAREHOUSE_OPTION = /狮峰茶叶.* · 入仓| · 入仓/

const MINIMAL_PDF = Buffer.from(
    "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R>>endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000068 00000 n \n0000000125 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n210\n%%EOF\n",
)

type LoginName = "xiaoshou" | "caigou" | "caiwu" | "cangchu" | "fukuan" | "admin"

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

function purchaseOrderRow(page: Page, salesOrderNo: string) {
    return page
        .locator("#procurement-orders-list-table")
        .getByRole("row")
        .filter({ hasText: salesOrderNo })
}

async function gotoHeading(page: Page, pathName: string, heading: string | RegExp) {
    await page.goto(pathName)
    await expect(page.getByRole("heading", { name: heading })).toBeVisible({
        timeout: 20000,
    })
}

/** 代发不得给本单留下采购入库。仓储没有默认仓库范围时台账页签不挂载。全量套件里其他流程可以已有库存。 */
async function assertInventoryUntouched(page: Page, documentHint: string) {
    await ensureWarehouseStockScope("BJ-TZ-01")
    await gotoHeading(page, "/inventory", "库存台账")
    for (const view of ["balance", "movement", "reservation"] as const) {
        const tab = page.locator(`#inventory-ledger-view-${view}`)
        await expect(tab).toBeVisible({ timeout: 20000 })
        await tab.click()
        await expect(tab).toHaveAttribute("aria-pressed", "true", { timeout: 20000 })
        await expect(
            page
                .locator(`#inventory-ledger-${view}-table`)
                .getByRole("row")
                .filter({ hasText: documentHint })
                .filter({ hasText: "采购入库" }),
        ).toHaveCount(0)
    }
}

test("供应商直接发客户（代发）全流程", async ({ browser }) => {
    test.setTimeout(8 * 60 * 1000)

    const stamp = Date.now().toString()
    const legalName = `代发测试客户${stamp}`
    const shortName = `代发${stamp.slice(-6)}`
    const contractNo = `HT-DS-${stamp}`
    const dueDate = isoPlusDays(90)
    const trackingNo = `SF${stamp.slice(-10)}`
    const secondTrackingNo = `LL${stamp.slice(-10)}`
    const otherLineTrackingNo = `SF-SECOND-${stamp.slice(-10)}`
    const sharedTrackingNo = `SHARED-${stamp.slice(-10)}`
    let salesLineIds: string[] = []
    let salesOrderNo = ""
    let purchaseOrderNo = ""
    let acceptanceNo = ""
    let session: LoggedInSession | undefined

    const switchTo = async (login: LoginName) => {
        await session?.context.close()
        session = await openLoggedInWorkspace(browser, login)
        return session.page
    }

    try {
    // ── 0. 采购责任规则：默认调度人 → caigou（提交销售单前置） ──
    let page = await switchTo("admin")
    await ensureDefaultProcurementOwner(page)

    // ── 1. 销售：新建客户 ──
    page = await switchTo("xiaoshou")
    await createCustomerViaUi(page, {
        legalName,
        shortName,
        creditCode: creditCode(stamp),
        paymentTermLabel: "货到 30 天",
        contact: { name: "李测", phone: "13800138001" },
        address: "北京市朝阳区测试路 1 号",
    })
    // 列表行链接展示客户简称，非法定全称。
    await expect(page.getByRole("link", { name: shortName })).toBeVisible({
        timeout: 20000,
    })

    // ── 2. 销售：上传合同 PDF ──
    {
        await gotoHeading(page, "/sales/contracts", /^合同$/)
        await page.locator("#page-actions-action-upload").click()
        await archiveContractViaUi(page, { contractNo, customerName: legalName, pdf: contractPdf(), paymentTerms: "货到 30 天" })
        await expect(page.getByText(contractNo).first()).toBeVisible({ timeout: 20000 })
    }

    // ── 3. 销售：创建并提交实物销售单（账期/货到，不选源） ──
    {
        await page.goto("/sales/orders?mode=create")
        await expect(
            page.getByRole("heading", { name: "新建销售单" }).or(
                page.getByRole("heading", { name: "业务信息" }),
            ),
        ).toBeVisible({
            timeout: 20000,
        })
        await chooseOption(
            page,
            page.locator("#sales-orders-create-contract"),
            new RegExp(`${legalName}|${contractNo}`),
            contractNo,
        )
        await expect(page.locator("#sales-orders-create-customer")).toHaveValue(new RegExp(legalName))
        await chooseOption(
            page,
            page.locator("#sales-orders-create-header-welfare-scene"),
            "年节礼包",
            "年节",
        )
        await expect(page.locator("#sales-orders-create-header-payment-terms")).toHaveValue("货到 30 天")
    await expect(page.locator("#sales-orders-create-header-payment-terms")).toBeDisabled()

        await page.locator("#sales-orders-create-line-items-add").click()
        const skuDialog = page.getByRole("dialog", { name: /添加商品|更换销售商品/ })
        await expect(skuDialog).toBeVisible({ timeout: 20000 })
        const skuSearch = skuDialog.getByPlaceholder("搜索 SKU、商品名称、编号或规格")
        await skuSearch.fill(SKU_KEYWORD)
        await skuSearch.press("Enter")
        await expect(skuDialog.getByText(SKU_KEYWORD).first()).toBeVisible({ timeout: 20000 })
        await skuDialog.getByRole("checkbox", { name: new RegExp(SKU_KEYWORD) }).check()
        await skuDialog.locator("#sales-orders-sku-picker-confirm").click()
        await expect(skuDialog).toBeHidden({ timeout: 20000 })
        await expect(page.getByText(SKU_KEYWORD)).toBeVisible({ timeout: 20000 })

        await page.locator("#sales-orders-create-line-items-add").click()
        await expect(skuDialog).toBeVisible({ timeout: 20000 })
        await skuSearch.fill(SECOND_SKU_KEYWORD)
        await skuSearch.press("Enter")
        await skuDialog.getByRole("checkbox", { name: new RegExp(SECOND_SKU_KEYWORD) }).check()
        await skuDialog.locator("#sales-orders-sku-picker-confirm").click()
        await expect(skuDialog).toBeHidden({ timeout: 20000 })
        await expect(page.getByText(SECOND_SKU_KEYWORD)).toBeVisible({ timeout: 20000 })
        await expect(page.locator('[id^="sales-orders-create-line-"][id$="-quantity"]')).toHaveCount(2)
        const owners = page.locator('[data-testid^="sales-line-procurement-owner-"]')
        await expect(owners).toHaveCount(2)
        for (let index = 0; index < 2; index += 1) {
            await expect(owners.nth(index)).not.toContainText("暂未确定", { timeout: 20000 })
        }

        await page.locator("#sales-orders-create-batch-due-date-open").click()
        await pickCalendarDay(
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
        await expect(page.getByText(/审核中|审批中|待采购/).first()).toBeVisible({
            timeout: 20000,
        })
        salesOrderNo = await readHeaderDocumentNumber(page)
        expect(salesOrderNo.length).toBeGreaterThan(0)
        const salesOrderId = new URL(page.url()).pathname.split("/").at(-1)
        const order = await apiGet<{ lines: Array<{ id: string; line_no: number }> }>(
            await apiToken("xiaoshou"),
            `/admin/sales-orders/${salesOrderId}`,
        )
        expect(order.lines).toHaveLength(2)
        salesLineIds = order.lines.sort((left, right) => left.line_no - right.line_no).map(line => line.id)
        expect(new Set(salesLineIds).size).toBe(2)
    }

    // ── 4. 负向：生效前不得建采购单、不得履约、采购确认不得选源 ──
    {
        page = await switchTo("caigou")
        await gotoHeading(page, "/procurement/orders", "采购单")
        const poSearch = page.locator("#procurement-orders-list-search")
        await poSearch.fill(salesOrderNo)
        await poSearch.press("Enter")
        await expect(page.locator("#procurement-orders-list-table").getByText(salesOrderNo)).toHaveCount(0)

        await openWorkspaceTask(page, "销售单审批", salesOrderNo, "approval")
        await expect(page.getByText("供给来源 / 履约责任")).toHaveCount(0)
        await expect(page.getByRole("button", { name: "预览供给分配" })).toHaveCount(0)
        await expect(page.getByRole("button", { name: /^(通过|同意审批)$/ })).toBeVisible({
            timeout: 20000,
        })
        await approveCurrentDocument(page)
    }

    // ── 5. 销售单已生效；仓储此时不得出现入库任务 ──
    {
        page = await switchTo("xiaoshou")
        await page.goto("/sales/orders")
        await expect(page.getByRole("heading", { name: "销售单", exact: true })).toBeVisible({
            timeout: 20000,
        })
        const orderLink = page.getByRole("button", { name: `查看销售单 ${salesOrderNo}` })
        await expect(orderLink).toBeVisible({ timeout: 20000 })
        await expect(orderLink.locator("xpath=ancestor::tr[1]").getByText("已生效")).toBeVisible({
            timeout: 20000,
        })
    }
    {
        page = await switchTo("cangchu")
        await page.goto("/workspace?family=fulfillment")
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByRole("button", { name: /入库/ })).toHaveCount(0)
    }

    // ── 6. 采购：供给分配显式选「供应商直发」，不得走入仓 ──
    {
        page = await switchTo("caigou")
        await openWorkspaceTask(page, "待供给分配", salesOrderNo, "procurement")
        await expect(page.getByRole("heading", { name: "供给分配" })).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByText("销售明细与供给方案")).toBeVisible({
            timeout: 20000,
        })
        await expandSourcingEditor(page, new RegExp(SKU_KEYWORD))
        await expandSourcingEditor(page, new RegExp(SECOND_SKU_KEYWORD))

        const sourcing = page.locator(
            '[id^="procurement-orders-create-row-"][id$="-sourcing-option"]',
        )
        await expect(sourcing).toHaveCount(2)
        for (let index = 0; index < 2; index += 1) {
            await chooseOption(page, sourcing.nth(index), DIRECT_OPTION, "供应商直发")
            await expect(sourcing.nth(index)).toHaveValue(new RegExp("供应商直发"))
            await expect(sourcing.nth(index)).not.toHaveValue(WAREHOUSE_OPTION)
        }
        await expect(page.locator('[id$="-warehouse"]')).toHaveCount(0)
        await expect(page.getByRole("combobox", { name: "仓库", exact: true })).toHaveCount(0)

        await page.locator("#procurement-orders-create-preview").click()
        const preview = page.getByRole("dialog", { name: "预览供给分配" })
        await expect(preview).toBeVisible({ timeout: 20000 })
        await expect(
            preview.getByText(/本次不占用现有库存|将为供给缺口创建|张采购单提交审批/),
        ).toBeVisible({
            timeout: 20000,
        })
        await expect(preview.getByText("现有库存分配")).toHaveCount(0)
        await expect(preview.getByText("供应商直发").first()).toBeVisible()
        await expect(preview.getByText("入仓")).not.toBeVisible()
        await confirmSupplyAllocation(page, /供给分配已完成|本次供给分配已保存/)
    }

    // ── 7. 采购单已提交审批：履约责任=供应商直发；不得留草稿 ──
    {
        await gotoHeading(page, "/procurement/orders", "采购单")
        const poRow = purchaseOrderRow(page, salesOrderNo)
        await expect(poRow).toBeVisible({ timeout: 20000 })
        await expect(poRow.getByText("实物 / 供应商直发", { exact: true })).toBeVisible({
            timeout: 20000,
        })
        await expect(poRow.getByText("审批中")).toBeVisible({ timeout: 20000 })
        await expect(poRow.getByText("草稿")).toHaveCount(0)
        purchaseOrderNo = (
            (await poRow.getByRole("button", { name: /打开采购单/ }).textContent()) ?? ""
        ).trim()
        expect(purchaseOrderNo.length).toBeGreaterThan(0)
    }

    // ── 8. 财务总监审批采购单生效 ──
    {
        page = await switchTo("caiwu")
        await openWorkspaceTask(page, "采购单审批", salesOrderNo, "approval")
        await expect(page.getByText("供给来源 / 履约责任")).toHaveCount(0)
        await approveCurrentDocument(page)
    }

    // ── 9. 采购单已生效；仓储仍不得入库 ──
    {
        page = await switchTo("caigou")
        await gotoHeading(page, "/procurement/orders", "采购单")
        const poRow = purchaseOrderRow(page, salesOrderNo)
        await expect(poRow.getByText("已生效")).toBeVisible({ timeout: 20000 })
        await expect(poRow.getByText("实物 / 供应商直发", { exact: true })).toBeVisible({
            timeout: 20000,
        })
        await expect(poRow.getByText(purchaseOrderNo).first()).toBeVisible()
    }
    {
        page = await switchTo("cangchu")
        await page.goto("/workspace?family=fulfillment")
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByRole("button", { name: /入库/ })).toHaveCount(0)
        await assertInventoryUntouched(page, salesOrderNo)
    }

    // ── 10. 若先款门槛拦住直发，出纳先完成付款任务 ──
    {
        page = await switchTo("caigou")
        await openWorkspaceTask(page, "履约处理", legalName, "fulfillment")
        await expect(page.getByText("供应商直发").first()).toBeVisible({ timeout: 20000 })
        await openFulfillmentWorkspaceForm(page)
        await expect(page.locator('[aria-label="供应商直发表单"]')).toBeVisible({
            timeout: 20000,
        })
        const confirmBtn = page.locator("#fulfillment-operations-work-surface-confirm")
        const blocked =
            (await page.getByText(/先款未到|暂时不能/).isVisible().catch(() => false)) ||
            !(await confirmBtn.isEnabled())

        if (blocked) {
            page = await switchTo("fukuan")
            await payOnlySupplierTask(page, purchaseOrderNo)
        }
    }

    // ── 11. 采购：登记代发（不经仓库）并确认发货 ──
    {
        page = await switchTo("caigou")
        await openWorkspaceTask(page, "履约处理", legalName, "fulfillment")
        await openFulfillmentWorkspaceForm(page)
        await expect(page.locator('[aria-label="供应商直发表单"]')).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByText("不走自有仓库，库存不变")).toBeVisible()
        await expect(page.locator('[id^="fulfillment-operations-direct-form-line-"][id$="-tracking-no"]')).toHaveCount(2)
        await fillDeliveryTrackingEntries(page, {
            kind: "direct",
            entries: [
                { salesOrderLineId: salesLineIds[0], trackingNo, carrier: "顺丰速运" },
                { salesOrderLineId: salesLineIds[0], trackingNo: secondTrackingNo, carrier: "货拉拉" },
                { salesOrderLineId: salesLineIds[0], trackingNo: sharedTrackingNo, carrier: "货拉拉" },
                { salesOrderLineId: salesLineIds[1], trackingNo: otherLineTrackingNo, carrier: "顺丰速运" },
                { salesOrderLineId: salesLineIds[1], trackingNo: sharedTrackingNo, carrier: "货拉拉" },
            ],
        })
        await expect(
            page.locator("#fulfillment-operations-work-surface-confirm"),
        ).toBeEnabled({ timeout: 20000 })
        await page.locator("#fulfillment-operations-work-surface-confirm").click()
        const confirm = page.getByRole("alertdialog", { name: "确认发货？" })
        await expect(confirm).toBeVisible({ timeout: 20000 })
        const posted = page.waitForResponse(response =>
            response.request().method() === "POST" && /\/deliveries\/[^/]+\/post$/.test(response.url()),
        )
        await confirm.locator("#fulfillment-operations-workspace-confirm-confirm").click()
        const postedResponse = await posted
        expect(postedResponse.ok(), "供应商直发必须已由后端确认后才能关闭会话").toBeTruthy()
        const delivery = (await postedResponse.json()).data
        expect(delivery.status).toBe("SHIPPED")
        const expectedTrackingEntries = [
            { sales_order_line_id: salesLineIds[0], tracking_no: trackingNo, carrier: "顺丰速运" },
            { sales_order_line_id: salesLineIds[0], tracking_no: secondTrackingNo, carrier: "货拉拉" },
            { sales_order_line_id: salesLineIds[0], tracking_no: sharedTrackingNo, carrier: "货拉拉" },
            { sales_order_line_id: salesLineIds[1], tracking_no: otherLineTrackingNo, carrier: "顺丰速运" },
            { sales_order_line_id: salesLineIds[1], tracking_no: sharedTrackingNo, carrier: "货拉拉" },
        ]
        expect(postedResponse.request().postDataJSON().tracking_entries).toEqual(expectedTrackingEntries)
        expect(delivery.tracking_entries).toEqual(expectedTrackingEntries)
        expect(delivery).not.toHaveProperty("tracking_no")
        expect(delivery).not.toHaveProperty("tracking_numbers")
        expect(delivery).not.toHaveProperty("carrier")
        await expect(confirm).toBeHidden({ timeout: 20000 })
    }

    // ── 12. 销售：客户验收通过 ──
    {
        page = await switchTo("xiaoshou")
        await openWorkspaceTask(page, "客户验收登记", salesOrderNo, "fulfillment")
        await page.locator("#sales-orders-acceptance-register-open").click()
        const dialog = page.getByRole("dialog", { name: "登记客户验收" })
        await expect(dialog).toBeVisible({ timeout: 20000 })
        const batches = dialog.locator("#acceptance-register-list")
        await dialog.getByRole("button", { name: /^明细 1，/ }).click()
        await expect(batches.getByText(trackingNo, { exact: true })).toBeVisible()
        await expect(batches.getByText(secondTrackingNo, { exact: true })).toBeVisible()
        await expect(batches.getByText(sharedTrackingNo, { exact: true })).toBeVisible()
        await expect(batches.getByText("货拉拉 ·", { exact: false }).first()).toBeVisible()
        await expect(batches.getByText(otherLineTrackingNo, { exact: true })).toHaveCount(0)
        await dialog.getByRole("button", { name: /^明细 2，/ }).click()
        await expect(batches.getByText(otherLineTrackingNo, { exact: true })).toBeVisible()
        await expect(batches.getByText(sharedTrackingNo, { exact: true })).toBeVisible()
        await expect(batches.getByText(trackingNo, { exact: true })).toHaveCount(0)
        await expect(batches.getByText(secondTrackingNo, { exact: true })).toHaveCount(0)
        await dialog.locator("#sales-orders-acceptance-comment").fill("按销售明细登记并核验包裹")
        await expect(dialog.locator("#sales-orders-acceptance-register-submit")).toBeDisabled({ timeout: 20000 })
        await expect(dialog.getByText("请上传签收单凭证（图片或 PDF）", { exact: true })).toBeVisible()
        await uploadAcceptanceEvidence(page)
        await expect(dialog.locator("#sales-orders-acceptance-register-submit")).toBeEnabled({ timeout: 20000 })
        await dialog.locator("#sales-orders-acceptance-register-submit").click()
        const confirm = page.getByRole("alertdialog", { name: "确认客户验收" })
        await expect(confirm).toBeVisible({ timeout: 20000 })
        const accepted = page.waitForResponse(
            response => response.request().method() === "POST" && response.url().endsWith("/admin/customer-acceptances/commit"),
        )
        await confirm.locator("#sales-orders-acceptance-confirm-confirm").click()
        const acceptedResponse = await accepted
        expect(acceptedResponse.ok(), await acceptedResponse.text()).toBeTruthy()
        const acceptance = (await acceptedResponse.json()).data.acceptance
        expect(acceptance.evidence_attachment_id).toBeTruthy()
        acceptanceNo = acceptance.acceptance_no
        expect(acceptanceNo).toBeTruthy()
        expect(acceptedResponse.request().postDataJSON().lines.map((line: { sales_order_line_id: string }) => line.sales_order_line_id).sort()).toEqual([...salesLineIds].sort())
        await expectToast(page, "客户验收已登记")
        // 验收提交后任务完成、任务视图关闭；下游销售单已完成断言覆盖正确性。
        await expect(page.getByText("当前筛选没有待办")).toBeVisible({ timeout: 20000 })
    }

    // ── 13. 终态断言：自有库存无变化、无采购入库单、无入仓履约 ──
    {
        page = await switchTo("cangchu")
        await assertInventoryUntouched(page, salesOrderNo)
        page = await switchTo("caigou")
        await gotoHeading(page, "/procurement/orders", "采购单")
        const poRow = purchaseOrderRow(page, salesOrderNo)
        await expect(poRow.getByText("实物 / 供应商直发", { exact: true })).toBeVisible({
            timeout: 20000,
        })
        await expect(poRow.getByText("已生效")).toBeVisible({ timeout: 20000 })
        await expect(poRow.getByText(purchaseOrderNo).first()).toBeVisible({ timeout: 20000 })
    }
    {
        page = await switchTo("cangchu")
        await page.goto("/workspace?family=fulfillment")
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByRole("button", { name: /入库/ })).toHaveCount(0)
        await assertInventoryUntouched(page, salesOrderNo)
    }
    {
        page = await switchTo("xiaoshou")
        await page.goto("/sales/orders")
        await expect(
            page.getByRole("button", { name: `查看销售单 ${salesOrderNo}` }),
        ).toBeVisible({
            timeout: 20000,
        })
        await page.getByRole("button", { name: `查看销售单 ${salesOrderNo}` }).click()
        await expect(page.getByText("已生效").first()).toBeVisible({ timeout: 20000 })
        await expect(page.getByText(/已完成|履约/).first()).toBeVisible({ timeout: 20000 })
        await page.getByRole("tab", { name: "验收", exact: true }).click()
        const evidenceDownload = page.getByRole("button", { name: "下载签收单凭证", exact: true })
        await expect(evidenceDownload).toBeVisible({ timeout: 20000 })
        const [download, evidenceResponse] = await Promise.all([
            page.waitForEvent("download"),
            page.waitForResponse(response =>
                response.request().method() === "GET" && /\/admin\/customer-acceptances\/[^/]+\/evidence$/.test(new URL(response.url()).pathname),
            ),
            evidenceDownload.click(),
        ])
        expect(await download.failure()).toBeNull()
        expect(download.suggestedFilename()).toBe(`${acceptanceNo}.pdf`)
        expect(evidenceResponse.ok(), await evidenceResponse.text()).toBeTruthy()
        expect(evidenceResponse.headers()["content-type"]).toBe("application/pdf")
        expect(evidenceResponse.headers()["cache-control"]).toBe("private, no-store")
        const downloadedPath = await download.path()
        expect(downloadedPath).toBeTruthy()
        expect(fs.readFileSync(downloadedPath!)).toEqual(contractPdf().buffer)
    }
    } finally {
        await session?.context.close()
    }
})
