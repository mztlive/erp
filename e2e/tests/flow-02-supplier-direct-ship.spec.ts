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

import { test, expect, type Page } from "@playwright/test"
import fs from "node:fs"
import path from "node:path"

import { createCustomerViaUi } from "../helpers/customers"
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

async function gotoHeading(page: Page, pathName: string, heading: string | RegExp) {
    await page.goto(pathName)
    await expect(page.getByRole("heading", { name: heading })).toBeVisible({
        timeout: 20000,
    })
}

async function assertInventoryUntouched(page: Page) {
    await gotoHeading(page, "/inventory", "库存台账")
    await expect(page.getByRole("button", { name: /^余额(?: \d+)?$/ })).toBeVisible({
        timeout: 20000,
    })
    await expect(
        page.getByText(/当前仓库尚无 ERP 自有库存记录|尚未建立库存台账|没有符合条件的库存/),
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
    const shortName = `代发${stamp.slice(-6)}`
    const contractNo = `HT-DS-${stamp}`
    const dueDate = isoPlusDays(90)
    const trackingNo = `SF${stamp.slice(-10)}`
    let salesOrderNo = ""
    let purchaseOrderNo = ""
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
        const submit = dialog.locator("#card-contracts-upload-submit")
        await expect(submit).toBeEnabled({ timeout: 20000 })
        const uploaded = page.waitForResponse(
            (response) =>
                response.request().method() === "POST" &&
                response.url().includes("/admin/contracts/upload"),
            { timeout: 60_000 },
        )
        await submit.click()
        const uploadResponse = await uploaded
        expect(uploadResponse.ok(), await uploadResponse.text()).toBeTruthy()
        await expect(dialog).toBeHidden({ timeout: 20_000 })
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
        await expect(page.getByText(legalName).first()).toBeVisible({ timeout: 20000 })
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
        await expect(
            page.locator('[data-testid^="sales-line-procurement-owner-"]'),
        ).not.toContainText("暂未确定", { timeout: 20000 })

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
    }

    // ── 4. 负向：生效前不得建采购单、不得履约、采购确认不得选源 ──
    {
        page = await switchTo("caigou")
        await gotoHeading(page, "/procurement/orders", "采购单")
        await expect(page.getByText(/0 条|当前没有/).first()).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByText("供应商直发")).toHaveCount(0)

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
        await expandSourcingEditor(page)

        const sourcing = page.locator(
            '[id^="procurement-orders-create-row-"][id$="-sourcing-option"]',
        )
        await expect(sourcing).toBeVisible({ timeout: 20000 })
        await chooseOption(page, sourcing, DIRECT_OPTION, "供应商直发")
        await expect(sourcing).toHaveValue(new RegExp("供应商直发"))
        await expect(sourcing).not.toHaveValue(WAREHOUSE_OPTION)
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
        await expect(preview.getByText("供应商直发")).toBeVisible()
        await expect(preview.getByText("入仓")).not.toBeVisible()
        await confirmSupplyAllocation(page, /供给分配已完成|本次供给分配已保存/)
    }

    // ── 7. 采购单已提交审批：履约责任=供应商直发；不得留草稿 ──
    {
        await gotoHeading(page, "/procurement/orders", "采购单")
        const poTable = page.locator("#procurement-orders-list-table")
        await expect(page.getByText("1 条").first()).toBeVisible({ timeout: 20000 })
        await expect(poTable.getByText("实物 / 供应商直发", { exact: true })).toBeVisible({ timeout: 20000 })
        await expect(poTable.getByText("草稿")).not.toBeVisible()
        await expect(poTable.getByText("审批中")).toBeVisible({ timeout: 20000 })
        purchaseOrderNo = (
            (await poTable.getByRole("button", { name: /打开采购单/ }).textContent()) ?? ""
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
        const poTable = page.locator("#procurement-orders-list-table")
        await expect(poTable.getByText("已生效")).toBeVisible({ timeout: 20000 })
        await expect(poTable.getByText("实物 / 供应商直发", { exact: true })).toBeVisible({ timeout: 20000 })
        await expect(poTable.getByText(purchaseOrderNo).first()).toBeVisible()
    }
    {
        page = await switchTo("cangchu")
        await page.goto("/workspace?family=fulfillment")
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByRole("button", { name: /入库/ })).toHaveCount(0)
        await assertInventoryUntouched(page)
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
            await payOnlySupplierTask(page)
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
        const posted = page.waitForResponse(response =>
            response.request().method() === "POST" && /\/deliveries\/[^/]+\/post$/.test(response.url()),
        )
        await confirm.locator("#fulfillment-operations-workspace-confirm-confirm").click()
        const postedResponse = await posted
        expect(postedResponse.ok(), "供应商直发必须已由后端确认后才能关闭会话").toBeTruthy()
        expect((await postedResponse.json()).data.status).toBe("SHIPPED")
        await expect(confirm).toBeHidden({ timeout: 20000 })
    }

    // ── 12. 销售：客户验收通过 ──
    {
        page = await switchTo("xiaoshou")
        await openWorkspaceTask(page, "客户验收登记", salesOrderNo, "fulfillment")
        await page.locator("#sales-orders-acceptance-register-open").click()
        const dialog = page.getByRole("dialog", { name: "登记客户验收" })
        await expect(dialog).toBeVisible({ timeout: 20000 })
        await dialog.locator("#sales-orders-acceptance-register-submit").click()
        const confirm = page.getByRole("alertdialog", { name: "确认客户验收" })
        await expect(confirm).toBeVisible({ timeout: 20000 })
        await confirm.locator("#sales-orders-acceptance-confirm-confirm").click()
        await expectToast(page, "客户验收已登记")
        // 验收提交后任务完成、任务视图关闭；下游销售单已完成断言覆盖正确性。
        await expect(page.getByText("当前筛选没有待办")).toBeVisible({ timeout: 20000 })
    }

    // ── 13. 终态断言：自有库存无变化、无采购入库单、无入仓履约 ──
    {
        page = await switchTo("caigou")
        await assertInventoryUntouched(page)
        await gotoHeading(page, "/procurement/orders", "采购单")
        const poTable = page.locator("#procurement-orders-list-table")
        await expect(poTable.getByText("实物 / 供应商直发", { exact: true })).toBeVisible({ timeout: 20000 })
        await expect(poTable.getByText("已生效")).toBeVisible({ timeout: 20000 })
        await expect(poTable.getByText(purchaseOrderNo).first()).toBeVisible({ timeout: 20000 })
    }
    {
        page = await switchTo("cangchu")
        await page.goto("/workspace?family=fulfillment")
        await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
            timeout: 20000,
        })
        await expect(page.getByRole("button", { name: /入库/ })).toHaveCount(0)
        await assertInventoryUntouched(page)
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
    }
    } finally {
        await session?.context.close()
    }
})
