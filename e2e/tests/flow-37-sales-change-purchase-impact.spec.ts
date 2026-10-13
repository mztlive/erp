/**
 * 流程: [flow-37] 销售变更对采购的影响
 * 文档: docs/erp-phase-1.md §6.3「销售变更对采购的影响」、§6.5.1；
 *       docs/approval-workflow-contract.md §3 第 10 条
 * 账号: xiaoshou（销售与改单）、caigou（销售审批 / 供给分配 / 采购处理）、
 *       caiwu（变更财务复核）
 *
 * 验收：服务销售 4 次、采购 4 次仍在审批中时，销售减到 2 次。
 * 1. 审批中预判受影响采购；生效后采购自动退回草稿、采购负责人收到“采购依据更新”待办，
 *    销售详情逐张提示且不阻断其他供给。
 * 2. 采购按“修改原单”把数量改为 2 次并保存后，依据换到新销售版本、标记待提交审批，
 *    待办保持开放；重新提交审批后待办关闭，变更单显示“采购处理完成”。
 * 3. 再把销售加到 3 次只形成 1 次新缺口，不影响已有采购。
 */
import { randomUUID } from "node:crypto"

import { test, expect, type Page } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import {
    analyticsCommand,
    analyticsDate,
    uploadAnalyticsEvidence,
    type AnalyticsOrder,
} from "../helpers/financial-analytics"
import { openLoggedInWorkspace } from "../helpers/login"
import { expandSourcingEditor } from "../helpers/sourcing"
import { approveCurrentDocument, openWorkspaceTask } from "../helpers/ui"

const VISIBLE = { timeout: 20_000 }
const SERVICE_SKU = "SVC-INSTALL-01"

type SalesChangeDraft = {
    version: number
    working_copy_version: number
    business_remark: string | null
    lines: Array<{ goods?: { quantity: string; [key: string]: unknown } | null; [key: string]: unknown }>
}
type SalesChangeDetail = {
    id: string
    status: string
    procurement: {
        total: number
        open: number
        completed: boolean
        predicted: boolean
        items: Array<{
            purchase_order_id: string
            open: boolean
            awaiting_submission: boolean
            supplier_name?: string | null
            resolution?: string | null
        }>
    }
}
type SalesCoverage = {
    current_revision_id: string
    purchase_coverage: {
        remaining_quantity: string
        source_changed: boolean
        affected_purchase_count: number
        awaiting_submission_count: number
        source_updates: Array<{ purchase_order_id: string; blocks_payment: boolean; reasons: Array<{ kind: string }> }>
    }
}
type PurchaseCenter = {
    id: string
    purchase_no: string
    status: string
    lines: Array<{ line_id: string; line_type: string; quantity: string | null }>
    source_update: { affected: boolean; awaiting_submission: boolean; sales_revision_outdated: boolean } | null
}

async function write<T>(page: Page, token: string, path: string, data: unknown): Promise<T> {
    const response = await page.request.post(`${API_BASE}${path}`, {
        headers: { Authorization: `Bearer ${token}` },
        data,
        timeout: 60_000,
    })
    const body = (await response.json()) as { success: boolean; errorMessage?: string; data: T }
    expect(response.ok(), `POST ${path}: ${body.errorMessage ?? response.status()}`).toBe(true)
    expect(body.success, body.errorMessage).toBe(true)
    return body.data
}

/** 用生产入口建档客户并提交一张服务销售单，由采购审批生效。 */
async function createServiceSale(
    salesToken: string,
    adminToken: string,
    approverPage: Page,
    suffix: string,
): Promise<AnalyticsOrder> {
    const companies = await apiGet<{ items: Array<{ id: string; party_no: string }> }>(adminToken, "/admin/companies", {
        status: "active",
        page_size: 100,
    })
    const company = companies.items.find((item) => item.party_no === "FSY")
    expect(company, "销售必须引用已建档的我方公司主体").toBeTruthy()
    const customer = await analyticsCommand<{ customer_id: string }>(salesToken, "/admin/customer-profiles", {
        idempotency_key: `flow37-customer-${suffix}`,
        legal_name: `采购影响验收客户${suffix}`,
        unified_credit_code: null,
        default_payment_term_id: "POSTPAY_NET15",
        effective_from: analyticsDate(),
        change_reason: "E2E 销售变更对采购的影响",
    })
    const admins = await apiGet<Array<{ id: string; account: string }>>(adminToken, "/admin/admins")
    const sales = admins.find((item) => item.account === "xiaoshou")!
    const pool = await apiGet<{ items: Array<{ sku_id: string; sku_revision_id: string; sku_no: string }> }>(
        salesToken,
        "/admin/sellable-skus",
        { q: SERVICE_SKU, page_size: 100 },
    )
    const sku = pool.items.find((item) => item.sku_no === SERVICE_SKU)!
    const created = await analyticsCommand<AnalyticsOrder>(salesToken, "/admin/sales-orders", {
        order_no: `XS-F37-${suffix}`,
        business_type: "GOODS_SERVICE",
        contract_id: null,
        customer_id: customer.customer_id,
        idempotency_key: `flow37-order-${suffix}`,
        intent: "SUBMIT",
        evidence_file_asset_ids: [await uploadAnalyticsEvidence(salesToken)],
        draft: {
            editor_user_id: sales.id,
            requested_contract_revision_id: null,
            no_contract_terms: {
                settlement_party_id: company!.id,
                payment_term_code: "POSTPAY_NET15",
                payment_term_name: "货到 15 天",
                invoice_type: "增值税专用发票",
                tax_point: "0",
            },
            lines: [
                {
                    line_no: 1,
                    line_type: "GOODS_SERVICE",
                    sales_tax_rate: "0",
                    item_name_snapshot: "家电上门安装（标准台）",
                    spec_snapshot: "标准台",
                    unit_snapshot: "次",
                    goods: {
                        sku_id: sku.sku_id,
                        sku_revision_id: sku.sku_revision_id,
                        service_region: "北京",
                        fulfillment_due_at: Math.floor(Date.now() / 1000) + 30 * 86400,
                        quantity: "4",
                        base_unit_code: "CI",
                        unit_price_gross: "100.00",
                        pricing_mode: "MANUAL",
                    },
                },
            ],
        },
    })
    await openWorkspaceTask(approverPage, "销售单审批", created.order_no, "approval")
    await approveCurrentDocument(approverPage)
    const effective = await apiGet<AnalyticsOrder>(salesToken, `/admin/sales-orders/${created.id}`)
    expect(effective.commercial_status).toBe("EFFECTIVE")
    return effective
}

/** 供给分配创建采购并提交审批，不审批，返回采购单号与 ID。 */
async function allocateWithoutApproval(page: Page, order: AnalyticsOrder) {
    await openWorkspaceTask(page, "待供给分配", order.order_no, "procurement")
    await expandSourcingEditor(page, "家电上门安装")
    await page.locator("#procurement-orders-create-preview").click()
    const submitted = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            new URL(response.url()).pathname === "/admin/purchase-orders/from-sourcing",
    )
    await page.locator("#procurement-orders-create-preview-confirm").click({ force: true })
    const response = await submitted
    expect(response.ok(), await response.text()).toBe(true)
    const result = (await response.json()).data as { orders: Array<{ purchase_order_id: string; purchase_no: string }> }
    expect(result.orders).toHaveLength(1)
    return result.orders[0]!
}

/** 销售发起改单，把唯一商品行数量改为目标值并提交审批，经采购与财务审批生效。 */
async function changeQuantity(
    salesPage: Page,
    salesToken: string,
    approvers: { procurement: Page; finance: Page },
    order: { id: string; order_no: string },
    baseRevisionNo: number,
    quantity: string,
    onSubmitted?: (changeId: string) => Promise<void>,
) {
    const change = await write<{ id: string }>(salesPage, salesToken, "/admin/sales-change-orders", {
        sales_order_id: order.id,
        expected_base_revision_no: baseRevisionNo,
        change_type: "QUANTITY",
        reason: `E2E 客户把服务数量调整为 ${quantity} 次`,
        idempotency_key: randomUUID(),
    })
    const draft = await apiGet<SalesChangeDraft>(salesToken, `/admin/sales-change-orders/${change.id}/draft`)
    const lines = draft.lines.map((line) => (line.goods ? { ...line, goods: { ...line.goods, quantity } } : line))
    const saved = await write<SalesChangeDraft>(salesPage, salesToken, `/admin/sales-change-orders/${change.id}/draft`, {
        expected_version: draft.version,
        expected_working_copy_version: draft.working_copy_version,
        reason: `客户确认服务数量调整为 ${quantity} 次`,
        business_remark: draft.business_remark,
        lines,
    })
    await write(salesPage, salesToken, `/admin/sales-change-orders/${change.id}/submit-impact`, {
        version: saved.version,
        idempotency_key: randomUUID(),
    })
    if (onSubmitted) await onSubmitted(change.id)
    await openWorkspaceTask(approvers.procurement, /销售变更单审批/, order.order_no, "approval")
    await approveCurrentDocument(approvers.procurement)
    await openWorkspaceTask(approvers.finance, /销售变更单审批/, order.order_no, "approval")
    await approveCurrentDocument(approvers.finance)
    return change.id
}

test("销售减量后审批中采购退回草稿并派待办，修改原单后完成；加量不影响已有采购", async ({ browser }) => {
    test.setTimeout(12 * 60 * 1000)
    const suffix = Date.now().toString(36)
    const procurement = await openLoggedInWorkspace(browser, "caigou")
    const finance = await openLoggedInWorkspace(browser, "caiwu")
    const adminToken = await apiToken("admin")
    const salesToken = await apiToken("xiaoshou")
    const procurementToken = await apiToken("caigou")

    const order = await createServiceSale(salesToken, adminToken, procurement.page, suffix)
    const purchase = await allocateWithoutApproval(procurement.page, order)
    const pending = await apiGet<PurchaseCenter>(procurementToken, `/admin/purchase-orders/${purchase.purchase_order_id}`)
    expect(pending.status).not.toBe("DRAFT")

    // 1. 审批中预判，生效后自动退回草稿、派待办、逐张提示。
    const changeId = await changeQuantity(
        procurement.page,
        salesToken,
        { procurement: procurement.page, finance: finance.page },
        order,
        1,
        "2",
        async (id) => {
            const predicted = await apiGet<SalesChangeDetail>(procurementToken, `/admin/sales-change-orders/${id}`)
            expect(predicted.procurement).toMatchObject({ predicted: true, total: 1, completed: false })
            expect(predicted.procurement.items[0]!.purchase_order_id).toBe(purchase.purchase_order_id)
        },
    )
    const withdrawn = await apiGet<PurchaseCenter>(procurementToken, `/admin/purchase-orders/${purchase.purchase_order_id}`)
    expect(withdrawn.status).toBe("DRAFT")
    expect(withdrawn.source_update).toMatchObject({ affected: true, sales_revision_outdated: true })
    const affected = await apiGet<SalesCoverage>(procurementToken, `/admin/sales-orders/${order.id}`)
    expect(affected.purchase_coverage).toMatchObject({ source_changed: true, affected_purchase_count: 1 })
    expect(affected.purchase_coverage.source_updates[0]!).toMatchObject({
        purchase_order_id: purchase.purchase_order_id,
        blocks_payment: true,
    })
    expect(affected.purchase_coverage.source_updates[0]!.reasons[0]!.kind).toBe("QUANTITY_REDUCED")
    const salesOnly = await apiGet<SalesCoverage>(salesToken, `/admin/sales-orders/${order.id}`)
    expect(salesOnly.purchase_coverage.affected_purchase_count).toBe(1)
    expect(salesOnly.purchase_coverage.source_updates, "销售岗位无采购列表权限时只给张数").toHaveLength(0)
    const effectiveChange = await apiGet<SalesChangeDetail>(procurementToken, `/admin/sales-change-orders/${changeId}`)
    expect(effectiveChange.procurement).toMatchObject({ predicted: false, total: 1, open: 1, completed: false })

    await openWorkspaceTask(procurement.page, /采购依据更新|依据待更新/, undefined, "procurement")
    await procurement.page.goto(`/procurement/orders/${purchase.purchase_order_id}`)
    const notice = procurement.page.locator("#procurement-orders-detail-source-update")
    await expect(notice).toContainText("采购依据待更新", VISIBLE)
    await expect(notice).toContainText("销售数量 4 → 2")
    await expect(procurement.page.locator("#procurement-orders-detail-source-update-maintain")).toBeEnabled()

    // 2. 修改原单：数量改为 2 次保存，依据换到新版本、待提交审批；提交后待办与记录关闭。
    await procurement.page.locator("#procurement-orders-detail-source-update-edit").click()
    const itemLine = withdrawn.lines.find((line) => line.line_type === "ITEM_SERVICE")!
    const quantityInput = procurement.page.locator(
        `#procurement-orders-detail-edit-row-${itemLine.line_id.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")}-quantity`,
    )
    await expect(quantityInput).toBeVisible(VISIBLE)
    await quantityInput.fill("2")
    const saved = procurement.page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            new URL(response.url()).pathname === `/admin/purchase-orders/${purchase.purchase_order_id}/draft`,
    )
    await procurement.page.locator(`#procurement-orders-detail-edit-save-${purchase.purchase_order_id}`).click()
    const savedResponse = await saved
    expect(savedResponse.ok(), await savedResponse.text()).toBe(true)
    const awaiting = await apiGet<SalesChangeDetail>(procurementToken, `/admin/sales-change-orders/${changeId}`)
    expect(awaiting.procurement).toMatchObject({ total: 1, open: 1, completed: false })
    expect(awaiting.procurement.items[0]!.awaiting_submission).toBe(true)
    expect(awaiting.procurement.items[0]!.supplier_name, "变更单逐张展示供应商").toBeTruthy()
    const rebasedDraft = await apiGet<PurchaseCenter>(procurementToken, `/admin/purchase-orders/${purchase.purchase_order_id}`)
    expect(rebasedDraft.source_update).toMatchObject({ affected: false, awaiting_submission: true })
    const awaitingSales = await apiGet<SalesCoverage>(procurementToken, `/admin/sales-orders/${order.id}`)
    expect(awaitingSales.purchase_coverage).toMatchObject({
        source_changed: false,
        affected_purchase_count: 0,
        awaiting_submission_count: 1,
    })
    await expect(notice).toContainText("待提交审批", VISIBLE)

    const resubmitted = procurement.page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            new URL(response.url()).pathname === `/admin/purchase-orders/${purchase.purchase_order_id}/submit`,
    )
    await procurement.page.locator(`#procurement-orders-detail-edit-submit-${purchase.purchase_order_id}`).click()
    await procurement.page.locator("#procurement-orders-detail-submit-confirm-confirm").click()
    const resubmittedResponse = await resubmitted
    expect(resubmittedResponse.ok(), await resubmittedResponse.text()).toBe(true)
    await expect
        .poll(async () => (await apiGet<SalesChangeDetail>(procurementToken, `/admin/sales-change-orders/${changeId}`)).procurement.completed, VISIBLE)
        .toBe(true)
    const rebased = await apiGet<PurchaseCenter>(procurementToken, `/admin/purchase-orders/${purchase.purchase_order_id}`)
    expect(rebased.source_update).toBeNull()
    const resolved = await apiGet<SalesCoverage>(procurementToken, `/admin/sales-orders/${order.id}`)
    expect(resolved.purchase_coverage).toMatchObject({
        source_changed: false,
        affected_purchase_count: 0,
        awaiting_submission_count: 0,
    })

    // 3. 加量到 3 次：已有采购不受影响，只形成 1 次新缺口。
    await changeQuantity(
        procurement.page,
        salesToken,
        { procurement: procurement.page, finance: finance.page },
        order,
        2,
        "3",
    )
    const increased = await apiGet<SalesCoverage>(procurementToken, `/admin/sales-orders/${order.id}`)
    expect(increased.purchase_coverage).toMatchObject({ source_changed: false, affected_purchase_count: 0 })
    expect(increased.purchase_coverage.remaining_quantity).toMatch(/^1(?:\.0+)?$/)
    const untouched = await apiGet<PurchaseCenter>(procurementToken, `/admin/purchase-orders/${purchase.purchase_order_id}`)
    expect(untouched.source_update).toMatchObject({ affected: false, sales_revision_outdated: true })
})
