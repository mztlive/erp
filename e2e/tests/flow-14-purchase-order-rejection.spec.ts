/**
 * 流程: [flow-14] 采购单审批驳回
 * 文档: docs/erp-phase-1.md §7.1/§7.2（采购审批驳回：轮次加一回到首节点）；
 *       approval-workflow-contract.md §4.3/§4.4、§11；workbench-workitem-contract.md 第 3 节
 * 账号: admin（补采购责任默认调度人）→ xiaoshou（客户/合同/销售单）
 *       → caigou（销售单采购确认、供给分配、原采购单改价重提）→ caiwu（采购单驳回及新提交审批）
 *       → cangchu / fukuan 仅负向断言（驳回未生效前不得履约、不得付款）
 * 最新业务验收：采购明细显示客户真实成交价；财务从采购单和审批读取关联销售版本、
 * 合同；采购销售审批读取冻结合同。临时收窄普通销售、合同、文件权限后仍须可操作
 * 精确上下文，预览 Blob 和浏览器下载均须与真实合同 PDF 原字节相同。
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
import { archiveContractViaUi } from "../helpers/contracts"
import fs from "node:fs"
import path from "node:path"

import {
    test,
    expect,
    type BrowserContext,
    type Locator,
    type Page,
} from "../helpers/test"

import { apiGet, apiToken } from "../helpers/api"
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
    expectToast,
    openWorkspaceTask,
    pickCalendarDay,
    readHeaderDocumentNumber,
    selectWorkspaceFamily,
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
    | "caiwu"
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
    const empty = page.getByRole("button", { name: "选择日期" })
    const total = await empty.count()
    for (let index = 0; index < total; index += 1) {
        const remaining = page.getByRole("button", { name: "选择日期" })
        if ((await remaining.count()) === 0) break
        await pickCalendarDay(page, remaining.first(), isoDate)
    }
}

function documentHeader(page: Page): Locator {
    return page.locator("header")
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

function approvalPane(page: Page): Locator {
    return page.getByRole("region", { name: "当前任务", exact: true })
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
    const dialog = page.getByRole("dialog", { name: "确认驳回" })
    await expect(dialog).toBeVisible(VISIBLE)
    await expect(dialog.getByText(/驳回后.*首节点|下一轮审批/)).toBeVisible()
    await dialog.getByLabel("驳回原因").fill(reason)
    await dialog.getByRole("button", { name: "确认驳回" }).click()
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
    expect(String(live.status)).toMatch(/IN_APPROVAL|PENDING_FINANCE_REVIEW/)
    expect(String(live.current_submission_id ?? "")).toBe(snap.submissionId)
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
            page
                .getByRole("heading", { name: "新建销售单" })
                .or(page.getByRole("heading", { name: "业务信息" })),
        ).toBeVisible(VISIBLE)
        await expect(page.locator("#sales-orders-create-contract")).toBeVisible(
            VISIBLE,
        )
        await page
            .locator("#sales-orders-create-contract-upload")
            .click()
        await archiveContractViaUi(page, { contractNo, customerName, pdf: contractUpload })
        await expect(
            page.getByText(`${contractNo}@v1`, { exact: true }).first(),
        ).toBeVisible(VISIBLE)
        await expect(page.locator("#sales-orders-create-customer")).toHaveValue(new RegExp(customerName))

        await chooseOption(page, page.getByLabel("福利场景"), "年节礼包")
        const salesPayment = page.locator(
            "#sales-orders-create-header-payment-terms",
        )
        if (!(await salesPayment.inputValue().catch(() => "")).trim()) {
            await chooseOption(
                page,
                salesPayment,
                /货到 15 天|按合同约定/,
                "货到 15 天",
            )
        }
        await expect(page.getByLabel("供应商")).toHaveCount(0)
        await expect(page.getByLabel("履约责任")).toHaveCount(0)
        await expect(page.getByLabel("采购成本")).toHaveCount(0)

        await page.locator("#sales-orders-create-line-items-add").click()
        const skuDialog = page.getByRole("dialog", { name: "添加商品" })
        await expect(skuDialog).toBeVisible(VISIBLE)
        const skuSearch = skuDialog.getByPlaceholder(
            "搜索 SKU、商品名称、编号或规格",
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
        await expect(page.getByText(SKU_NAME).first()).toBeVisible(VISIBLE)
        await page.getByLabel("数量").fill(SALES_QTY)
        await page
            .getByLabel("含税成交单价", { exact: true })
            .fill(SALES_UNIT_PRICE)
        await expect(page.getByText("手动成交价", { exact: true })).toBeVisible(
            VISIBLE,
        )
        await page.locator("#sales-orders-create-batch-due-date-open").click()
        await pickCalendarDay(
            page,
            page.locator("#sales-orders-create-batch-due-date"),
            dueDate,
        )
        await page.locator("#sales-orders-create-batch-due-date-apply").click()
        await expectToast(page, "已批量设置交期")
        await submitCreatedSalesOrder(page)
        const submitDialog = page.getByRole("dialog", { name: "提交销售单" })
        await expect(submitDialog.getByText("审批中")).toBeVisible()
        await submitDialog
            .locator("#sales-orders-submit-confirm-confirm")
            .click()
        await expect(page).toHaveURL(/\/sales\/orders\/[^/?]+/, VISIBLE)
        salesOrderId =
            page.url().split("/sales/orders/")[1]?.split("?")[0] ?? ""
        expect(salesOrderId).toBeTruthy()
        contractId = (
            await helperGet<{ contract_id: string }>(
                await helperToken("xiaoshou"),
                `/admin/sales-orders/${salesOrderId}`,
            )
        ).contract_id
        expect(contractId).toBeTruthy()
        await expect(
            page.getByRole("heading", { name: customerName }),
        ).toBeVisible(VISIBLE)
        await expect(documentHeader(page).getByText("审批中")).toBeVisible(
            VISIBLE,
        )
        salesOrderNo = await readDocumentNumber(page)
        await page.getByRole("tab", { name: /^采购/ }).click()
        await expect(
            page.getByTestId("sales-order-purchase-status"),
        ).toContainText("待采购", VISIBLE)

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
                page.getByRole("heading", { name: /销售单/ }),
            ).toBeVisible(VISIBLE)
            await expect(page.getByText("第 1 轮").first()).toBeVisible(VISIBLE)
            await expect(page.getByText("采购确认").first()).toBeVisible(
                VISIBLE,
            )
            await expect(page.getByLabel("供给来源 / 履约责任")).toHaveCount(0)
            await expect(page.getByLabel("含税成本")).toHaveCount(0)
            await expect(page.getByLabel("预计交付日")).toHaveCount(0)
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
            await page.locator("#workspace-document-paper-dialog-close").click()
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
        await page.locator("#workspace-home-refresh").click()
        await openWorkspaceTask(page, "待供给分配", salesOrderNo, "procurement")
        await expect(
            page.getByRole("heading", { name: "供给分配" }),
        ).toBeVisible(VISIBLE)
        await expect(
            page.getByRole("heading", { name: "销售明细与供给方案" }),
        ).toBeVisible(VISIBLE)
        await page.getByTestId("purchase-create-match-best").click()
        await expectToast(page, /已重新分配供给|没有可匹配的供给方案/)

        await expandSourcingEditor(page)
        const sourcing = page.getByRole("combobox", { name: /^履约方案，/ })
        await chooseOption(page, sourcing, /入仓/)
        const warehouseField = page.getByRole("combobox", {
            name: "仓库",
            exact: true,
        })
        await expect(warehouseField).toBeVisible(VISIBLE)
        await chooseOption(page, warehouseField, WAREHOUSE_CODE, WAREHOUSE_CODE)
        await fillEmptyDatePickers(page, dueDate)
        await expect(
            page.getByText("将创建采购单").locator("xpath=.."),
        ).toContainText("1 张")
        await expect(
            page.getByText("将建立库存预留").locator("xpath=.."),
        ).toContainText("0 条")

        await page.locator("#procurement-orders-create-preview").click()
        const preview = page.getByRole("dialog", { name: "预览供给分配" })
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
        expect((await committed).ok()).toBeTruthy()

        await expectToast(page, /供给分配已完成|已创建 1 张采购单并提交审批/)
        await expect(
            page
                .locator('[data-slot="toast-description"]')
                .filter({ hasText: /无需采购/ }),
        ).toHaveCount(0)

        const caigouToken = await helperToken("caigou")
        let listed = await listPurchasesBySalesOrder(caigouToken, salesOrderId)
        if (listed.length === 0) {
            await page.goto("/procurement/orders")
            await expect(
                page.getByRole("heading", { name: "采购单", exact: true }),
            ).toBeVisible(VISIBLE)
            await page
                .locator("#procurement-orders-list-search")
                .fill(salesOrderNo)
            await page.locator("#procurement-orders-list-search").press("Enter")
            const fallbackOpen = page.getByRole("button", {
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
        ).toMatch(/财务总监审批/)
        expect(created.content_source).toBe("SUBMISSION")

        page = await switchTo("xiaoshou")
        await page.goto(`/sales/orders/${salesOrderId}`)
        await expect(documentHeader(page).getByText("已生效")).toBeVisible(
            VISIBLE,
        )
        await page.getByRole("tab", { name: /^采购/ }).click()
        await expect(
            page.getByTestId("sales-order-purchase-status"),
        ).toContainText("采购已覆盖")
        await expect(page.getByText("草稿")).toHaveCount(0)
        // 销售账号无采购单明细查看权限，面板仅显示计数提示，不显示审批中。
        await expect(
            page.getByTestId("sales-order-purchase-count-only"),
        ).toContainText("已创建 1 张采购单")

        // 4) 财务在采购单审批首节点驳回
        const unrelatedAssetId = await uploadUnrelatedSalesMaterial(
            contractBytes,
            stamp,
        )
        expect(unrelatedAssetId).not.toBe(contractAssetId)
        const restoreFinanceReader = await restrictSalesMaterialReader(
            "caiwu",
            stamp,
        )
        try {
            page = await switchTo("caiwu")
            const financeToken = await helperToken("caiwu")
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
            await expect(roundOne.getByText("财务总监审批")).toBeVisible(
                VISIBLE,
            )
            await expect(roundOne.getByText(SKU_NAME).first()).toBeVisible()
            const frozen = await openFrozenApprovalMaterials(page)
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
            await expect(
                page.getByRole("heading", { name: "关联销售单", exact: true }),
            ).toBeVisible(VISIBLE)
            await expect(
                page.getByRole("dialog").filter({
                    has: page.getByRole("heading", {
                        name: "审批提交资料",
                    }),
                }),
            ).toContainText(contractNo)
            const contract = frozen.materials.attachments.find(
                (file) => file.file_asset_id === contractAssetId,
            )
            expect(contract).toBeTruthy()
            await expectApprovalMaterialPreviewAndDownload(page, {
                instanceId: frozen.instanceId,
                file: contract!,
                bytes: contractBytes,
            })
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
            await page.locator("#workspace-document-paper-dialog-close").click()
            await rejectCurrentDocument(page, REJECT_REASON)
        } finally {
            await restoreFinanceReader()
        }

        await page.locator("#workspace-home-refresh").click()
        await openWorkspaceTask(page, "采购单审批", snap.purchaseNo, "approval")
        const roundTwo = approvalPane(page)
        await expect(roundTwo.getByText("第 2 轮")).toBeVisible(VISIBLE)
        await expect(roundTwo.getByText("财务总监审批").first()).toBeVisible(
            VISIBLE,
        )
        await expect(roundTwo.getByText("最近驳回")).toBeVisible(VISIBLE)
        await expect(roundTwo.getByText(REJECT_REASON).first()).toBeVisible(
            VISIBLE,
        )

        // 5) 驳回后：不生效、不形成应付、内容不变、禁止变更单/履约/付款
        page = await switchTo("caigou")
        await page.goto("/procurement/orders")
        await expect(
            page.getByRole("heading", { name: "采购单", exact: true }),
        ).toBeVisible(VISIBLE)
        await page.locator("#procurement-orders-list-search").fill(salesOrderNo)
        await page.locator("#procurement-orders-list-search").press("Enter")
        const openPo = page.getByRole("button", {
            name: new RegExp(`打开采购单 ${snap.purchaseNo}`),
        })
        await expect(openPo).toBeVisible(VISIBLE)
        await openPo.click()
        await expect(page).toHaveURL(/\/procurement\/orders\/[^/?#]+/, VISIBLE)
        purchaseHref = page.url().split("?")[0] ?? page.url()
        await expect(
            documentHeader(page).getByText("审批中").first(),
        ).toBeVisible(VISIBLE)
        await expect(documentHeader(page).getByText("已生效")).toHaveCount(0)
        await expect(
            page.locator('[aria-label="采购单摘要"]').getByText("未付"),
        ).toBeVisible()
        await expect(
            page.locator('[aria-label="采购单摘要"]').getByText("未开始"),
        ).toBeVisible()
        await expect(
            page.locator("#procurement-orders-detail-change"),
        ).toHaveCount(0)
        await expect(
            page.locator("#procurement-orders-detail-submit"),
        ).toHaveCount(0)
        await expect(page.getByRole("button", { name: "去交付" })).toHaveCount(
            0,
        )
        await expect(
            page.getByRole("button", { name: "提交审批" }),
        ).toHaveCount(0)

        await page.getByRole("tab", { name: /^概览/ }).click()
        await expect(page.getByText(SKU_NAME).first()).toBeVisible(VISIBLE)
        await expect(
            page.getByText(new RegExp(`${snap.quantity}`)).first(),
        ).toBeVisible()
        await expect(page.getByText("已提交内容")).toBeVisible()
        await expect(page.getByText("生效版本")).toHaveCount(0)

        await page.getByRole("tab", { name: /^审批/ }).click()
        await expect(page.getByText("第 2 轮").first()).toBeVisible(VISIBLE)
        await expect(page.getByText("财务总监审批").first()).toBeVisible(
            VISIBLE,
        )
        await expect(page.getByText("最近驳回")).toBeVisible(VISIBLE)
        await expect(page.getByText(REJECT_REASON).first()).toBeVisible(VISIBLE)
        await expect(page.getByText("已驳回").first()).toBeVisible()
        await expect(page.getByText("第 1 轮").first()).toBeVisible()

        await page.getByRole("tab", { name: /^票款/ }).click()
        await expect(
            page.getByText("尚未形成应付（需审批通过）。"),
        ).toBeVisible(VISIBLE)

        await page.getByRole("tab", { name: /^履约/ }).click()
        await expect(page.getByText("履约进度")).toBeVisible(VISIBLE)

        await page.getByRole("tab", { name: /^变更/ }).click()
        await expect(page.getByText("暂无采购变更")).toBeVisible(VISIBLE)
        await expect(
            page.getByRole("button", { name: "发起采购变更" }),
        ).toHaveCount(0)
        await expect(
            page.locator("#procurement-orders-detail-change"),
        ).toHaveCount(0)

        const rejected = await fetchPurchaseCenter(caigouToken, snap.id)
        expectSameContent(rejected, snap)
        expect(rejected.approval?.instance?.status).toBe("RUNNING")
        expect(rejected.approval?.instance?.current_round_no).toBe(2)
        expect(rejected.approval?.instance?.latest_rejection).toBe(
            REJECT_REASON,
        )
        expect(
            rejected.approval?.instance?.current_node_name ??
                rejected.approval?.instance?.current_node,
        ).toMatch(/财务总监审批/)
        const roundOneHistory = (
            rejected.approval?.recent_history ?? []
        ).filter((item) => item.round_no === 1)
        expect(
            roundOneHistory.some((item) => item.result === "REJECTED"),
        ).toBeTruthy()
        expect(
            roundOneHistory.some(
                (item) => item.decision_reason === REJECT_REASON,
            ),
        ).toBeTruthy()

        page = await switchTo("cangchu")
        await page.goto("/workspace")
        await expect(
            page.getByRole("heading", { name: "我的工作台" }),
        ).toBeVisible(VISIBLE)
        await selectWorkspaceFamily(page, "fulfillment")
        // 工作台搜索不匹配单号，填了会把列表滤空。驳回未生效的采购单不应给本单产生履约任务。
        await expect(
            page.getByRole("button", {
                name: documentTaskPattern("履约处理", [
                    snap.purchaseNo,
                    salesOrderNo,
                    customerName,
                ]),
            }),
        ).toHaveCount(0)

        page = await switchTo("fukuan")
        await page.goto("/workspace")
        await expect(
            page.getByRole("heading", { name: "我的工作台" }),
        ).toBeVisible(VISIBLE)
        await selectWorkspaceFamily(page, "finance")
        // 同上，不按搜索框断言；驳回未生效前本单不得出现付款任务。
        await expect(
            page.getByRole("button", {
                name: documentTaskPattern("供应商付款处理", [snap.purchaseNo]),
            }),
        ).toHaveCount(0)
        await selectWorkspaceFamily(page, "approval")
        await expect(page.getByText("供应商付款单审批")).toHaveCount(0)

        // 6) 修改原采购单：撤回旧审批，编辑含税单价，保留 ID 和采购单号再提交。
        page = await switchTo("caigou")
        await page.goto(purchaseHref)
        const revise = page.locator(
            `[id="procurement-orders-detail-cancel-approval-trigger-${snap.id}"]`,
        )
        await expect(revise).toHaveText("修改原单")
        await revise.click()
        const cancel = page.getByRole("alertdialog", {
            name: "修改原单",
            exact: true,
        })
        await expect(cancel).toBeVisible(VISIBLE)
        await cancel.getByLabel("撤回原因").fill("按驳回意见修正采购报价后重提")
        await cancel
            .getByRole("button", { name: "撤回并修改原单", exact: true })
            .click()
        await expect(page).toHaveURL(
            new RegExp(`/procurement/orders/${snap.id}\\?mode=edit`),
            VISIBLE,
        )
        await expect(page.getByText("采购草稿", { exact: true })).toBeVisible(
            VISIBLE,
        )
        const cancelled = await fetchPurchaseCenter(caigouToken, snap.id)
        expect(cancelled.id).toBe(snap.id)
        expect(cancelled.purchase_no).toBe(snap.purchaseNo)
        expect(cancelled.status).toBe("DRAFT")
        expect(cancelled.approval?.instance?.id).toBe(snap.instanceId)
        expect(cancelled.approval?.instance?.status).toBe("CANCELLED")
        expect(cancelled.current_submission_id).not.toBe(snap.submissionId)
        // 草稿是独立快照副本，快照行 ID 更新；全部业务内容与稳定来源身份必须保留。
        const businessContent = (lines: PurchaseCenter["lines"]) =>
            lines?.map(({ line_id: _snapshotLineId, ...content }) => content)
        expect(businessContent(cancelled.lines)).toEqual(
            businessContent(rejected.lines),
        )
        const cancelledHistory = await apiGet<{
            items: Array<{ result: string; decision_reason?: string }>
        }>(
            caigouToken,
            `/admin/approval-instances/${snap.instanceId}/history`,
            { limit: 50 },
        )
        expect(
            cancelledHistory.items.some(
                (item) =>
                    item.result === "REJECTED" &&
                    item.decision_reason === REJECT_REASON,
            ),
        ).toBe(true)
        expect(
            cancelledHistory.items.some((item) => item.result === "CANCELLED"),
        ).toBe(true)
        const costInput = page
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
        await page
            .locator(`[id="procurement-orders-detail-edit-submit-${snap.id}"]`)
            .click()
        const submit = page.getByRole("alertdialog", {
            name: "确认提交采购单",
            exact: true,
        })
        await expect(submit).toBeVisible(VISIBLE)
        await submit
            .getByRole("button", { name: "确认提交", exact: true })
            .click()
        expect((await resubmitting).ok()).toBeTruthy()
        await expect(submit).toBeHidden(VISIBLE)
        const resubmitted = await fetchPurchaseCenter(caigouToken, snap.id)
        expect(resubmitted.id).toBe(snap.id)
        expect(resubmitted.purchase_no).toBe(snap.purchaseNo)
        expect(resubmitted.status).toBe("IN_APPROVAL")
        expect(resubmitted.current_submission_id).not.toBe(snap.submissionId)
        expect(resubmitted.current_submission_id).not.toBe(
            cancelled.current_submission_id,
        )
        expect(resubmitted.approval?.instance?.id).not.toBe(snap.instanceId)
        expect(resubmitted.approval?.instance?.current_round_no).toBe(1)
        expect(resubmitted.lines?.[0]?.unit_cost_gross).toMatch(/^99(?:\.0+)?$/)
        expect(resubmitted.lines?.[0]?.quantity).toBe(snap.quantity)
        expect(resubmitted.payable_summary ?? null).toBeNull()
        expect(
            (await listPurchasesBySalesOrder(caigouToken, salesOrderId)).map(
                (item) => item.id,
            ),
        ).toEqual([snap.id])
        page = await switchTo("caiwu")
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
            page.locator("#procurement-orders-detail-pay"),
        ).toHaveCount(0)
        await expect(
            page.locator("#procurement-orders-detail-change"),
        ).toBeVisible(VISIBLE)
        await expect(
            page.locator("#procurement-orders-detail-change"),
        ).toBeEnabled()
        await page.getByRole("tab", { name: /^票款/ }).click()
        await expect(page.getByText("应付未结")).toBeVisible(VISIBLE)
        await expect(
            page.getByText("尚未形成应付（需审批通过）。"),
        ).toHaveCount(0)

        page = await switchTo("fukuan")
        await page.goto("/workspace")
        await expect(
            page.getByRole("heading", { name: "我的工作台" }),
        ).toBeVisible(VISIBLE)
        await selectWorkspaceFamily(page, "finance")
        await expect(
            page.getByRole("button", {
                name: documentTaskPattern("供应商付款处理", [snap.purchaseNo]),
            }),
        ).toBeVisible(VISIBLE)

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
        await page.getByRole("tab", { name: /^采购/ }).click()
        await expect(
            page.getByTestId("sales-order-purchase-status"),
        ).toContainText("采购已覆盖")
        // 销售账号无采购单明细查看权限，面板仅显示计数提示，不显示已生效。
        await expect(
            page.getByTestId("sales-order-purchase-count-only"),
        ).toContainText("已创建 1 张采购单")
    } finally {
        await closeSession(session)
    }
})
