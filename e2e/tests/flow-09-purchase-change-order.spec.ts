/**
 * 流程: [flow-09] 采购变更单（未执行）
 * 文档: docs/erp-phase-1.md §6.5.2；审批合同 §4.3/§4.4；工作台合同第 3 节
 * 账号: xiaoshou（销售提交）→ caigou（采购确认销售单、供给分配、发起并提交采购变更）
 *       → caiwu（财务审批采购单 / 复核采购变更）→ cangchu（仓储确认变更）
 *
 * 验收约束：
 * - 采购变更走统一 DOCUMENT_APPROVAL（仓储确认 → 财务复核），W01 原地通过/驳回
 * - 末节点通过即 on_final_approve 生效；客户端 /effect 关闭
 * - 完整目标编辑后提交；驳回后修改同一变更单，重提形成新提交并重新经过仓储与财务
 */
import { workPage } from "../helpers/workspace-page"
import { archiveContractViaUi } from "../helpers/contracts"
import { selectExistingContractForSalesOrder } from "../helpers/sales-orders"
import { openAuthorizedSupplierPaymentTask } from "../helpers/payments"
import path from "node:path"

import { test, expect, type Page } from "../helpers/test"

import { apiGet, apiToken } from "../helpers/api"
import { createCustomerViaUi } from "../helpers/customers"
import { openLoggedInWorkspace } from "../helpers/login"
import { expandSourcingEditor } from "../helpers/sourcing"
import {
    workspaceTaskRows,
    approveCurrentDocument,
    chooseOption,
    expectToast,
    openWorkspaceTask,
    pickCalendarDay,
    readHeaderDocumentNumber,
    selectWorkspaceFamily,
} from "../helpers/ui"

const SAMPLE_CONTRACT_PDF = path.join(
    process.cwd(),
    "fixtures/sample-contract.pdf",
)
const SKU_NAME = "狮峰明前龙井礼盒"
const WAREHOUSE_NAME = "北京通州仓"
const WAREHOUSE_CODE = "BJ-TZ-01"
const VISIBLE = { timeout: 20_000 } as const
const CHANGE_REJECTION = "采购成本需重新确认，请修改原采购变更单"
const CHANGE_RESUBMISSION = "已按仓储意见重新核对供应商报价"
const CHANGE_REASON = "供应商报价调整，采购单价改为 900"
const EFFECTIVE_GROSS = /^1900(?:\.0+)?$/
const EFFECTIVE_GROSS_DISPLAY = /(?<![\d,])(?:1,900\.00|1900\.00)(?![\d.])/

type ChangeDetail = {
    id: string
    purchase_order_id: string
    base_revision_id: string
    current_submission_id: string | null
    status: string
    reason: string
    approval: {
        instance: { id: string; subject_version: string | null } | null
    }
}

type PurchaseLine = {
    line_type: string
    quantity: string | null
    unit_cost_gross: string | null
    input_tax_rate: string | null
    allocated_quantity: string | null
    gross_amount: string | null
    [field: string]: unknown
}

type ChangeDraft = {
    reason: string
    payment_term_code: string
    lines: PurchaseLine[]
}
type PurchaseCenter = {
    id: string
    purchase_no: string
    current_revision_id: string | null
    revision_no: number | null
    lines: PurchaseLine[]
    totals: { gross: string; net: string; tax: string }
    payable_summary: {
        payable_open_amount: string
        paid_allocated_amount: string
    } | null
}

async function readChangeApprovalSubject(
    token: string,
    changeOrderId: string,
    instanceId: string,
) {
    // 采购对象详情的 instance 是简化摘要；正式提交版本由本人发起的审批列表投影读取。
    const page = await apiGet<{
        items: Array<{
            instance_id: string
            document_id: string
            subject_version: number
        }>
    }>(token, "/admin/approval-instances", {
        view: "started",
        document_type: "purchase_change_order",
        q: changeOrderId,
        limit: 20,
    })
    const instance = page.items.find(
        (item) =>
            item.instance_id === instanceId &&
            item.document_id === changeOrderId,
    )
    expect(instance).toBeDefined()
    return instance!.subject_version
}

function canonicalPurchaseLines(lines: PurchaseLine[]) {
    const decimal = (value: string | null) =>
        value == null
            ? null
            : value.includes(".")
              ? value.replace(/0+$/, "").replace(/\.$/, "")
              : value
    return lines.map((line) => ({
        ...line,
        quantity: decimal(line.quantity),
        unit_cost_gross: decimal(line.unit_cost_gross),
        input_tax_rate: decimal(line.input_tax_rate),
        allocated_quantity: decimal(line.allocated_quantity),
        gross_amount: decimal(line.gross_amount),
    }))
}

test.describe.configure({ mode: "serial" })
test.setTimeout(8 * 60 * 1000)

function todayIso(): string {
    const date = new Date()
    const pad = (value: number) => String(value).padStart(2, "0")
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

async function fillEmptyDatePickers(page: Page) {
    const iso = todayIso()
    const empty = workPage(page).getByRole("button", { name: "选择日期" })
    const total = await empty.count()
    for (let index = 0; index < total; index += 1) {
        const remaining = workPage(page).getByRole("button", { name: "选择日期" })
        if ((await remaining.count()) === 0) break
        await pickCalendarDay(page, remaining.first(), iso)
    }
}

function taskButtonPattern(label: string, hint: string): RegExp {
    const escaped = hint.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
    return new RegExp(`${label}[\\s\\S]*${escaped}|${escaped}[\\s\\S]*${label}`)
}

async function approveWorkspaceTask(
    page: Page,
    taskName: RegExp,
    currentNode?: string | RegExp,
    hint?: string,
) {
    await openWorkspaceTask(page, taskName, hint, "approval")
    if (currentNode) {
        await expect(workPage(page).getByText(currentNode).first()).toBeVisible(VISIBLE)
    }
    await expect(
        workPage(page).getByRole("button", { name: /^(通过|通过审批)$/ }),
    ).toBeVisible(VISIBLE)
    const decided = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().endsWith("/admin/approval-decisions"),
        { timeout: 40_000 },
    )
    await approveCurrentDocument(page)
    expect((await decided).ok()).toBeTruthy()
    const gone = hint
        ? workPage(page).getByRole("button", {
              name: taskButtonPattern(taskName.source, hint),
          })
        : workPage(page).getByRole("button", { name: taskName })
    await expect(gone).toHaveCount(0, VISIBLE)
}

test("[flow-09] 采购变更实改后驳回，修改原单重提并生效", async ({
    browser,
}) => {
    const stamp = Date.now().toString(10)
    const creditCode = `91${stamp}FLOW09XX`
        .replace(/[^0-9A-Za-z]/g, "")
        .slice(0, 18)
        .padEnd(18, "0")
    const legalName = `华润置地福利测试${stamp.slice(-8)}`
    const contractNo = `HT-FLOW09-${stamp.slice(-8)}`
    let salesOrderNo = ""
    let purchaseNo = ""

    // 1–2. 使用已初始化的采购责任规则，创建客户、合同及实物销售单。
    const sales = await openLoggedInWorkspace(browser, "xiaoshou")
    try {
        await createCustomerViaUi(sales.page, {
            legalName,
            shortName: `测试客户${stamp.slice(-6)}`,
            creditCode,
            paymentTermLabel: "货到 15 天",
            contact: { name: "李测", phone: "13800138001" },
            address: "北京市朝阳区测试路 1 号",
        })

        await sales.page.goto("/sales/contracts")
        await expect(
            workPage(sales.page).getByRole("heading", { name: "合同" }),
        ).toBeVisible(VISIBLE)
        await workPage(sales.page)
            .locator("#page-actions-action-upload")
            .click()
        await archiveContractViaUi(sales.page, { contractNo, customerName: legalName, pdf: SAMPLE_CONTRACT_PDF })

        await sales.page.goto("/sales/orders?mode=create")
        await expect(
            workPage(sales.page)
                .getByRole("heading", { name: "新建销售单" })
                .or(workPage(sales.page).getByRole("heading", { name: "业务信息" })),
        ).toBeVisible(VISIBLE)
        await selectExistingContractForSalesOrder(sales.page, contractNo)
        await expect(workPage(sales.page).locator("#sales-orders-create-customer")).toContainText(new RegExp(legalName))
        await chooseOption(
            sales.page,
            workPage(sales.page).locator("#sales-orders-create-header-welfare-scene"),
            "礼包",
        )
        await workPage(sales.page).locator("#sales-orders-create-line-items-add").click()
        const skuDialog = workPage(sales.page).getByRole("dialog", { name: "添加商品" })
        await expect(skuDialog).toBeVisible(VISIBLE)
        await skuDialog
            .locator("#sales-orders-sku-picker-filter-search")
            .fill(SKU_NAME)
        await skuDialog
            .locator("#sales-orders-sku-picker-filter-search")
            .press("Enter")
        const skuRow = skuDialog.getByRole("row", {
            name: new RegExp(SKU_NAME),
        })
        await expect(skuRow).toBeVisible(VISIBLE)
        await skuRow.getByRole("checkbox").check()
        await skuDialog.getByRole("button", { name: /加入所选/ }).click()
        await expect(skuDialog).toBeHidden(VISIBLE)
        await expect(
            workPage(sales.page).getByRole("button", {
                name: new RegExp(`更换销售项目 ${SKU_NAME}`),
            }),
        ).toBeVisible(VISIBLE)
        await workPage(sales.page)
            .locator('input[id^="sales-orders-create-line-"][id$="-quantity"]')
            .fill("2")
        // 成交单价由销售填写，参考价不自动成为成交价（第一期 §4.4.2）。
        await workPage(sales.page)
            .getByLabel("含税成交单价", { exact: true })
            .fill("1000.00")
        await workPage(sales.page)
            .locator("#sales-orders-create-batch-due-date-open")
            .click()
        await pickCalendarDay(
            sales.page,
            workPage(sales.page).locator("#sales-orders-create-batch-due-date"),
            todayIso(),
        )
        await workPage(sales.page)
            .locator("#sales-orders-create-batch-due-date-apply")
            .click()
        await expectToast(sales.page, "已批量设置交期")
        await expect(
            workPage(sales.page).locator(
                '[data-testid^="sales-line-procurement-owner-"]',
            ),
        ).not.toContainText("暂未确定采购负责人", VISIBLE)
        await workPage(sales.page).getByTestId("sales-order-submit").click()
        await expect(
            workPage(sales.page).getByRole("heading", { name: "提交销售单" }),
        ).toBeVisible(VISIBLE)
        await workPage(sales.page).locator("#sales-orders-submit-confirm-confirm").click()
        const negativeProfit = workPage(sales.page).getByRole("alertdialog", { name: "预估毛利为负" })
        await expect(negativeProfit).toBeVisible(VISIBLE)
        await negativeProfit.getByRole("button", { name: "仍然提交", exact: true }).click()
        await expect(sales.page).toHaveURL(/\/sales\/orders\/[^/?#]+/, VISIBLE)
        await expect(
            workPage(sales.page).locator("header").getByText("审批中"),
        ).toBeVisible(VISIBLE)
        salesOrderNo = await readHeaderDocumentNumber(sales.page)
        expect(salesOrderNo.length).toBeGreaterThan(0)
    } finally {
        await sales.context.close()
    }

    // 3. 采购确认销售单（采购确认节点不选供给）
    {
        const procurement = await openLoggedInWorkspace(browser, "caigou")
        try {
            await approveWorkspaceTask(
                procurement.page,
                /销售单审批/,
                "采购确认",
                salesOrderNo,
            )
        } finally {
            await procurement.context.close()
        }
    }

    // 4. 供给分配：库存为空必须生成采购单并立即提交审批
    {
        const procurement = await openLoggedInWorkspace(browser, "caigou")
        try {
            await openWorkspaceTask(
                procurement.page,
                /待安排采购/,
                salesOrderNo,
                "procurement",
            )
            await expect(
                workPage(procurement.page).getByRole("region", {
                    name: "当前供给分配任务",
                }),
            ).toBeVisible(VISIBLE)
            await expect(
                workPage(procurement.page).getByRole("region", { name: "供给商品列表" }),
            ).toBeVisible(VISIBLE)
            await workPage(procurement.page)
                .getByTestId("purchase-create-match-best")
                .click()
            await expectToast(
                procurement.page,
                /已重新分配供给|没有可匹配的供给方案/,
            )
            await expect(
                workPage(procurement.page).getByRole("region", { name: "供给商品列表" }),
            ).toBeVisible(VISIBLE)
            const editor = await expandSourcingEditor(procurement.page, SKU_NAME)
            await chooseOption(
                procurement.page,
                editor.getByRole("combobox", { name: /^履约方案，/ }),
                /杭州狮峰.*集采入仓/,
                "集采入仓",
            )
            await chooseOption(
                procurement.page,
                editor.getByRole("combobox", { name: "仓库", exact: true }),
                new RegExp(`${WAREHOUSE_CODE}|${WAREHOUSE_NAME}`),
                WAREHOUSE_CODE,
            )
            await fillEmptyDatePickers(procurement.page)
            await editor.locator("#sourcing-product-adjust-done").click()
            await workPage(procurement.page)
                .getByTestId("purchase-create-preview")
                .click()
            await expect(
                workPage(procurement.page).getByRole("heading", { name: "预览供给分配" }),
            ).toBeVisible(VISIBLE)
            await expect(
                workPage(procurement.page).getByText(
                    /本次不占用现有库存|将为供给缺口创建|张采购单提交审批/,
                ),
            ).toBeVisible(VISIBLE)
            await expect(
                workPage(procurement.page).getByText("无需创建采购单"),
            ).toHaveCount(0)
            const committed = procurement.page.waitForResponse((response) =>
                response.request().method() === "POST" &&
                new URL(response.url()).pathname === "/admin/purchase-orders/from-sourcing",
            )
            await workPage(procurement.page).locator("#procurement-orders-create-preview-confirm").click()
            const allocation = await committed
            const result = await allocation.json()
            expect(allocation.ok(), JSON.stringify(result)).toBe(true)
            expect(result).toMatchObject({ success: true, data: { stock_reservations: [], work_item_status: "COMPLETED" } })
            expect(result.data.orders).toHaveLength(1)
            await procurement.page.goto("/procurement/orders")
            await expect(
                workPage(procurement.page).getByRole("heading", {
                    name: "采购单",
                    exact: true,
                }),
            ).toBeVisible(VISIBLE)
            const createdSearch = workPage(procurement.page).locator(
                "#procurement-orders-list-search",
            )
            await createdSearch.fill(salesOrderNo)
            await createdSearch.press("Enter")
            const createdRow = workPage(procurement.page)
                .locator("#procurement-orders-list-table")
                .getByRole("row")
                .filter({ hasText: salesOrderNo })
            await expect(createdRow).toHaveCount(1, VISIBLE)
            purchaseNo = (
                (await createdRow
                    .getByRole("button", { name: /打开采购单/ })
                    .textContent()) ?? ""
            ).trim()
            expect(purchaseNo.length).toBeGreaterThan(0)
        } finally {
            await procurement.context.close()
        }
    }

    // 5. 采购部门审批采购单 → 采购单生效、形成应付；本流程不付款、不入库
    {
        const finance = await openLoggedInWorkspace(browser, "caigoushenpi")
        try {
            await approveWorkspaceTask(
                finance.page,
                /采购单审批/,
                "采购部门审批",
                purchaseNo,
            )
        } finally {
            await finance.context.close()
        }
    }

    const procurement = await openLoggedInWorkspace(browser, "caigou")
    let purchaseHref = ""
    let changeOrderId = ""
    let firstSubmission: ChangeDetail
    let firstTarget: {
        payment_term_code: string
        reason?: string
        lines: PurchaseLine[]
    }
    let original: PurchaseCenter
    let purchaseOrderId = ""
    let effectiveGross = ""
    const procurementToken = await apiToken("caigou")
    try {
        await procurement.page.goto("/procurement/orders")
        await expect(
            workPage(procurement.page).getByRole("heading", {
                name: "采购单",
                exact: true,
            }),
        ).toBeVisible(VISIBLE)
        const poSearch = workPage(procurement.page).locator(
            "#procurement-orders-list-search",
        )
        await poSearch.fill(purchaseNo)
        await poSearch.press("Enter")
        const openPo = workPage(procurement.page).getByRole("button", {
            name: new RegExp(
                `^打开采购单 ${purchaseNo.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`,
            ),
        })
        await expect(openPo).toBeVisible(VISIBLE)
        await openPo.click()
        await expect(procurement.page).toHaveURL(
            /\/procurement\/orders\/[^/?#]+/,
            VISIBLE,
        )
        purchaseHref = procurement.page.url()
        purchaseOrderId = purchaseHref.match(
            /\/procurement\/orders\/([^/?#]+)/,
        )![1]
        original = await apiGet<PurchaseCenter>(
            procurementToken,
            `/admin/purchase-orders/${purchaseOrderId}`,
        )
        expect(original.revision_no).toBe(1)
        await expect(
            workPage(procurement.page)
                .locator("header")
                .getByText("已生效", { exact: true }),
        ).toBeVisible(VISIBLE)
        await expect(
            workPage(procurement.page)
                .locator('[aria-label="采购单摘要"]')
                .getByText("未付"),
        ).toBeVisible(VISIBLE)
        await expect(
            workPage(procurement.page)
                .locator('[aria-label="采购单摘要"]')
                .getByText("未开始"),
        ).toBeVisible(VISIBLE)

        // 负向：未执行前不得把履约/付款当成本流程
        await expect(
            workPage(procurement.page).getByRole("button", {
                name: /确认入库|确认发货|确认入账/,
            }),
        ).toHaveCount(0)

        // 6. 发起改单（未入库未付款，走变更单而非纠正单）：直接打开改单弹窗，与销售改单一致。
        const startedResponse = procurement.page.waitForResponse(
            (response) =>
                response.request().method() === "POST" &&
                response
                    .url()
                    .endsWith(
                        `/admin/purchase-orders/${purchaseOrderId}/changes`,
                    ),
        )
        await workPage(procurement.page)
            .locator("#procurement-orders-detail-change")
            .click()
        const started = (await (await startedResponse).json()).data as {
            change_id: string
        }
        changeOrderId = started.change_id
        expect(changeOrderId).toBeTruthy()

        // 7. 编辑完整目标：保留销售分配和数量，实际将采购单价改为 900 并填写原因后提交。
        const edit = workPage(procurement.page).getByRole("dialog", {
            name: "采购改单",
            exact: true,
        })
        await expect(edit).toBeVisible(VISIBLE)
        await edit
            .locator('input[id^="purchase-change-edit-"][id$="-unit-cost"]')
            .fill("900")
        await edit
            .locator('textarea[id^="purchase-change-edit-"][id$="-reason"]')
            .fill(CHANGE_REASON)
        const submittedResponse = procurement.page.waitForResponse(
            (response) =>
                response.request().method() === "POST" &&
                response
                    .url()
                    .endsWith(
                        `/admin/purchase-change-orders/${changeOrderId}/submit`,
                    ),
        )
        await edit
            .getByRole("button", { name: "提交改单", exact: true })
            .click()
        const submitted = await submittedResponse
        expect(submitted.ok()).toBeTruthy()
        firstTarget = submitted.request().postDataJSON() as {
            payment_term_code: string
            reason?: string
            lines: PurchaseLine[]
        }
        expect(firstTarget.reason).toBe(CHANGE_REASON)
        await expect(edit).toBeHidden(VISIBLE)
        await expect(procurement.page).toHaveURL(/section=changes/, VISIBLE)
        // 审批中的改单只能撤回，不得再开第二张改单。
        await expect(
            workPage(procurement.page).locator("#procurement-orders-detail-change"),
        ).toHaveCount(0)
        firstSubmission = await apiGet<ChangeDetail>(
            procurementToken,
            `/admin/purchase-change-orders/${changeOrderId}`,
        )
        expect(firstSubmission.status).toBe("IN_APPROVAL")
        expect(firstSubmission.current_submission_id).toBeTruthy()
        expect(
            await readChangeApprovalSubject(
                procurementToken,
                changeOrderId,
                firstSubmission.approval.instance!.id,
            ),
        ).toBe(1)
        expect(
            firstTarget.lines.find((line) => line.line_type === "ITEM_SERVICE")
                ?.unit_cost_gross,
        ).toBe("900")
        // 提交未生效，原采购版本与金额继续有效。
        const stillOriginal = await apiGet<PurchaseCenter>(
            procurementToken,
            `/admin/purchase-orders/${purchaseOrderId}`,
        )
        expect(stillOriginal.current_revision_id).toBe(
            original.current_revision_id,
        )
        expect(stillOriginal.lines).toEqual(original.lines)
        expect(stillOriginal.totals).toEqual(original.totals)
        await expect(
            workPage(procurement.page)
                .locator("header")
                .getByText("已生效", { exact: true }),
        ).toBeVisible(VISIBLE)
    } finally {
        await procurement.context.close()
    }

    // 8. 仓储终态驳回；采购编辑并重提原变更单，驳回历史不得被覆盖。
    {
        const warehouse = await openLoggedInWorkspace(browser, "cangchu")
        try {
            await openWorkspaceTask(
                warehouse.page,
                /采购变更单审批/,
                changeOrderId,
                "approval",
            )
            await workPage(warehouse.page)
                .getByRole("button", { name: "驳回", exact: true })
                .click()
            const reject = workPage(warehouse.page).getByRole("dialog", {
                name: "确认驳回",
                exact: true,
            })
            await expect(reject).toBeVisible(VISIBLE)
            await reject.getByLabel("驳回原因").fill(CHANGE_REJECTION)
            await reject
                .getByRole("button", { name: "确认驳回", exact: true })
                .click()
            await expect(reject).toBeHidden(VISIBLE)
        } finally {
            await warehouse.context.close()
        }
    }
    {
        const procurement = await openLoggedInWorkspace(browser, "caigou")
        try {
            await procurement.page.goto(purchaseHref)
            await workPage(procurement.page)
                .getByRole("tab", { name: "变更", exact: true })
                .click()
            const pendingChange = workPage(procurement.page).getByRole("region", {
                name: "进行中的改单",
            })
            await expect(pendingChange.getByText(CHANGE_REJECTION, { exact: true })).toBeVisible(VISIBLE)
            await pendingChange.getByRole("button", { name: "编辑改单", exact: true }).click()
            const edit = workPage(procurement.page).getByRole("dialog", {
                name: "采购改单",
                exact: true,
            })
            await expect(edit).toBeVisible(VISIBLE)
            const reopened = await apiGet<ChangeDraft>(
                procurementToken,
                `/admin/purchase-change-orders/${changeOrderId}/draft`,
            )
            expect(canonicalPurchaseLines(reopened.lines)).toEqual(
                canonicalPurchaseLines(firstTarget!.lines),
            )
            expect(reopened.payment_term_code).toBe(
                firstTarget!.payment_term_code,
            )
            const draft = await apiGet<ChangeDetail>(
                procurementToken,
                `/admin/purchase-change-orders/${changeOrderId}`,
            )
            expect(draft.id).toBe(changeOrderId)
            expect(draft.status).toBe("REJECTED")
            expect(draft.current_submission_id).toBe(
                firstSubmission!.current_submission_id,
            )
            const oldInstanceId = firstSubmission!.approval.instance!.id
            const rejected = await apiGet<{ status: string }>(
                procurementToken,
                `/admin/approval-instances/${oldInstanceId}`,
            )
            expect(rejected.status).toBe("REJECTED")
            const oldHistory = await apiGet<{
                items: Array<{ result: string; decision_reason: string | null }>
            }>(
                procurementToken,
                `/admin/approval-instances/${oldInstanceId}/history`,
            )
            expect(
                oldHistory.items.some(
                    (item) =>
                        item.result === "REJECTED" &&
                        item.decision_reason === CHANGE_REJECTION,
                ),
            ).toBeTruthy()
            expect(
                oldHistory.items.some((item) => item.result === "CANCELLED"),
            ).toBe(false)
            await expect(
                edit.locator(
                    'input[id^="purchase-change-edit-"][id$="-unit-cost"]',
                ),
            ).toHaveValue(/^900(?:\.0+)?$/)
            await edit
                .locator('input[id^="purchase-change-edit-"][id$="-unit-cost"]')
                .fill("950")
            await expect(edit.getByLabel("变更原因")).toHaveValue(CHANGE_REASON)
            const submit = edit.getByRole("button", { name: "提交改单", exact: true })
            await expect(submit).toBeDisabled()
            await edit.getByLabel("重新提交说明").fill(CHANGE_RESUBMISSION)
            const [resubmittedHttp] = await Promise.all([
                procurement.page.waitForResponse(
                    (response) =>
                        response.request().method() === "POST" &&
                        new URL(response.url()).pathname ===
                            `/admin/purchase-change-orders/${changeOrderId}/submit`,
                ),
                submit.click(),
            ])
            expect(resubmittedHttp.request().postDataJSON().resubmission_reason).toBe(CHANGE_RESUBMISSION)
            const resubmittedBody = await resubmittedHttp.text()
            expect(resubmittedHttp.ok(), resubmittedBody).toBe(true)
            expect(JSON.parse(resubmittedBody).success, resubmittedBody).toBe(
                true,
            )
            await expect(edit).toBeHidden(VISIBLE)
            const resubmitted = await apiGet<ChangeDetail>(
                procurementToken,
                `/admin/purchase-change-orders/${changeOrderId}`,
            )
            expect(resubmitted.id).toBe(changeOrderId)
            expect(resubmitted.purchase_order_id).toBe(purchaseOrderId)
            expect(resubmitted.base_revision_id).toBe(
                firstSubmission!.base_revision_id,
            )
            expect(resubmitted.current_submission_id).not.toBe(
                firstSubmission!.current_submission_id,
            )
            expect(resubmitted.approval.instance?.id).not.toBe(oldInstanceId)
            expect(
                await readChangeApprovalSubject(
                    procurementToken,
                    changeOrderId,
                    resubmitted.approval.instance!.id,
                ),
            ).toBe(2)
            const unchanged = await apiGet<PurchaseCenter>(
                procurementToken,
                `/admin/purchase-orders/${purchaseOrderId}`,
            )
            expect(unchanged.purchase_no).toBe(purchaseNo)
            expect(unchanged.current_revision_id).toBe(
                original!.current_revision_id,
            )
            expect(unchanged.lines).toEqual(original!.lines)
            expect(unchanged.totals).toEqual(original!.totals)
        } finally {
            await procurement.context.close()
        }
    }
    {
        const warehouse = await openLoggedInWorkspace(browser, "cangchu")
        try {
            await approveWorkspaceTask(
                warehouse.page,
                /采购变更单审批/,
                "仓储确认库存发货影响",
                changeOrderId,
            )
        } finally {
            await warehouse.context.close()
        }
    }

    // 9. 财务复核金额与应付；末节点通过即生效
    {
        const finance = await openLoggedInWorkspace(browser, "caiwu")
        try {
            await approveWorkspaceTask(
                finance.page,
                /采购变更单审批/,
                "财务复核金额与应付",
                changeOrderId,
            )
        } finally {
            await finance.context.close()
        }
    }

    // 10. 断言：变更已生效，采购单/应付按变更更新；仍未付款未履约
    {
        const procurement = await openLoggedInWorkspace(browser, "caigou")
        try {
            await procurement.page.goto(purchaseHref)
            const effective = await apiGet<PurchaseCenter>(
                procurementToken,
                `/admin/purchase-orders/${purchaseOrderId}`,
            )
            expect(effective.id).toBe(purchaseOrderId)
            expect(effective.purchase_no).toBe(purchaseNo)
            expect(effective.revision_no).toBe(2)
            expect(effective.current_revision_id).not.toBe(
                original!.current_revision_id,
            )
            expect(effective.totals.gross).not.toBe(original!.totals.gross)
            expect(effective.totals.gross).toMatch(EFFECTIVE_GROSS)
            effectiveGross = effective.totals.gross
            expect(
                effective.lines.find(
                    (line) => line.line_type === "ITEM_SERVICE",
                )?.unit_cost_gross,
            ).toMatch(/^950(?:\.0+)?$/)
            expect(effective.payable_summary?.payable_open_amount).toBe(
                effective.totals.gross,
            )
            await expect(
                workPage(procurement.page)
                    .locator("header")
                    .getByText("已生效", { exact: true }),
            ).toBeVisible(VISIBLE)
            await expect(
                workPage(procurement.page)
                    .locator("header")
                    .getByText("已生效", { exact: true }),
            ).toBeVisible(VISIBLE)
            await workPage(procurement.page).getByRole("tab", { name: "变更" }).click()
            await expect(
                workPage(procurement.page)
                    .getByRole("listitem")
                    .filter({ hasText: CHANGE_REASON })
                    .getByText("已生效", { exact: true }),
            ).toBeVisible(VISIBLE)
            await expect(
                workPage(procurement.page).getByRole("button", {
                    name: "编辑改单",
                    exact: true,
                }),
            ).toHaveCount(0)
            await workPage(procurement.page).getByRole("tab", { name: "票款" }).click()
            await expect(workPage(procurement.page).getByText("应付未结")).toBeVisible(
                VISIBLE,
            )
            await expect(
                workPage(procurement.page).getByText("尚未形成应付（需审批通过）。"),
            ).toHaveCount(0)
            // 侧栏金额和票款摘要都会写「已付并核销」。摘要这一处才是本单票款。
            await expect(
                workPage(procurement.page)
                    .getByLabel("采购票款摘要")
                    .getByText(/已付并核销/),
            ).toBeVisible(VISIBLE)
            await workPage(procurement.page).getByRole("tab", { name: "概览" }).click()
            await expect(workPage(procurement.page).getByText("未付")).toBeVisible(
                VISIBLE,
            )
            await expect(workPage(procurement.page).getByText("未开始")).toBeVisible(
                VISIBLE,
            )
            await expect(
                workPage(procurement.page).getByRole("button", {
                    name: /确认入库|确认发货/,
                }),
            ).toHaveCount(0)
        } finally {
            await procurement.context.close()
        }
    }

    // 11. 出纳从同一采购单的付款任务读取变更后的待付金额，不执行付款。
    {
        const cashier = await openLoggedInWorkspace(browser, "fukuan")
        try {
            const paymentRequest = await openAuthorizedSupplierPaymentTask(cashier.page, purchaseNo)
            expect(paymentRequest.lines).toHaveLength(1)
            expect(paymentRequest.lines[0]?.source_snapshot?.purchase_preview.purchase_no).toBe(purchaseNo)
            expect(paymentRequest.total_amount).toBe(effectiveGross)
            expect(paymentRequest.lines[0]?.requested_amount).toBe(effectiveGross)
            const paymentTask = workPage(cashier.page).getByLabel("当前付款任务")
            await expect(paymentTask).toBeVisible(VISIBLE)
            const paymentSummary = workPage(cashier.page).getByRole("region", { name: "付款任务摘要", exact: true })
            await expect(
                paymentSummary.getByText("本申请剩余待付", { exact: true }).locator(".."),
            ).toContainText(EFFECTIVE_GROSS_DISPLAY, VISIBLE)
            await cashier.page.goto(`/procurement/orders/${purchaseOrderId}`)
            await expect(cashier.page).toHaveURL(
                new RegExp(`/procurement/orders/${purchaseOrderId}(?:\\?|$)`),
                VISIBLE,
            )
            const summary = workPage(cashier.page).getByLabel("采购单摘要")
            await expect(
                summary.getByText("应付未结", { exact: true }).locator(".."),
            ).toContainText(EFFECTIVE_GROSS_DISPLAY, VISIBLE)
            const cashierToken = await apiToken("fukuan")
            const current = await apiGet<PurchaseCenter>(
                cashierToken,
                `/admin/purchase-orders/${purchaseOrderId}`,
            )
            expect(current.id).toBe(purchaseOrderId)
            expect(current.purchase_no).toBe(purchaseNo)
            expect(current.totals.gross).toBe(effectiveGross)
            expect(current.payable_summary?.payable_open_amount).toBe(
                effectiveGross,
            )
            expect(current.payable_summary?.paid_allocated_amount).toMatch(
                /^0(?:\.0+)?$/,
            )
        } finally {
            await cashier.context.close()
        }
    }

    // 仓储工作台：未执行本流程不得把履约任务当变更完成
    {
        const warehouse = await openLoggedInWorkspace(browser, "cangchu")
        try {
            await warehouse.page.goto("/workspace/tasks")
            await expect(
                workPage(warehouse.page).getByRole("heading", { name: "待我处理" }),
            ).toBeVisible(VISIBLE)
            await selectWorkspaceFamily(warehouse.page, "approval")
            await expect(
                workspaceTaskRows(warehouse.page).and(workPage(warehouse.page).getByRole("row", { name: /采购变更单审批/ })),
            ).toHaveCount(0)
        } finally {
            await warehouse.context.close()
        }
    }
})
