/**
 * 流程: [flow-08] 销售变更单（未履约）
 * 文档: docs/erp-phase-1.md §6.5.1 + §4.4（生效后才允许变更单）
 * 账号: xiaoshou（提交销售单/变更单）、caigou（销售单采购确认 + 变更单履约影响）、caiwu（变更单财务复核）
 *
 * 验收约束：销售变更必须实际修改完整目标后提交；驳回后以「修改原单」撤回，
 * 原变更单 ID 与基准版本不变，重提形成新提交与新审批实例，历史提交及原 v1 不被改写。
 * 末节点通过自动生成 v2；未生效的变更不改变销售金额、应收和采购状态。
 */
import { archiveContractViaUi } from "../helpers/contracts"
import fs from "node:fs"
import os from "node:os"
import path from "node:path"
import { fileURLToPath } from "node:url"

import { expect, test, type BrowserContext, type Page } from "../helpers/test"

import { apiGet, apiToken } from "../helpers/api"
import { createCustomerViaUi } from "../helpers/customers"
import { openLoggedInWorkspace } from "../helpers/login"
import {
    approveCurrentDocument,
    chooseOption,
    dismissToasts,
    expectToast,
    openWorkspaceTask,
    pickCalendarDay,
    readHeaderDocumentNumber,
} from "../helpers/ui"

const TIMEOUT = 20_000
const SKU_NO = "TEA-SF-LJ-250"
const SKU_NAME = "狮峰明前龙井礼盒"
const UNIT_PRICE_RE = /^(?:1,288|1288)(?:\.0+)?$/
const LINE_QTY = "2"
const GROSS_RE = /2,576\.00|2576\.00/
const CHANGED_GROSS_RE = /5,200\.00|5200\.00/
const CHANGE_REJECTION = "目标数量超出客户确认范围，请修改原销售变更单"

type ChangeDetail = {
    id: string
    sales_order_id: string
    base_revision_id: string
    current_submission_id: string | null
    target_content_hash: string | null
    status: string
    reason: string
    approval: {
        instance: { id: string; subject_version: string | null } | null
    }
}

type ChangeDraft = {
    reason: string
    business_remark: string | null
    content_hash: string
    lines: Array<{
        sales_order_line_id: string
        goods: { quantity: string; unit_price_gross: string } | null
    }>
}

type SalesCenter = {
    id: string
    order_no: string
    current_revision_id: string | null
    revisions: Array<{
        id: string
        revision_no: number
        gross_amount: string
        [field: string]: unknown
    }>
}

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

async function createPhysicalSalesOrder(
    page: Page,
    customerName: string,
    contractNo: string,
) {
    await gotoNav(page, "销售单", "workspace-sidebar-nav-sales-orders")
    await expect(
        page.getByRole("heading", { name: "销售单", exact: true }),
    ).toBeVisible({
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

    await page
        .locator("#sales-orders-create-contract-upload")
        .click()
    await archiveContractViaUi(page, { contractNo, customerName, pdf: contractPdfPath(), paymentTerms: "货到 30 天" })
    await expect(page.locator("#sales-orders-create-customer")).toHaveValue(new RegExp(customerName))

    await chooseOption(
        page,
        page.locator("#sales-orders-create-header-welfare-scene"),
        "年节礼包",
    )
    await expect(page.locator("#sales-orders-create-header-payment-terms")).toHaveValue("货到 30 天")
    await expect(page.locator("#sales-orders-create-header-payment-terms")).toBeDisabled()

    await page.locator("#sales-orders-create-line-items-add").click()
    const skuDialog = page.getByRole("dialog", { name: "添加商品" })
    await expect(skuDialog).toBeVisible({ timeout: TIMEOUT })
    await skuDialog
        .getByPlaceholder("搜索 SKU、商品名称、编号或规格")
        .fill(SKU_NO)
    await skuDialog
        .getByPlaceholder("搜索 SKU、商品名称、编号或规格")
        .press("Enter")
    const skuRow = skuDialog.getByRole("row").filter({
        has: page.getByText(SKU_NO, { exact: true }),
    })
    await expect(skuRow).toHaveCount(1, { timeout: TIMEOUT })
    await expect(skuRow).toBeVisible({ timeout: TIMEOUT })
    await expect(skuRow).toContainText(SKU_NAME)
    await expect(skuRow).toContainText(/1,288\.00|1288\.00/)
    await skuRow.getByRole("checkbox").check()
    await skuDialog.getByRole("button", { name: /加入所选（/ }).click()
    await expect(skuDialog).toHaveCount(0, { timeout: TIMEOUT })
    const selectedLine = page.getByRole("row").filter({
        has: page.getByRole("button", {
            name: new RegExp(`^更换销售项目 ${SKU_NAME}`),
        }),
    })
    await expect(selectedLine).toHaveCount(1, { timeout: TIMEOUT })
    await expect(
        selectedLine.locator(
            'input[id^="sales-orders-create-line-"][id$="-unit-price"]',
        ),
    ).toHaveValue(UNIT_PRICE_RE)

    await page.getByLabel("数量").fill(LINE_QTY)
    await page.locator("#sales-orders-create-batch-due-date-open").click()
    await pickCalendarDay(
        page,
        page.locator("#sales-orders-create-batch-due-date"),
        plusDaysIso(21),
    )
    await page.locator("#sales-orders-create-batch-due-date-apply").click()
    await expectToast(page, "已批量设置交期")

    await expect(
        page.getByText("暂未确定采购负责人，请联系管理员维护采购责任规则"),
    ).toHaveCount(0, {
        timeout: TIMEOUT,
    })

    await page.locator("#sales-orders-create-submit").click()
    const submitDialog = page.getByRole("dialog", { name: "提交销售单" })
    await expect(submitDialog).toBeVisible({ timeout: TIMEOUT })
    await dismissToasts(page)
    await submitDialog
        .locator("#sales-orders-submit-confirm-confirm")
        .click({ force: true })
    await page.waitForURL(/\/sales\/orders\/(?!.*mode=create)[^/?]+/, {
        timeout: TIMEOUT,
    })
    await expect(page.getByText("审批中", { exact: true }).first()).toBeVisible(
        {
            timeout: TIMEOUT,
        },
    )
    const match = page.url().match(/\/sales\/orders\/([^/?]+)/)
    if (!match?.[1]) throw new Error("未能从地址栏读取销售单 ID")
    return match[1]
}

async function openSalesOrder(page: Page, salesOrderId: string) {
    await page.goto(`/sales/orders/${salesOrderId}`)
    await expect(page.locator("#sales-orders-detail-start-change")).toBeVisible(
        {
            timeout: TIMEOUT,
        },
    )
}

async function expectChangeBlocked(page: Page) {
    await expect(
        page.locator("#sales-orders-detail-start-change"),
    ).toBeDisabled({
        timeout: TIMEOUT,
    })
    await expect(page.locator("#sales-orders-create-submit")).toHaveCount(0)
}

test.describe("flow-08 销售变更单（未履约）", () => {
    test.setTimeout(8 * 60 * 1000)

    test("销售变更实改后驳回，修改原单重提并生成新正式版本", async ({
        browser,
    }) => {
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
            await openWorkspaceTask(
                procurement.page,
                /销售单审批/,
                customerName,
                "approval",
            )
            await expect(
                procurement.page.getByText("采购确认").first(),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await rejectOpenTask(
                procurement.page,
                "交期无法承诺，先驳回验证不得开变更单",
            )

            await openSalesOrder(sales.page, salesOrderId)
            await expect(
                sales.page.getByText("审批中", { exact: true }).first(),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await sales.page.getByRole("tab", { name: /审批/ }).click()
            await expect(sales.page.getByText("最近驳回").first()).toBeVisible({
                timeout: TIMEOUT,
            })
            await expectChangeBlocked(sales.page)

            // 3. 采购再通过，销售单生效；不得出现已建采购单/已履约
            await openWorkspaceTask(
                procurement.page,
                /销售单审批/,
                customerName,
                "approval",
            )
            await approveCurrentDocument(procurement.page)

            await openSalesOrder(sales.page, salesOrderId)
            await expect(
                sales.page.getByText("已生效", { exact: true }).first(),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(sales.page.getByText(/版本\s*v1/)).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(
                sales.page.getByText("未开始", { exact: true }).first(),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(
                sales.page.locator("#sales-orders-detail-start-change"),
            ).toBeEnabled()
            await expect(
                sales.page.locator("#sales-orders-create-submit"),
            ).toHaveCount(0)

            await sales.page.getByRole("tab", { name: /采购/ }).click()
            await expect(
                sales.page.getByTestId("sales-order-purchase-status"),
            ).toContainText("待采购", {
                timeout: TIMEOUT,
            })

            const salesToken = await apiToken("xiaoshou")
            const orderNo = await readHeaderDocumentNumber(sales.page)
            const original = await apiGet<SalesCenter>(
                salesToken,
                `/admin/sales-orders/${salesOrderId}`,
            )
            const originalRevision = original.revisions.find(
                (revision) => revision.revision_no === 1,
            )
            expect(originalRevision).toBeDefined()

            // 4. 发起变更并编辑完整目标；原销售单 v1 在末节点通过前仍有效。
            const createdResponse = sales.page.waitForResponse(
                (response) =>
                    response.request().method() === "POST" &&
                    response.url().endsWith("/admin/sales-change-orders"),
            )
            await sales.page
                .locator("#sales-orders-detail-start-change")
                .click()
            const startDialog = sales.page.getByRole("alertdialog", {
                name: "发起改单",
            })
            await expect(startDialog).toBeVisible({ timeout: TIMEOUT })
            await startDialog
                .locator("#sales-orders-detail-change-confirm")
                .click()
            await expect(sales.page.getByText("改单已创建")).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(startDialog).toHaveCount(0, { timeout: TIMEOUT })
            await expectChangeBlocked(sales.page)
            await expect(
                sales.page.getByRole("tab", { name: /版本/ }),
            ).toContainText("改单中", {
                timeout: TIMEOUT,
            })

            await sales.page.getByRole("tab", { name: /版本/ }).click()
            const created = (await (await createdResponse).json()).data as {
                id: string
            }
            expect(created.id).toBeTruthy()
            const changeOrderId = created.id
            await sales.page
                .locator("#sales-orders-change-edit-original")
                .click()
            const edit = sales.page.getByRole("dialog", {
                name: "修改销售变更原单",
            })
            await expect(edit).toBeVisible({ timeout: TIMEOUT })
            await edit
                .locator('textarea[id^="sales-change-edit-"][id$="-reason"]')
                .fill("客户追加福利数量，调整销售成交单价")
            await edit
                .locator('input[id^="sales-change-edit-"][id$="-quantity"]')
                .fill("3")
            await edit
                .locator('input[id^="sales-change-edit-"][id$="-price"]')
                .fill("1300")
            const savedResponse = sales.page.waitForResponse(
                (response) =>
                    response.request().method() === "POST" &&
                    response
                        .url()
                        .endsWith(
                            `/admin/sales-change-orders/${changeOrderId}/draft`,
                        ),
            )
            const submittedResponse = sales.page.waitForResponse(
                (response) =>
                    response.request().method() === "POST" &&
                    new URL(response.url()).pathname ===
                        `/admin/sales-change-orders/${changeOrderId}/submit-impact`,
            )
            await edit
                .getByRole("button", { name: "保存并提交审批", exact: true })
                .click()
            const firstTarget = (await (await savedResponse).json())
                .data as ChangeDraft
            const submittedHttp = await submittedResponse
            const submittedBody = await submittedHttp.text()
            expect(submittedHttp.ok(), submittedBody).toBe(true)
            expect(JSON.parse(submittedBody).success, submittedBody).toBe(true)
            await expect(edit).toBeHidden({ timeout: TIMEOUT })
            const firstSubmission = await apiGet<ChangeDetail>(
                salesToken,
                `/admin/sales-change-orders/${changeOrderId}`,
            )
            expect(firstSubmission.current_submission_id).toBeTruthy()
            expect(firstSubmission.approval.instance?.subject_version).toBe("1")
            expect(firstSubmission.status).toBe("IN_APPROVAL")
            expect(firstTarget.lines[0].goods?.quantity).toMatch(/^3(?:\.0+)?$/)
            expect(firstTarget.lines[0].goods?.unit_price_gross).toMatch(
                /^1300(?:\.0+)?$/,
            )
            await expect(sales.page.getByText(GROSS_RE).first()).toBeVisible({
                timeout: TIMEOUT,
            })

            // 5. 首次目标经采购通过、财务驳回；修改原单后必须重新经过首节点。
            await openWorkspaceTask(
                procurement.page,
                /销售变更单审批/,
                orderNo,
                "approval",
            )
            await approveCurrentDocument(procurement.page)
            await openWorkspaceTask(
                finance.page,
                /销售变更单审批/,
                orderNo,
                "approval",
            )
            await rejectOpenTask(finance.page, CHANGE_REJECTION)
            await openSalesOrder(sales.page, salesOrderId)
            await sales.page.getByRole("tab", { name: /版本/ }).click()
            await sales.page
                .getByRole("button", { name: "修改原单", exact: true })
                .click()
            const revise = sales.page.getByRole("dialog", {
                name: "修改原单",
                exact: true,
            })
            await expect(revise).toBeVisible({ timeout: TIMEOUT })
            await revise
                .locator('textarea[id$="-cancel-dialog-reason"]')
                .fill("按财务驳回意见修改原销售变更单并重提")
            await revise
                .getByRole("button", { name: "撤回并修改原单", exact: true })
                .click()
            await expect(revise).toBeHidden({ timeout: TIMEOUT })
            await expect(edit).toBeVisible({ timeout: TIMEOUT })
            const reopened = await apiGet<ChangeDraft>(
                salesToken,
                `/admin/sales-change-orders/${changeOrderId}/draft`,
            )
            expect(reopened.lines).toEqual(firstTarget.lines)
            expect(reopened.reason).toBe(firstTarget.reason)
            const draftDetail = await apiGet<ChangeDetail>(
                salesToken,
                `/admin/sales-change-orders/${changeOrderId}`,
            )
            expect(draftDetail.id).toBe(changeOrderId)
            expect(draftDetail.status).toBe("DRAFT")
            expect(draftDetail.current_submission_id).toBe(
                firstSubmission.current_submission_id,
            )
            expect(draftDetail.target_content_hash).toBe(
                firstSubmission.target_content_hash,
            )
            const oldInstanceId = firstSubmission.approval.instance!.id
            const cancelled = await apiGet<{ status: string }>(
                salesToken,
                `/admin/approval-instances/${oldInstanceId}`,
            )
            expect(cancelled.status).toBe("CANCELLED")
            const oldHistory = await apiGet<{
                items: Array<{ result: string; decision_reason: string | null }>
            }>(salesToken, `/admin/approval-instances/${oldInstanceId}/history`)
            expect(
                oldHistory.items.some(
                    (item) =>
                        item.result === "REJECTED" &&
                        item.decision_reason === CHANGE_REJECTION,
                ),
            ).toBeTruthy()
            expect(
                oldHistory.items.some((item) => item.result === "CANCELLED"),
            ).toBeTruthy()
            await edit
                .locator('textarea[id^="sales-change-edit-"][id$="-reason"]')
                .fill("按财务驳回意见重新确认客户福利数量")
            await edit
                .locator('input[id^="sales-change-edit-"][id$="-quantity"]')
                .fill("4")
            await expect(
                edit.locator('input[id^="sales-change-edit-"][id$="-price"]'),
            ).toHaveValue(/^1300(?:\.0+)?$/)
            const resubmittedResponse = sales.page.waitForResponse(
                (response) =>
                    response.request().method() === "POST" &&
                    new URL(response.url()).pathname ===
                        `/admin/sales-change-orders/${changeOrderId}/submit-impact`,
            )
            await edit
                .getByRole("button", { name: "保存并提交审批", exact: true })
                .click()
            const resubmittedHttp = await resubmittedResponse
            const resubmittedBody = await resubmittedHttp.text()
            expect(resubmittedHttp.ok(), resubmittedBody).toBe(true)
            expect(JSON.parse(resubmittedBody).success, resubmittedBody).toBe(
                true,
            )
            await expect(edit).toBeHidden({ timeout: TIMEOUT })
            const resubmitted = await apiGet<ChangeDetail>(
                salesToken,
                `/admin/sales-change-orders/${changeOrderId}`,
            )
            expect(resubmitted.id).toBe(changeOrderId)
            expect(resubmitted.sales_order_id).toBe(salesOrderId)
            expect(resubmitted.base_revision_id).toBe(
                firstSubmission.base_revision_id,
            )
            expect(resubmitted.current_submission_id).not.toBe(
                firstSubmission.current_submission_id,
            )
            expect(resubmitted.target_content_hash).not.toBe(
                firstSubmission.target_content_hash,
            )
            expect(resubmitted.approval.instance?.id).not.toBe(oldInstanceId)
            expect(resubmitted.approval.instance?.subject_version).toBe("2")
            const beforeEffect = await apiGet<SalesCenter>(
                salesToken,
                `/admin/sales-orders/${salesOrderId}`,
            )
            expect(beforeEffect.order_no).toBe(orderNo)
            expect(beforeEffect.current_revision_id).toBe(
                original.current_revision_id,
            )
            expect(
                beforeEffect.revisions.find(
                    (revision) => revision.revision_no === 1,
                ),
            ).toEqual(originalRevision)

            await openWorkspaceTask(
                procurement.page,
                /销售变更单审批/,
                orderNo,
                "approval",
            )
            await expect(
                procurement.page.getByText("采购确认履约影响").first(),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await approveCurrentDocument(procurement.page)
            await openWorkspaceTask(
                finance.page,
                /销售变更单审批/,
                orderNo,
                "approval",
            )
            await expect(
                finance.page.getByText("财务复核金额与应收").first(),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await approveCurrentDocument(finance.page)

            // 6. v2 采用重提目标（4 × 1300）；v1 完整历史继续保留。
            await openSalesOrder(sales.page, salesOrderId)
            await expect(
                sales.page.getByText("已生效", { exact: true }).first(),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(sales.page.getByText(/版本\s*v2/)).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(
                sales.page.getByText(CHANGED_GROSS_RE).first(),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(
                sales.page.getByText("未开始", { exact: true }).first(),
            ).toBeVisible()
            await expect(
                sales.page.locator("#sales-orders-detail-start-change"),
            ).toBeEnabled()

            await sales.page.getByRole("tab", { name: /版本/ }).click()
            await expect(
                sales.page.getByText("v2", { exact: true }).first(),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(sales.page.getByText("当前在用")).toBeVisible()
            const effective = await apiGet<SalesCenter>(
                salesToken,
                `/admin/sales-orders/${salesOrderId}`,
            )
            expect(effective.id).toBe(salesOrderId)
            expect(effective.order_no).toBe(orderNo)
            expect(effective.current_revision_id).not.toBe(
                original.current_revision_id,
            )
            expect(
                effective.revisions.find(
                    (revision) => revision.revision_no === 1,
                ),
            ).toEqual(originalRevision)
            expect(
                effective.revisions.find(
                    (revision) => revision.revision_no === 2,
                )?.gross_amount,
            ).toMatch(/^5200(?:\.0+)?$/)
            await expect(
                sales.page.getByText("销售变更单").first(),
            ).toBeVisible()
            await expect(
                sales.page.getByRole("button", { name: "提交改单" }),
            ).toHaveCount(0)

            await sales.page.getByRole("tab", { name: /采购/ }).click()
            await expect(
                sales.page.getByTestId("sales-order-purchase-status"),
            ).toContainText("待采购")

            await sales.page.getByRole("tab", { name: /票款/ }).click()
            await expect(
                sales.page.getByText(/待收|未结|未收/).first(),
            ).toBeVisible({
                timeout: TIMEOUT,
            })
            await expect(
                sales.page.getByText(CHANGED_GROSS_RE).first(),
            ).toBeVisible()
        } finally {
            await Promise.all(sessions.map((context) => context.close()))
        }
    })
})
