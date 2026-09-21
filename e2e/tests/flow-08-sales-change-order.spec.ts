/**
 * 流程: [flow-08] 销售变更单（未履约）
 * 文档: docs/erp-phase-1.md §6.5.1 + §4.4（生效后才允许变更单）
 * 账号: xiaoshou（提交销售单/变更单）、caigou（销售单采购确认 + 变更单履约影响）、caiwu（变更单财务复核）
 *
 * 文档-代码差异（以代码为准）：
 * 1. 文档要求「调整数量/金额并填原因」；UI 发起改单只克隆当前生效版本，原因写死为「销售发起变更」，
 *    无变更工作副本编辑面（PUT /sales-orders/{id}/working-copy 只服务 FirstSubmission）。
 * 2. 文档时序在财务复核后由销售「生效」；代码在末节点通过时 apply_effective_change 自动生成新版本。
 * 3. 文档「采购确认履约影响 / 财务复核」已收敛为 SalesChangeOrder 的 DOCUMENT_APPROVAL 两节点；
 *    退役类型 SALES_CHANGE_IMPACT_REVIEW / SALES_CHANGE_FINANCE_REVIEW 不再进入 W01。
 * 4. 驳回不改业务状态：销售单保持「审批中」，仍禁止发起改单。
 * 5. 改单提交确认框展示创建时冻结的审批路线，
 *    路线正确性由后继两节点审批实际流转覆盖验证。
 */
import fs from "node:fs"
import os from "node:os"
import path from "node:path"
import { fileURLToPath } from "node:url"

import { expect, test, type BrowserContext, type Page } from "@playwright/test"

import { createCustomerViaUi } from "../helpers/customers"
import { openLoggedInWorkspace } from "../helpers/login"
import {
    approveCurrentDocument,
    chooseOption,
    dismissToasts,
    expectToast,
    openWorkspaceTask,
    pickCalendarDay,
} from "../helpers/ui"

const TIMEOUT = 20_000
const SKU_KEYWORD = "龙井"
const LINE_QTY = "2"
const GROSS_RE = /2,576\.00|2576\.00/

function contractPdfPath() {
    const here = path.dirname(fileURLToPath(import.meta.url))
    const candidates = [
        path.join(process.cwd(), "fixtures", "sample-contract.pdf"),
        path.join(here, "..", "fixtures", "sample-contract.pdf"),
    ]
    for (const candidate of candidates) {
        if (fs.existsSync(candidate)) return candidate
    }
    const fallback = path.join(os.tmpdir(), "erp-flow-08-sample-contract.pdf")
    fs.writeFileSync(
        fallback,
        "%PDF-1.4\n1 0 obj<</Type/Catalog>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n",
    )
    return fallback
}

function uniqueCreditCode() {
    return `91110108MA${Date.now().toString().slice(-7)}X`
}

async function gotoNav(page: Page, name: string, hrefId: string) {
    const link = page.getByRole("link", { name })
    if (await link.count()) {
        await link.click()
        return
    }
    await page.locator(`#${hrefId}`).click()
}

async function rejectOpenTask(page: Page, reason: string) {
    await page.getByRole("button", { name: "驳回" }).click()
    const dialog = page.getByRole("dialog", { name: "确认驳回" })
    await expect(dialog).toBeVisible({ timeout: TIMEOUT })
    await dialog.getByLabel("驳回原因").fill(reason)
    await dialog.getByRole("button", { name: "确认驳回" }).click()
    await expect(dialog).toHaveCount(0, { timeout: TIMEOUT })
}

function plusDaysIso(days: number): string {
    const date = new Date()
    date.setDate(date.getDate() + days)
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

async function createPhysicalSalesOrder(page: Page, customerName: string, contractNo: string) {
    await gotoNav(page, "销售单", "workspace-sidebar-nav-sales-orders")
    await expect(page.getByRole("heading", { name: "销售单", exact: true })).toBeVisible({
        timeout: TIMEOUT,
    })
    await page.locator("#sales-orders-list-header-create").click()
    await expect(
        page
            .getByRole("heading", { name: "新建销售单" })
            .or(page.getByRole("heading", { name: "业务信息" })),
    ).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(page.locator("#sales-orders-create-contract")).toBeVisible({
        timeout: TIMEOUT,
    })

    await page.getByRole("button", { name: "上传合同 PDF", exact: true }).click()
    const upload = page.getByRole("dialog", { name: "上传合同 PDF" })
    await expect(upload).toBeVisible({ timeout: TIMEOUT })
    await upload.locator("#card-contracts-upload-pdf-input").setInputFiles(contractPdfPath())
    await upload.getByLabel("合同编号").fill(contractNo)
    await chooseOption(page, upload.locator("#card-contracts-upload-customer"), customerName)
    await expect
        .poll(async () => upload.locator("#card-contracts-upload-settlement-party").inputValue(), {
            timeout: TIMEOUT,
        })
        .not.toEqual("")
    await chooseOption(page, upload.locator("#card-contracts-upload-payment-terms"), "货到 30 天")
    await upload.locator("#card-contracts-upload-submit").click()
    await expect(upload).toHaveCount(0, { timeout: TIMEOUT })
    await expect(page.getByText(customerName).first()).toBeVisible({ timeout: TIMEOUT })

    await chooseOption(page, page.locator("#sales-orders-create-header-welfare-scene"), "年节礼包")
    await chooseOption(page, page.locator("#sales-orders-create-header-payment-terms"), "货到 30 天")

    await page.locator("#sales-orders-create-line-items-add").click()
    const skuDialog = page.getByRole("dialog", { name: "添加商品" })
    await expect(skuDialog).toBeVisible({ timeout: TIMEOUT })
    await skuDialog
        .getByPlaceholder("搜索 SKU、商品名称、编号或规格")
        .fill(SKU_KEYWORD)
    await skuDialog.getByPlaceholder("搜索 SKU、商品名称、编号或规格").press("Enter")
    const skuRow = skuDialog.getByRole("checkbox", { name: new RegExp(SKU_KEYWORD) }).first()
    await expect(skuRow).toBeVisible({ timeout: TIMEOUT })
    await skuRow.check()
    await skuDialog.getByRole("button", { name: /加入所选（/ }).click()
    await expect(skuDialog).toHaveCount(0, { timeout: TIMEOUT })
    await expect(page.getByText(new RegExp(SKU_KEYWORD)).first()).toBeVisible({
        timeout: TIMEOUT,
    })

    await page.getByLabel("数量").fill(LINE_QTY)
    await page.locator("#sales-orders-create-batch-due-date-open").click()
    await pickCalendarDay(page, page.locator("#sales-orders-create-batch-due-date"), plusDaysIso(21))
    await page.locator("#sales-orders-create-batch-due-date-apply").click()
    await expectToast(page, "已批量设置交期")

    await expect(
        page.getByText("暂未确定采购负责人，请联系管理员维护采购责任规则"),
    ).toHaveCount(0, { timeout: TIMEOUT })

    await page.locator("#sales-orders-create-submit").click()
    const submitDialog = page.getByRole("dialog", { name: "提交销售单" })
    await expect(submitDialog).toBeVisible({ timeout: TIMEOUT })
    await dismissToasts(page)
    await submitDialog.locator("#sales-orders-submit-confirm-confirm").click({ force: true })
    await page.waitForURL(/\/sales\/orders\/(?!.*mode=create)[^/?]+/, {
        timeout: TIMEOUT,
    })
    await expect(page.getByText("审批中", { exact: true }).first()).toBeVisible({
        timeout: TIMEOUT,
    })
    const match = page.url().match(/\/sales\/orders\/([^/?]+)/)
    if (!match?.[1]) throw new Error("未能从地址栏读取销售单 ID")
    return match[1]
}

async function openSalesOrder(page: Page, salesOrderId: string) {
    await page.goto(`/sales/orders/${salesOrderId}`)
    await expect(page.locator("#sales-orders-detail-start-change")).toBeVisible({
        timeout: TIMEOUT,
    })
}

async function expectChangeBlocked(page: Page) {
    await expect(page.locator("#sales-orders-detail-start-change")).toBeDisabled({
        timeout: TIMEOUT,
    })
    await expect(page.locator("#sales-orders-create-submit")).toHaveCount(0)
}

test.describe("flow-08 销售变更单（未履约）", () => {
    test.setTimeout(240_000)

    test("审批中/驳回不得改单；生效且未履约后变更审批生效", async ({ browser }) => {
        const stamp = Date.now().toString().slice(-8)
        const customerName = `流08福利客户${stamp}`
        const contractNo = `HT-E2E-08-${stamp}`
        const sessions: BrowserContext[] = []

        const sales = await openLoggedInWorkspace(browser, "xiaoshou")
        const procurement = await openLoggedInWorkspace(browser, "caigou")
        const finance = await openLoggedInWorkspace(browser, "caiwu")
        sessions.push(sales.context, procurement.context, finance.context)

        try {
            // 1. 客户 + 合同 + 实物销售单提交（未履约、未出入库）
            await createCustomerViaUi(sales.page, {
                legalName: customerName,
                shortName: `流08客户${stamp}`,
                creditCode: uniqueCreditCode(),
                paymentTermLabel: "货到 15 天",
                contact: { name: "李测", phone: "13800138001" },
                address: "北京市朝阳区测试路 1 号",
            })
            const salesOrderId = await createPhysicalSalesOrder(
                sales.page,
                customerName,
                contractNo,
            )
            await expect(sales.page.getByText(GROSS_RE).first()).toBeVisible({
                timeout: TIMEOUT,
            })
            await expectChangeBlocked(sales.page)

            // 2. 负向：审批中不得发起改单
            await openWorkspaceTask(procurement.page, /销售单审批/, undefined, "approval")
            await expect(procurement.page.getByText("采购确认").first()).toBeVisible({
                timeout: TIMEOUT,
            })
            await rejectOpenTask(procurement.page, "交期无法承诺，先驳回验证不得开变更单")

            await openSalesOrder(sales.page, salesOrderId)
            await expect(sales.page.getByText("审批中", { exact: true }).first()).toBeVisible({
                timeout: TIMEOUT,
            })
            await sales.page.getByRole("tab", { name: /审批/ }).click()
            await expect(sales.page.getByText("最近驳回").first()).toBeVisible({
                timeout: TIMEOUT,
            })
            await expectChangeBlocked(sales.page)

            // 3. 采购再通过，销售单生效；不得出现已建采购单/已履约
            await openWorkspaceTask(procurement.page, /销售单审批/, undefined, "approval")
            await approveCurrentDocument(procurement.page)

            await openSalesOrder(sales.page, salesOrderId)
            await expect(sales.page.getByText("已生效", { exact: true }).first()).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(sales.page.getByText(/版本\s*v1/)).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(sales.page.getByText("未开始", { exact: true }).first()).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(sales.page.locator("#sales-orders-detail-start-change")).toBeEnabled()
            await expect(sales.page.locator("#sales-orders-create-submit")).toHaveCount(0)

            await sales.page.getByRole("tab", { name: /采购/ }).click()
            await expect(sales.page.getByTestId("sales-order-purchase-status")).toContainText(
                "待采购",
                { timeout: TIMEOUT },
            )

            // 4. 发起改单草稿并提交审批（代码无改量编辑面，工作副本即当前版本）
            await sales.page.locator("#sales-orders-detail-start-change").click()
            const startDialog = sales.page.getByRole("alertdialog", { name: "发起改单" })
            await expect(startDialog).toBeVisible({ timeout: TIMEOUT })
            await startDialog.locator("#sales-orders-detail-change-confirm").click()
            await expect(sales.page.getByText("改单已创建")).toBeVisible({ timeout: TIMEOUT })
            await expect(startDialog).toHaveCount(0, { timeout: TIMEOUT })
            await expectChangeBlocked(sales.page)
            await expect(sales.page.getByRole("tab", { name: /版本/ })).toContainText("改单中", {
                timeout: TIMEOUT,
            })

            await sales.page.getByRole("tab", { name: /版本/ }).click()
            await expect(sales.page.getByRole("button", { name: "提交改单" })).toBeVisible({
                timeout: TIMEOUT,
            })
            await sales.page.locator("#sales-orders-change-submit").click()
            const changeSubmit = sales.page.getByRole("alertdialog", { name: /提交改单/ })
            await expect(changeSubmit).toBeVisible({ timeout: TIMEOUT })
            await expect(changeSubmit.getByText("采购确认履约影响 → 财务复核金额与应收", { exact: true })).toBeVisible()
            await changeSubmit.locator("#sales-orders-change-submit-confirm-confirm").click()
            await expect(sales.page.getByText("改单已提交审批")).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(sales.page.getByText("已生效", { exact: true }).first()).toBeVisible()
            // 改单状态渲染为"状态：审批中（基于 vN）…"，无独立"审批中"徽标。
            await expect(sales.page.getByText(/状态：审批中/).first()).toBeVisible({
                timeout: TIMEOUT,
            })

            // 5. 采购确认履约影响 → 财务复核金额与应收 → 自动生效
            await openWorkspaceTask(procurement.page, /销售变更单审批/, undefined, "approval")
            await expect(
                procurement.page.getByText("采购确认履约影响").first(),
            ).toBeVisible({ timeout: TIMEOUT })
            await approveCurrentDocument(procurement.page)
            await expect(
                procurement.page.getByRole("button", { name: /销售变更单审批/ }),
            ).toHaveCount(0, { timeout: TIMEOUT })

            await openWorkspaceTask(finance.page, /销售变更单审批/, undefined, "approval")
            await expect(
                finance.page.getByText("财务复核金额与应收").first(),
            ).toBeVisible({ timeout: TIMEOUT })
            await approveCurrentDocument(finance.page)
            await expect(
                finance.page.getByRole("button", { name: /销售变更单审批/ }),
            ).toHaveCount(0, { timeout: TIMEOUT })

            // 6. 销售单金额/明细/应收按变更版本更新（本实现克隆原量，版本号递增）
            await openSalesOrder(sales.page, salesOrderId)
            await expect(sales.page.getByText("已生效", { exact: true }).first()).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(sales.page.getByText(/版本\s*v2/)).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(sales.page.getByText(GROSS_RE).first()).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(sales.page.getByText("未开始", { exact: true }).first()).toBeVisible()
            await expect(sales.page.locator("#sales-orders-detail-start-change")).toBeEnabled()

            await sales.page.getByRole("tab", { name: /版本/ }).click()
            await expect(sales.page.getByText("v2", { exact: true }).first()).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(sales.page.getByText("当前在用")).toBeVisible()
            await expect(sales.page.getByText("销售变更单").first()).toBeVisible()
            await expect(sales.page.getByRole("button", { name: "提交改单" })).toHaveCount(0)

            await sales.page.getByRole("tab", { name: /采购/ }).click()
            await expect(sales.page.getByTestId("sales-order-purchase-status")).toContainText(
                "待采购",
            )

            await sales.page.getByRole("tab", { name: /票款/ }).click()
            await expect(sales.page.getByText(/待收|未结|未收/).first()).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(sales.page.getByText(GROSS_RE).first()).toBeVisible()
        } finally {
            await Promise.all(sessions.map((context) => context.close()))
        }
    })
})
