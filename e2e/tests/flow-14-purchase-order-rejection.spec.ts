/**
 * 流程: [flow-14] 采购单审批驳回
 * 账号: admin（补采购责任默认调度人）→ xiaoshou（客户/合同/销售单）
 *       → caigou（销售单采购确认、供给分配、原采购单改价重提）→ caigoushenpi（采购单驳回及新提交审批）
 *       → cangchu / fukuan 仅负向断言（驳回未生效前不得履约、不得付款）
 * 最新业务验收：采购明细显示客户真实成交价；财务从采购单和审批读取关联销售版本、
 * 合同；采购销售审批读取冻结合同。临时收窄普通销售、合同、文件权限后仍须可操作
 * 精确上下文，预览 Blob 和浏览器下载均须与真实合同 PDF 原字节相同。
 *
 * 供给分配确认时创建采购单并提交；终态驳回保留冻结提交，关闭本次审批。
 * 原提交人可直接编辑重提，不产生撤回记录；生效后才形成应付。
 */
import { workPage } from "../helpers/workspace-page"
import { archiveContractViaUi } from "../helpers/contracts"
import { expectSalesOrderContractSummary } from "../helpers/sales-orders"
import fs from "node:fs"
import path from "node:path"

import {
    test,
    expect,
    type BrowserContext,
    type Locator,
    type Page,
} from "../helpers/test"

import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { createCustomerViaUi } from "../helpers/customers"
import { openLoggedInWorkspace } from "../helpers/login"
import {
    ensureDefaultProcurementOwner,
    submitCreatedSalesOrder,
} from "../helpers/procurement"
import {
    expectApprovalMaterialPreviewAndDownload,
    expectGenericSalesMaterialReadsDenied,
    expectPurchaseSalesContext,
    openFrozenApprovalMaterials,
    restrictSalesMaterialReader,
    uploadUnrelatedSalesMaterial,
} from "../helpers/purchase-sales-context"
import { expandSourcingEditor } from "../helpers/sourcing"
import {
    approveCurrentDocument,
    chooseOption,
    expectSourcingCounts,
    expectToast,
    openWorkspaceTask,
    pickCalendarDay,
    readHeaderDocumentNumber,
    selectWorkspaceFamily,
    workspaceTaskId,
} from "../helpers/ui"

const VISIBLE = { timeout: 20_000 } as const
const FLOW_TIMEOUT = 12 * 60 * 1000
const SKU_KEYWORD = "龙井"
const SKU_NAME = "狮峰明前龙井礼盒"
const WAREHOUSE_CODE = "BJ-TZ-01"
const SALES_QTY = "2"
const SALES_UNIT_PRICE = "137.00"
const REJECT_REASON = "供应商报价超预算，本轮采购单不通过"

const CONTRACT_PDF = path.resolve(process.cwd(), "fixtures/sample-contract.pdf")
const MINIMAL_PDF = Buffer.from(
    "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000052 00000 n \n0000000101 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n178\n%%EOF\n",
)

type LoginName =
    | "xiaoshou"
    | "caigou"
    | "cangchu"
    | "caigoushenpi"
    | "fukuan"
    | "admin"

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
        line_id?: string
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
    } | null
    changes?: Array<{ change_id?: string; status?: string }>
}

type SalesPurchaseSummary = {
    contract_id: string | null
    purchase_order_count: number
    purchase_coverage: { remaining_quantity: string }
    purchase_creation_access: { allowed: boolean; task_count: number }
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

async function closeSession(session: Session | undefined): Promise<void> {
    if (!session) return
    await session.context.close()
}

function contractFile():
    | string
    | { name: string; mimeType: string; buffer: Buffer } {
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

function helperToken(login: LoginName): Promise<string> {
    return apiToken(login)
}

async function helperGet<T>(token: string, apiPath: string): Promise<T> {
    return apiGet<T>(token, apiPath)
}

async function fetchSalesPurchaseSummary(id: string): Promise<SalesPurchaseSummary> {
    return apiGet<SalesPurchaseSummary>(await helperToken("xiaoshou"), `/admin/sales-orders/${id}`)
}

async function expectCoveredByOnePurchase(id: string): Promise<void> {
    const salesOrder = await fetchSalesPurchaseSummary(id)
    expect(salesOrder.purchase_order_count).toBe(1)
    expect(Number(salesOrder.purchase_coverage.remaining_quantity)).toBe(0)
}

async function fetchPurchaseCenter(
    token: string,
    purchaseOrderId: string,
): Promise<PurchaseCenter> {
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
        | {
              items?: Array<{
                  id: string
                  purchase_no?: string
                  status?: string
              }>
          }
        | Array<{ id: string; purchase_no?: string; status?: string }>
    >(
        token,
        `/admin/purchase-orders?sales_order_id=${encodeURIComponent(salesOrderId)}&page=1&page_size=20`,
    )
    if (Array.isArray(raw)) return raw
    return raw.items ?? []
}

async function fillEmptyDatePickers(
    page: Page,
    isoDate: string,
): Promise<void> {
    const empty = workPage(page).getByRole("button", { name: "选择日期" })
    const total = await empty.count()
    for (let index = 0; index < total; index += 1) {
        const remaining = workPage(page).getByRole("button", { name: "选择日期" })
        if ((await remaining.count()) === 0) break
        await pickCalendarDay(page, remaining.first(), isoDate)
    }
}

function documentHeader(page: Page): Locator {
    return workPage(page).locator("header")
}

async function readDocumentNumber(page: Page): Promise<string> {
    return readHeaderDocumentNumber(page)
}

function documentTaskPattern(label: string, hints: readonly string[]): RegExp {
    const parts = hints
        .filter((hint) => hint.length > 0)
        .map((hint) => {
            const escaped = hint.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
            return `${label}[\\s\\S]*${escaped}|${escaped}[\\s\\S]*${label}`
        })
    if (parts.length === 0) throw new Error(`缺少${label}的单据提示`)
    return new RegExp(parts.join("|"))
}

async function expectNoQueueTask(page: Page, label: RegExp): Promise<void> {
    const queue = workPage(page).getByRole("region", { name: "任务队列", exact: true })
    const rows = queue.locator('[id^="workspace-task-table-row-"]')
    const empty = queue.getByText(/没有符合条件的事项|当前没有待处理事项/)
    await expect(rows.first().or(empty).first()).toBeVisible(VISIBLE)
    await expect(queue.getByRole("row", { name: label })).toHaveCount(0)
}

function approvalPane(page: Page): Locator {
    return workPage(page).getByRole("region", { name: "当前任务", exact: true })
}

async function rejectCurrentDocument(
    page: Page,
    reason: string,
): Promise<void> {
    const pane = approvalPane(page)
    await expect(
        pane.getByRole("button", { name: "驳回", exact: true }),
    ).toBeVisible(VISIBLE)
    await pane.getByRole("button", { name: "驳回", exact: true }).click()
    const dialog = workPage(page).getByRole("dialog", { name: "确认驳回" })
    await expect(dialog).toBeVisible(VISIBLE)
    await expect(dialog.getByText(/驳回后结束本次审批并关闭审批待办/).first()).toBeVisible()
    await dialog.getByLabel("驳回原因").fill(reason)
    const [response] = await Promise.all([
        page.waitForResponse((reply) =>
            reply.request().method() === "POST" && new URL(reply.url()).pathname === "/admin/approval-decisions",
        ),
        dialog.getByRole("button", { name: "确认驳回" }).click(),
    ])
    const body = await response.json() as { success: boolean; errorMessage?: string }
    expect(response.ok() && body.success, `驳回失败（HTTP ${response.status()}）：${body.errorMessage ?? ""}`).toBe(true)
    await expect(dialog).toBeHidden(VISIBLE)
}

async function snapshotFromCenter(
    center: PurchaseCenter,
): Promise<PurchaseSnapshot> {
    const line =
        center.lines?.find((item) => item.product_name?.includes(SKU_NAME)) ??
        center.lines?.[0]
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
    expect(live.status).toBe("REJECTED")
    expect(live.content_source).toBe("DRAFT")
    expect(live.current_submission_id).toBeTruthy()
    expect(live.current_submission_id).not.toBe(snap.submissionId)
    expect(String(live.approval?.instance?.id ?? "")).toBe(snap.instanceId)
    expect(live.revision_no ?? null).toBeNull()
    expect(live.current_revision_id ?? null).toBeNull()
    expect(live.payable_summary ?? null).toBeNull()
    expect(live.changes ?? []).toEqual([])
    const line =
        live.lines?.find((item) => item.product_name?.includes(SKU_NAME)) ??
        live.lines?.[0]
    expect(String(line?.quantity ?? "")).toBe(snap.quantity)
    expect(String(line?.unit_cost_gross ?? "")).toBe(snap.unitCostGross)
    expect(String(live.totals?.gross ?? "")).toBe(snap.gross)
    expect(String(live.totals?.net ?? "")).toBe(snap.net)
    expect(String(live.totals?.tax ?? "")).toBe(snap.tax)
}

test("[flow-14] 采购单审批驳回后修改原单重提，同单号生效并形成应付", async ({
    browser,
}, testInfo) => {
    test.setTimeout(FLOW_TIMEOUT)
    const stamp = Date.now().toString(36).toUpperCase()
    const customerName = `E2E采购驳回客户${stamp}`
    const contractNo = `HT-E2E-PO-REJ-${stamp}`
    const dueDate = plusDaysIso(21)
    let salesOrderId = ""
    let salesOrderNo = ""
    let salesRevisionId = ""
    let contractId = ""
    let contractAssetId = ""
    let purchaseHref = ""
    let snap: PurchaseSnapshot | undefined
    let session: Session | undefined
    const contractUpload = contractFile()
    const contractBytes =
        typeof contractUpload === "string"
            ? fs.readFileSync(contractUpload)
            : contractUpload.buffer

    const switchTo = async (login: LoginName) => {
        await closeSession(session)
        session = await openLoggedInWorkspace(browser, login)
        return session.page
    }

    try {
        // 0) 销售提交实物单前必须能解析采购负责人
        let page = await switchTo("admin")
        await ensureDefaultProcurementOwner(page)

        // 1) 客户 + 合同 PDF + 实物销售单提交（付款条件用货到，避免本流程走先款履约）
        page = await switchTo("xiaoshou")
        await createCustomerViaUi(page, {
            legalName: customerName,
            shortName: `驳回${stamp.slice(-6)}`,
            creditCode: uniqueCreditCode(stamp),
            paymentTermLabel: "货到 15 天",
            contact: { name: "李测", phone: "13800138001" },
            address: "北京市朝阳区测试路 1 号",
        })

        await page.goto("/sales/orders?mode=create")
        await expect(
            workPage(page)
                .getByRole("heading", { name: "新建销售单" })
                .or(workPage(page).getByRole("heading", { name: "业务信息" })),
        ).toBeVisible(VISIBLE)
        await archiveContractViaUi(page, {
            contractNo,
            customerName,
            pdf: contractUpload,
        })
        await expect(
            workPage(page).getByText(`合同编号：${contractNo} · v1`, { exact: true }).first(),
        ).toBeVisible(VISIBLE)
        await expect(workPage(page).locator("#sales-orders-create-customer")).toContainText(
            new RegExp(customerName),
        )

        await chooseOption(page, workPage(page).getByLabel("福利场景"), "礼包")
        await expectSalesOrderContractSummary(page, { paymentTerms: "货到 15 天" })
        await expect(workPage(page).getByLabel("供应商")).toHaveCount(0)
        await expect(workPage(page).getByLabel("履约责任")).toHaveCount(0)
        await expect(workPage(page).getByLabel("采购成本")).toHaveCount(0)

        await workPage(page).locator("#sales-orders-create-line-items-add").click()
        const skuDialog = workPage(page).getByRole("dialog", { name: "添加商品" })
        await expect(skuDialog).toBeVisible(VISIBLE)
        const skuSearch = skuDialog.locator(
            "#sales-orders-sku-picker-filter-search",
        )
        await skuSearch.fill(SKU_KEYWORD)
        await skuSearch.press("Enter")
        const skuRow = skuDialog.getByRole("checkbox", {
            name: new RegExp(SKU_NAME),
        })
        await expect(skuRow.first()).toBeVisible(VISIBLE)
        await skuRow.first().check()
        await skuDialog.locator("#sales-orders-sku-picker-confirm").click()
        await expect(skuDialog).toBeHidden(VISIBLE)
        await expect(workPage(page).getByText(SKU_NAME).first()).toBeVisible(VISIBLE)
        await workPage(page).getByLabel("数量").fill(SALES_QTY)
        await workPage(page)
            .getByLabel("含税成交单价", { exact: true })
            .fill(SALES_UNIT_PRICE)
        await workPage(page).locator("#sales-orders-create-batch-due-date-open").click()
        await pickCalendarDay(
            page,
            workPage(page).locator("#sales-orders-create-batch-due-date"),
            dueDate,
        )
        await workPage(page).locator("#sales-orders-create-batch-due-date-apply").click()
        await expectToast(page, "已批量设置交期")
        await submitCreatedSalesOrder(page)
        const submitDialog = workPage(page).getByRole("dialog", { name: "提交销售单" })
        await expect(submitDialog.getByText("审批中")).toBeVisible()
        await submitDialog
            .locator("#sales-orders-submit-confirm-confirm")
            .click()
        const negativeProfit = workPage(page).getByRole("alertdialog", { name: "预估毛利为负" })
        await expect(negativeProfit).toBeVisible(VISIBLE)
        await negativeProfit.getByRole("button", { name: "仍然提交", exact: true }).click()
        await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, VISIBLE)
        salesOrderId =
            page.url().split("/sales/orders/")[1]?.split("?")[0] ?? ""
        expect(salesOrderId).toBeTruthy()
        const submittedSales = await fetchSalesPurchaseSummary(salesOrderId)
        contractId = submittedSales.contract_id ?? ""
        expect(contractId).toBeTruthy()
        await expect(
            workPage(page).getByRole("heading", { name: customerName }),
        ).toBeVisible(VISIBLE)
        await expect(documentHeader(page).getByText("审批中")).toBeVisible(
            VISIBLE,
        )
        salesOrderNo = await readDocumentNumber(page)
        expect(submittedSales.purchase_order_count).toBe(0)
        expect(submittedSales.purchase_creation_access).toMatchObject({ allowed: false, task_count: 0 })

        // 2) 采购确认节点：只通过/驳回，不选源
        const restoreProcurementReader = await restrictSalesMaterialReader(
            "caigou",
            stamp,
        )
        try {
            page = await switchTo("caigou")
            await openWorkspaceTask(
                page,
                "销售单审批",
                salesOrderNo,
                "approval",
            )
            await expect(
                workPage(page).getByRole("heading", { name: /销售单/ }),
            ).toBeVisible(VISIBLE)
            await expect(workPage(page).getByText("第 1 轮").first()).toBeVisible(VISIBLE)
            await expect(workPage(page).getByText("采购确认").first()).toBeVisible(
                VISIBLE,
            )
            await expect(workPage(page).getByLabel("供给来源 / 履约责任")).toHaveCount(0)
            await expect(workPage(page).getByLabel("含税成本")).toHaveCount(0)
            await expect(workPage(page).getByLabel("预计交付日")).toHaveCount(0)
            await expect(
                approvalPane(page).getByRole("button", {
                    name: "驳回",
                    exact: true,
                }),
            ).toBeVisible()
            const frozen = await openFrozenApprovalMaterials(page)
            expect(frozen.materials.document_id).toBe(salesOrderId)
            expect(frozen.materials.document_type).toBe("sales_order")
            expect(frozen.materials.attachments).toHaveLength(1)
            const contract = frozen.materials.attachments[0]!
            expect(contract.content_type).toBe("application/pdf")
            contractAssetId = contract.file_asset_id
            await expectGenericSalesMaterialReadsDenied(
                await helperToken("caigou"),
                {
                    salesOrderId,
                    contractId,
                    fileAssetId: contractAssetId,
                },
            )
            await expectApprovalMaterialPreviewAndDownload(page, {
                instanceId: frozen.instanceId,
                file: contract,
                bytes: contractBytes,
            })
            await workPage(page).locator("#workspace-document-paper-dialog-close").click()
            await approveCurrentDocument(page)
        } finally {
            await restoreProcurementReader()
        }
        salesRevisionId = (
            await helperGet<{ current_revision_id: string }>(
                await helperToken("xiaoshou"),
                `/admin/sales-orders/${salesOrderId}`,
            )
        ).current_revision_id
        expect(salesRevisionId).toBeTruthy()

        // 3) 供给分配：创建采购单并立即提交审批
        await page.reload()
        await workPage(page).locator("#workspace-home-refresh").click()
        await openWorkspaceTask(page, "待安排采购", salesOrderNo, "procurement")
        await expect(
            workPage(page).getByRole("heading", { name: "供给分配" }),
        ).toBeVisible(VISIBLE)
        const supplyTask = workPage(page).getByRole("region", {
            name: "当前供给分配任务",
            exact: true,
        })
        await expect(supplyTask).toHaveCount(1, VISIBLE)
        const sourcingTaskId = workspaceTaskId(page)
        expect(sourcingTaskId).toBeTruthy()
        expect(await apiGet(await apiToken("caigou"), `/admin/work-items/${sourcingTaskId}`)).toMatchObject({
            id: sourcingTaskId,
            work_item_type: "PROCUREMENT_ORDER_CREATION",
            business_object_type: "sales_order",
            business_object_id: salesOrderId,
        })
        const products = supplyTask.getByRole("region", {
            name: "供给商品列表",
            exact: true,
        })
        await expect(products).toBeVisible(VISIBLE)
        await expect(products.locator("tbody tr")).toHaveCount(1, VISIBLE)
        await expect(products.locator("tbody tr")).toContainText(SKU_NAME)
        await products.getByTestId("purchase-create-match-best").click()
        await expectToast(page, /已重新分配供给|没有可匹配的供给方案/)

        const editor = await expandSourcingEditor(page, SKU_NAME)
        await expect(editor).toHaveCount(1, VISIBLE)
        const sourcing = editor
            .getByRole("combobox", { name: /^履约方案，/ })
            .and(editor.locator('[id$="-sourcing-option"]'))
        await expect(sourcing).toHaveCount(1, VISIBLE)
        const sourceFieldId = await sourcing.getAttribute("id")
        expect(sourceFieldId).toMatch(
            /^procurement-orders-create-row-.+-sourcing-option$/,
        )
        await chooseOption(page, sourcing, /杭州狮峰.*入仓/, "入仓")
        const targetWarehouseId = sourceFieldId!.replace(
            /-sourcing-option$/,
            "-warehouse",
        )
        const warehouseField = editor
            .getByRole("combobox", { name: "仓库", exact: true })
            .and(editor.locator(`[id=${JSON.stringify(targetWarehouseId)}]`))
        await expect(warehouseField).toHaveCount(1, VISIBLE)
        await expect(warehouseField).toBeVisible(VISIBLE)
        await chooseOption(page, warehouseField, WAREHOUSE_CODE, WAREHOUSE_CODE)
        await fillEmptyDatePickers(page, dueDate)
        await editor.locator("#sourcing-product-adjust-done").click()
        await expectSourcingCounts(page, 1, 0)

        await workPage(page).locator("#procurement-orders-create-preview").click()
        const preview = workPage(page).getByRole("dialog", { name: "预览供给分配" })
        await expect(preview).toBeVisible(VISIBLE)
        await expect(preview.getByText("现有库存分配")).toHaveCount(0)
        await expect(preview.getByText("无需创建采购单")).toHaveCount(0)
        await expect(preview.getByText(/确认提交 1 张采购单/)).toBeVisible()
        const committed = page.waitForResponse(
            (response) =>
                response.request().method() === "POST" &&
                response.url().includes("/admin/purchase-orders/from-sourcing"),
            { timeout: 60_000 },
        )
        await preview
            .locator("#procurement-orders-create-preview-confirm")
            .click()
        const allocation = await committed
        const result = await allocation.json()
        expect(allocation.ok(), JSON.stringify(result)).toBe(true)
        expect(result).toMatchObject({ success: true, data: { stock_reservations: [], work_item_status: "COMPLETED" } })
        expect(result.data.orders).toHaveLength(1)
        await expect(preview).toBeHidden(VISIBLE)

        const caigouToken = await helperToken("caigou")
        let listed = await listPurchasesBySalesOrder(caigouToken, salesOrderId)
        if (listed.length === 0) {
            await page.goto("/procurement/orders")
            await expect(
                workPage(page).getByRole("heading", { name: "采购单", exact: true }),
            ).toBeVisible(VISIBLE)
            await workPage(page)
                .locator("#procurement-orders-list-search")
                .fill(salesOrderNo)
            await workPage(page).locator("#procurement-orders-list-search").press("Enter")
            const fallbackOpen = workPage(page).getByRole("button", {
                name: /打开采购单/,
            })
            await expect(fallbackOpen).toBeVisible(VISIBLE)
            await fallbackOpen.click()
            await expect(page).toHaveURL(
                /\/procurement\/orders\/[^/?#]+/,
                VISIBLE,
            )
            const fallbackId =
                page.url().split("/procurement/orders/")[1]?.split("?")[0] ?? ""
            listed = [{ id: fallbackId, status: "IN_APPROVAL" }]
        }
        expect(listed.length, "供给分配必须创建恰好 1 张采购单").toBe(1)
        expect(String(listed[0]?.status ?? "IN_APPROVAL")).toMatch(
            /IN_APPROVAL|PENDING_FINANCE_REVIEW/,
        )
        const created = await fetchPurchaseCenter(caigouToken, listed[0]!.id)
        snap = await snapshotFromCenter(created)
        if (!snap) throw new Error("采购单快照失败")
        expect(String(created.status)).toMatch(
            /IN_APPROVAL|PENDING_FINANCE_REVIEW/,
        )
        expect(created.payable_summary ?? null).toBeNull()
        expect(created.approval?.instance?.current_round_no).toBe(1)
        expect(
            created.approval?.instance?.current_node_name ??
                created.approval?.instance?.current_node,
        ).toMatch(/采购部门审批/)
        expect(created.content_source).toBe("SUBMISSION")

        page = await switchTo("xiaoshou")
        await page.goto(`/sales/orders/${salesOrderId}`)
        await expect(documentHeader(page).getByText("已生效")).toBeVisible(
            VISIBLE,
        )
        await expectCoveredByOnePurchase(salesOrderId)

        // 4) 采购部门在采购单审批首节点驳回
        const unrelatedAssetId = await uploadUnrelatedSalesMaterial(
            contractBytes,
            stamp,
        )
        expect(unrelatedAssetId).not.toBe(contractAssetId)
        const restoreFinanceReader = await restrictSalesMaterialReader(
            "caigoushenpi",
            stamp,
        )
        try {
            page = await switchTo("caigoushenpi")
            const financeToken = await helperToken("caigoushenpi")
            await expectGenericSalesMaterialReadsDenied(financeToken, {
                salesOrderId,
                contractId,
                fileAssetId: contractAssetId,
            })
            const context = await expectPurchaseSalesContext(
                page,
                financeToken,
                {
                    purchaseOrderId: snap.id,
                    salesOrderId,
                    salesOrderNo,
                    salesRevisionId,
                    customerName,
                    contractNo,
                    skuName: SKU_NAME,
                    salesUnitPrice: SALES_UNIT_PRICE,
                    salesQuantity: SALES_QTY,
                    salesGrossTotal: "274.00",
                    contractBytes,
                    contractAssetId,
                    unrelatedAssetId,
                },
            )
            await openWorkspaceTask(
                page,
                "采购单审批",
                snap.purchaseNo,
                "approval",
            )
            const roundOne = approvalPane(page)
            await expect(roundOne.getByText("第 1 轮")).toBeVisible(VISIBLE)
            await expect(roundOne.getByText("采购部门审批")).toBeVisible(
                VISIBLE,
            )
            await expect(roundOne.getByText(SKU_NAME).first()).toBeVisible()
            const frozen = await openFrozenApprovalMaterials(
                page,
                salesRevisionId,
            )
            expect(frozen.materials.document_id).toBe(snap.id)
            expect(frozen.materials.document_type).toBe("purchase_order")
            const source = frozen.materials.display.source_sales
            expect(source).toHaveLength(1)
            expect(source![0]!.document_id).toBe(salesOrderId)
            expect(source![0]!.document_no).toBe(salesOrderNo)
            expect(source![0]!.revision_id).toBe(salesRevisionId)
            expect(source![0]!.source.customer).toBe(customerName)
            expect(source![0]!.revision_no).toBe(1)
            expect(source![0]!.source.amount_label).toMatch(/^¥274(?:\.0+)?$/)
            expect(source![0]!.source.lines).toHaveLength(1)
            expect(source![0]!.source.extra_sections).toContainEqual({
                label: "合同",
                value: contractNo,
                numeric: false,
                object_id: null,
            })
            expect(source![0]!.source.lines[0]!.quantity).toMatch(
                /销售单价 ¥137(?:\.0+)?/,
            )
            expect(source![0]!.source.lines[0]!.quantity).toMatch(
                /^2(?:\.0+)? /,
            )
            expect(source![0]!.source.lines[0]!.quantity).toMatch(
                /¥274(?:\.0+)?$/,
            )
            const salesPaper = workPage(page).getByRole("dialog", {
                name: "销售单预览",
                exact: true,
            })
            await expect(salesPaper).toContainText(contractNo)
            const completeSource = frozen.materials.source_sales_orders[0]!
            expect(completeSource.document_no).toBe(salesOrderNo)
            expect(completeSource.revision.id).toBe(salesRevisionId)
            expect(completeSource.revision.revision_no).toBe(1)
            expect(completeSource.revision.customer_name).toBe(customerName)
            expect(completeSource.revision.contract_no).toBe(contractNo)
            expect(completeSource.revision.gross_amount).toBe("274.00")
            expect(completeSource.revision.commercial_lines).toHaveLength(1)
            const fullLine = completeSource.revision.commercial_lines[0]!
            expect(fullLine.id).toBe(
                context.source_sales_order!.lines[0]!
                    .sales_order_revision_line_id,
            )
            expect(fullLine.quantity).toMatch(/^2(?:\.0+)?$/)
            expect(fullLine.unit_price_gross).toMatch(/^137(?:\.0+)?$/)
            expect(fullLine.gross_amount).toBe("274.00")
            const contract = frozen.materials.attachments.find(
                (file) => file.file_asset_id === contractAssetId,
            )
            expect(contract).toBeTruthy()
            await expectApprovalMaterialPreviewAndDownload(page, {
                instanceId: frozen.instanceId,
                file: contract!,
                bytes: contractBytes,
            })
            for (const action of ["preview", "download"] as const) {
                const denied = await page.request.get(
                    `${API_BASE}/admin/approval-instances/${frozen.instanceId}/materials/${unrelatedAssetId}/${action}`,
                    { headers: { Authorization: `Bearer ${financeToken}` } },
                )
                expect(
                    denied.status(),
                    "采购审批白名单不得读取未关联资产",
                ).toBe(404)
            }
            await testInfo.attach("purchase-source-sales-contract-acceptance", {
                contentType: "application/json",
                body: Buffer.from(
                    JSON.stringify(
                        {
                            purchase_order_id: snap.id,
                            purchase_submission_id: snap.submissionId,
                            purchase_approval_instance_id: frozen.instanceId,
                            sales_order_id: salesOrderId,
                            sales_revision_id:
                                context.source_sales_order!.revision_id,
                            sales_revision_no:
                                context.source_sales_order!.revision_no,
                            contract_id: contractId,
                            contract_file_asset_id: contractAssetId,
                            unrelated_file_asset_id: unrelatedAssetId,
                            lines: context.lines.map((line) => ({
                                sales_revision_line_id:
                                    line.sales_order_revision_line_id,
                                purchase_unit_cost_gross: line.unit_cost_gross,
                                sales_unit_price_gross:
                                    context.source_sales_order!.lines.find(
                                        (sales) =>
                                            sales.sales_order_revision_line_id ===
                                            line.sales_order_revision_line_id,
                                    )?.unit_price_gross,
                            })),
                            verification: {
                                generic_sales_contract_file_reads: "403",
                                procurement_sales_approval_contract_preview_download:
                                    "200; bytes equal fixture",
                                finance_purchase_detail_and_contract_preview_download:
                                    "200; bytes equal fixture",
                                purchase_unrelated_file_download: "403 or 404",
                                finance_purchase_approval_source_sales_and_contract:
                                    "200; bytes equal fixture",
                            },
                        },
                        null,
                        2,
                    ),
                ),
            })
            await workPage(page).locator("#workspace-document-paper-dialog-close").click()
            await rejectCurrentDocument(page, REJECT_REASON)
        } finally {
            await restoreFinanceReader()
        }

        await page.goto("/workspace/tasks")
        await selectWorkspaceFamily(page, "approval")
        await expectNoQueueTask(page, documentTaskPattern("采购单审批", [snap.purchaseNo]))

        // 5) 驳回后：不生效、不形成应付、内容不变、禁止变更单/履约/付款
        page = await switchTo("caigou")
        await page.goto("/procurement/orders")
        await expect(
            workPage(page).getByRole("heading", { name: "采购单", exact: true }),
        ).toBeVisible(VISIBLE)
        await workPage(page).locator("#procurement-orders-list-search").fill(salesOrderNo)
        await workPage(page).locator("#procurement-orders-list-search").press("Enter")
        const openPo = workPage(page).getByRole("button", {
            name: new RegExp(`打开采购单 ${snap.purchaseNo}`),
        })
        await expect(openPo).toBeVisible(VISIBLE)
        await openPo.click()
        await expect(page).toHaveURL(/\/procurement\/orders\/[^/?#]+/, VISIBLE)
        purchaseHref = page.url().split("?")[0] ?? page.url()
        await expect(
            documentHeader(page).getByText("已驳回", { exact: true }).first(),
        ).toBeVisible(VISIBLE)
        await expect(documentHeader(page).getByText("已生效")).toHaveCount(0)
        await expect(
            workPage(page).locator('[aria-label="采购单摘要"]').getByText("未付"),
        ).toBeVisible()
        await expect(
            workPage(page).locator('[aria-label="采购单摘要"]').getByText("未开始"),
        ).toBeVisible()
        await expect(
            workPage(page).locator("#procurement-orders-detail-change"),
        ).toHaveCount(0)
        await expect(workPage(page).getByRole("button", { name: "去交付" })).toHaveCount(
            0,
        )

        await workPage(page).getByRole("tab", { name: /^概览/ }).click()
        await expect(workPage(page).getByText(SKU_NAME).first()).toBeVisible(VISIBLE)
        await expect(
            workPage(page).getByText(new RegExp(`${snap.quantity}`)).first(),
        ).toBeVisible()
        const facts = workPage(page).getByRole("region", { name: "关联单据与基本信息" })
        await expect(facts.getByText("草稿", { exact: true })).toBeVisible()
        await expect(facts.getByText("尚未生效", { exact: true })).toBeVisible()

        await workPage(page).getByRole("tab", { name: /^审批/ }).click()
        await expect(workPage(page).getByText("采购部门审批", { exact: true }).first()).toBeVisible(VISIBLE)
        await expect(workPage(page).getByText(/^最近驳回/)).toBeVisible(VISIBLE)
        await expect(workPage(page).getByText(REJECT_REASON).first()).toBeVisible(VISIBLE)
        await expect(workPage(page).getByText("已驳回").first()).toBeVisible()
        await expect(workPage(page).getByText("第 1 轮").first()).toBeVisible()

        await workPage(page).getByRole("tab", { name: /^票款/ }).click()
        await expect(
            workPage(page).getByText("尚未形成应付（需审批通过）。"),
        ).toBeVisible(VISIBLE)

        await workPage(page).getByRole("tab", { name: /^履约/ }).click()
        await expect(workPage(page).getByText("履约进度")).toBeVisible(VISIBLE)

        await workPage(page).getByRole("tab", { name: /^变更/ }).click()
        await expect(workPage(page).getByText("暂无采购变更")).toBeVisible(VISIBLE)
        await expect(
            workPage(page).getByRole("button", { name: "发起改单" }),
        ).toHaveCount(0)
        await expect(
            workPage(page).locator("#procurement-orders-detail-change"),
        ).toHaveCount(0)

        const rejected = await fetchPurchaseCenter(caigouToken, snap.id)
        expectSameContent(rejected, snap)
        expect(rejected.approval?.instance?.status).toBe("REJECTED")
        expect(rejected.approval?.instance?.current_round_no).toBe(1)
        expect(rejected.approval?.instance?.latest_rejection).toBe(
            REJECT_REASON,
        )
        const originalHistory = await apiGet<{
            items: Array<{ round_no: number; result: string; decision_reason?: string }>
        }>(caigouToken, `/admin/approval-instances/${snap.instanceId}/history`, { limit: 50 })
        expect(originalHistory.items).toContainEqual(expect.objectContaining({
            round_no: 1, result: "REJECTED", decision_reason: REJECT_REASON,
        }))

        page = await switchTo("cangchu")
        await page.goto("/workspace/tasks")
        await expect(
            workPage(page).getByRole("heading", { name: "待我处理" }),
        ).toBeVisible(VISIBLE)
        await selectWorkspaceFamily(page, "fulfillment")
        // 工作台搜索不匹配单号，填了会把列表滤空。驳回未生效的采购单不应给本单产生履约任务。
        await expectNoQueueTask(page, documentTaskPattern("履约处理", [
            snap.purchaseNo, salesOrderNo, customerName,
        ]))

        page = await switchTo("fukuan")
        await page.goto("/workspace/tasks")
        await expect(
            workPage(page).getByRole("heading", { name: "待我处理" }),
        ).toBeVisible(VISIBLE)
        await selectWorkspaceFamily(page, "finance")
        // 同上，不按搜索框断言；驳回未生效前本单不得出现付款任务。
        await expectNoQueueTask(page, /待执行采购付款/)
        await selectWorkspaceFamily(page, "approval")
        await expect(workPage(page).getByText("供应商付款单审批")).toHaveCount(0)

        // 6) 提交人在驳回待办打开原单、改价并填写重提说明。
        page = await switchTo("caigou")
        await openWorkspaceTask(page, "驳回待处理", snap.purchaseNo, "approval")
        await expect(workPage(page).getByText("本次审批已驳回", { exact: true })).toBeVisible(VISIBLE)
        await expect(workPage(page).getByText(REJECT_REASON, { exact: true }).first()).toBeVisible(VISIBLE)
        const orderPath = `/procurement/orders/${snap.id}`
        const revise = workPage(page).locator('a[id^="workspace-rework-"][id$="-edit"]')
        await expect(revise).toHaveText("修改或重新提交", VISIBLE)
        await revise.click()
        await expect(page).toHaveURL((url) =>
            url.pathname === orderPath && url.searchParams.get("mode") === "edit", VISIBLE)
        await expect(workPage(page).getByText("被驳回待修改", { exact: true }).first()).toBeVisible(VISIBLE)
        const costInput = workPage(page)
            .locator(
                'input[id^="procurement-orders-detail-edit-row-"][id$="-cost"]',
            )
            .first()
        await expect(costInput).toBeVisible(VISIBLE)
        await costInput.fill("99")
        const resubmitting = page.waitForResponse(
            (response) =>
                response.request().method() === "POST" &&
                response.url().includes(`/purchase-orders/${snap!.id}/submit`),
            { timeout: 30_000 },
        )
        await workPage(page)
            .locator(`[id="procurement-orders-detail-edit-submit-${snap.id}"]`)
            .click()
        const submit = workPage(page).getByRole("alertdialog", {
            name: "确认提交采购单",
            exact: true,
        })
        await expect(submit).toBeVisible(VISIBLE)
        const confirmResubmission = submit.getByRole("button", { name: "确认提交", exact: true })
        await expect(confirmResubmission).toBeDisabled()
        await submit.getByLabel("重新提交说明").fill("按驳回意见修正采购报价后重提")
        await confirmResubmission.click()
        expect((await resubmitting).ok()).toBeTruthy()
        await expect(submit).toBeHidden(VISIBLE)
        const resubmitted = await fetchPurchaseCenter(caigouToken, snap.id)
        expect(resubmitted.id).toBe(snap.id)
        expect(resubmitted.purchase_no).toBe(snap.purchaseNo)
        expect(resubmitted.status).toBe("IN_APPROVAL")
        expect(resubmitted.current_submission_id).toBeTruthy()
        expect(resubmitted.current_submission_id).not.toBe(snap.submissionId)
        expect(resubmitted.current_submission_id).not.toBe(rejected.current_submission_id)
        expect(resubmitted.approval?.instance?.id).not.toBe(snap.instanceId)
        expect(resubmitted.approval?.instance?.current_round_no).toBe(1)
        expect(resubmitted.lines?.[0]?.unit_cost_gross).toMatch(/^99(?:\.0+)?$/)
        const retainedHistory = await apiGet<typeof originalHistory>(
            caigouToken, `/admin/approval-instances/${snap.instanceId}/history`, { limit: 50 },
        )
        expect(retainedHistory.items).toEqual(originalHistory.items)

        expect(resubmitted.lines?.[0]?.quantity).toBe(snap.quantity)
        expect(resubmitted.payable_summary ?? null).toBeNull()
        expect(
            (await listPurchasesBySalesOrder(caigouToken, salesOrderId)).map(
                (item) => item.id,
            ),
        ).toEqual([snap.id])
        page = await switchTo("caigoushenpi")
        await openWorkspaceTask(page, "采购单审批", snap.purchaseNo, "approval")
        await expect(approvalPane(page).getByText("第 1 轮")).toBeVisible(
            VISIBLE,
        )
        await approveCurrentDocument(page)

        page = await switchTo("caigou")
        await page.goto(purchaseHref)
        await expect(
            documentHeader(page).getByText("已生效").first(),
        ).toBeVisible(VISIBLE)
        // 付款走 W01 出纳任务，采购单详情页头没有独立付款按钮。
        await expect(
            workPage(page).locator("#procurement-orders-detail-pay"),
        ).toHaveCount(0)
        await expect(
            workPage(page).locator("#procurement-orders-detail-change"),
        ).toBeVisible(VISIBLE)
        await expect(
            workPage(page).locator("#procurement-orders-detail-change"),
        ).toBeEnabled()
        await workPage(page).getByRole("tab", { name: /^票款/ }).click()
        await expect(workPage(page).getByText("应付未结")).toBeVisible(VISIBLE)
        await expect(
            workPage(page).getByText("尚未形成应付（需审批通过）。"),
        ).toHaveCount(0)

        page = await switchTo("fukuan")
        await page.goto("/workspace/tasks")
        await expect(
            workPage(page).getByRole("heading", { name: "待我处理" }),
        ).toBeVisible(VISIBLE)
        await selectWorkspaceFamily(page, "finance")
        await expectNoQueueTask(page, /待执行采购付款/)

        const effective = await fetchPurchaseCenter(caigouToken, snap.id)
        expect(String(effective.status)).toBe("EFFECTIVE")
        expect(effective.revision_no).toBe(1)
        expect(String(effective.current_submission_id ?? "")).toBe(
            resubmitted.current_submission_id,
        )
        expect(String(effective.approval?.instance?.id ?? "")).toBe(
            resubmitted.approval?.instance?.id,
        )
        expect(effective.approval?.instance?.status).toBe("APPROVED")
        expect(effective.approval?.instance?.current_round_no).toBe(1)
        expect(effective.payable_summary).toBeTruthy()
        expect(
            Number(effective.payable_summary?.payable_open_amount ?? "0"),
        ).toBeGreaterThan(0)
        expect(effective.totals?.gross).toBe(resubmitted.totals?.gross)
        expect(effective.totals?.gross).not.toBe(snap.gross)
        const effectiveLine =
            effective.lines?.find((item) =>
                item.product_name?.includes(SKU_NAME),
            ) ?? effective.lines?.[0]
        expect(String(effectiveLine?.quantity ?? "")).toBe(snap.quantity)
        expect(effectiveLine?.unit_cost_gross).toMatch(/^99(?:\.0+)?$/)

        page = await switchTo("xiaoshou")
        await page.goto(`/sales/orders/${salesOrderId}`)
        await expect(documentHeader(page).getByText("已生效")).toBeVisible(
            VISIBLE,
        )
        await expectCoveredByOnePurchase(salesOrderId)
    } finally {
        await closeSession(session)
    }
})
