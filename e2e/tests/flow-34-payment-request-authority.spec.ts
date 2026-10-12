/**
 * [flow-34] 采购付款申请的正式主体、任务材料、账户失效和关闭后冲正边界。
 * 来源、审批、账户修订、付款及冲正均使用正式命令；不写库伪造批准或占用。
 * 合同：docs/purchase-payment-request-contract.md PPR-03/04/10/11/14。
 */
import { randomUUID } from "node:crypto"
import { readFileSync } from "node:fs"

import { test, expect, type Page } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import {
    analyticsCommand,
    analyticsDate,
    createAnalyticsPurchase,
    prepareAnalyticsOrders,
    uploadAnalyticsEvidence,
    type AnalyticsOrder,
    type AnalyticsPurchase,
} from "../helpers/financial-analytics"
import { openLoggedInWorkspace } from "../helpers/login"
import { expandSourcingEditor } from "../helpers/sourcing"
import {
    BANK_RECEIPT_FILE,
    purchasePaymentRequestCommand,
    type PaymentRequestCandidate,
    type PurchasePaymentRequestResult,
} from "../helpers/payments"
import { approveCurrentDocument, chooseOption, openWorkspaceTask } from "../helpers/ui"

const ROOT = "/admin/purchase-payment-requests"
const VISIBLE = { timeout: 20_000 }
const SOURCE_PERMISSION = /^(?:sales|purchase_order|contract|file_asset)(?:_|:)/

type Detail = PurchasePaymentRequestResult & {
    supplier_id: string
    payer_party_id: string
    payee_account: { account_id: string; version: number }
    work_item_id: string | null
    purchase_previews: Array<{ purchase_order_id: string; purchase_no: string; revision_id: string }>
    sales_previews: Array<{ sales_order_id: string; revision_id: string }>
    attachments: Array<{ file_asset_id: string; file_name: string }>
    payments: Array<{ payment_id: string; amount: string }>
    current_lines: Array<{
        payable_account_id: string; current_gross_total: string; current_settled_total: string
        current_reserved_total: string; current_open_total: string; current_available_total: string; source_changed: boolean
        payable_entries: Array<{ id: string; open_total: string }>
    }>
}
type SalesHead = {
    id: string; order_no: string; commercial_status: string; current_revision_id: string; settlement_party_id: string
    revisions: Array<{ id: string; revision_no: number; net_amount: string }>
    purchase_coverage: { source_changed: boolean; outdated_purchase_quantity: string; outdated_purchase_count: number }
}
type SalesChangeDraft = {
    version: number; working_copy_version: number; business_remark: string | null
    lines: Array<{ goods?: { unit_price_gross: string; [key: string]: unknown } | null; [key: string]: unknown }>
}
type PurchaseSource = {
    id: string; current_revision_id: string; sales_order_id: string
    source_sales_order: { revision_id: string; revision_no: number }
}
type Account = { id: string; account: string; role_ids: string[] }
type Role = { id: string; permissions: string[] }
type SupplierDetail = {
    id: string
    party_version: number
    version: number
    legal_name: string
    short_name: string | null
    unified_credit_code: string | null
    maintainer_user_id: string
    bank_accounts: Array<{ id: string; version: number; status: string; is_default: boolean }>
    capability_codes: string[]
    capabilities: Array<{ id: string; capability_code: string }>
    qualifications: Array<{
        qualification_type: string; certificate_no: string; issuer: string | null
        valid_from: string | null; valid_to: string | null; attachment_id: string | null
        capability_ids: string[]; status: string
    }>
    current_profile: {
        settlement_mode: string; reconciliation_cycle: string; payment_term_snapshot: string
        business_category: string | null; invoice_type: string; invoice_tax_rate: string | null
        invoice_tax_rates?: string[]; signing_entity_party_id: string; payment_entity_party_id: string
    }
}
type PaymentCommand = {
    payment_request_id: string
    expected_request_version: number
    idempotency_key: string
    payment: { payment_no: string; amount: string; bank_receipt_asset_id: string }
    [key: string]: unknown
}

const detail = (token: string, id: string) => apiGet<Detail>(token, `${ROOT}/${id}`)
const candidates = (token: string) => apiGet<PaymentRequestCandidate[]>(token, `${ROOT}/candidates`)
const segment = (id: string) => id.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")

async function write<T>(page: Page, token: string, method: string, path: string, data?: unknown): Promise<T> {
    const response = await page.request.fetch(`${API_BASE}${path}`, {
        method, headers: { Authorization: `Bearer ${token}` }, data, timeout: 60_000,
    })
    const body = await response.json() as { success: boolean; errorMessage?: string; data: T }
    expect(response.ok(), `${method} ${path}: ${body.errorMessage ?? response.status()}`).toBe(true)
    expect(body.success, body.errorMessage).toBe(true)
    return body.data
}

async function expectDenied(page: Page, token: string, path: string, message?: RegExp) {
    const response = await page.request.get(`${API_BASE}${path}`, { headers: { Authorization: `Bearer ${token}` } })
    const body = await response.json() as { success: boolean; errorMessage?: string }
    expect([403, 404, 409, 422], body.errorMessage).toContain(response.status())
    expect(body.success).toBe(false)
    if (message) expect(body.errorMessage).toMatch(message)
}

function draft(rows: PaymentRequestCandidate[], attachmentIds: string[] = [], requestAmount?: string) {
    const first = rows[0]!
    const account = first.bank_accounts.find((item) => item.is_default) ?? first.bank_accounts[0]!
    expect(account, "正式有效收款账户是申请必填依据").toBeTruthy()
    return { data: {
        supplier_id: first.supplier_id, payee_bank_account_id: account.id,
        desired_payment_date: analyticsDate(), reason: "采购核对来源主体、金额和付款账户后申请",
        attachment_ids: attachmentIds,
        lines: rows.map((row) => ({ payable_account_id: row.payable_account_id, amount: requestAmount ?? row.available_total })),
    } }
}

async function createSubmitted(page: Page, rows: PaymentRequestCandidate[], attachments: string[] = [], requestAmount?: string) {
    const saved = await purchasePaymentRequestCommand<PurchasePaymentRequestResult>(page, "caigou", "POST", ROOT, draft(rows, attachments, requestAmount))
    const submitted = await purchasePaymentRequestCommand<PurchasePaymentRequestResult>(page, "caigou", "POST", `${ROOT}/${saved.id}/submit`, {
        expected_version: saved.version, idempotency_key: randomUUID(),
    })
    // 写命令只返回领域实体；冻结完整预览须由采购原本具备来源资格时正式读取。
    const view = await detail(await apiToken("caigou"), submitted.id)
    expect(view.id).toBe(submitted.id)
    expect(view.version).toBe(submitted.version)
    expect(view.status).toBe("in_approval")
    expect(view.purchase_previews.map((item) => item.purchase_order_id).sort()).toEqual(rows.map((row) => row.purchase_order_id).sort())
    expect(view.lines.map((line) => line.payable_account_id).sort()).toEqual(rows.map((row) => row.payable_account_id).sort())
    return view
}

/** 替换全部授予来源；保存并恢复每个账号的原始角色集合，避免影响同 shard 后续流程。 */
async function taskOnlyRole(page: Page, adminToken: string, identity: string, suffix: string) {
    const accounts = await apiGet<Account[]>(adminToken, "/admin/admins")
    const roles = await apiGet<Role[]>(adminToken, "/admin/roles")
    const account = accounts.find((item) => item.account === identity)!
    expect(account, `固定岗位 ${identity}`).toBeTruthy()
    const original = [...account.role_ids]
    const permissions = [...new Set(roles.filter((role) => original.includes(role.id)).flatMap((role) => role.permissions))]
        .filter((permission) => permission !== "*" && !SOURCE_PERMISSION.test(permission))
    expect(permissions).toContain("work_item:detail")
    const role = await write<string>(page, adminToken, "POST", "/admin/roles", {
        name: `E2E 付款任务受限 ${identity} ${suffix}`, permissions,
    })
    const restore = async () => {
        await write(page, adminToken, "PUT", `/admin/admins/${account.id}/role`, { role_ids: original })
        await write(page, adminToken, "DELETE", `/admin/roles/${role}`)
    }
    try {
        await write(page, adminToken, "PUT", `/admin/admins/${account.id}/role`, { role_ids: [role] })
        const restricted = (await apiGet<Account[]>(adminToken, "/admin/admins")).find((item) => item.id === account.id)!
        expect(restricted.role_ids).toEqual([role])
        const actual = (await apiGet<Role[]>(adminToken, "/admin/roles")).find((item) => item.id === role)!
        expect(actual.permissions.some((permission) => SOURCE_PERMISSION.test(permission) || permission === "*")).toBe(false)
    } catch (error) {
        await restore()
        throw error
    }
    return restore
}

async function expectTaskMaterials(
    page: Page, token: string, request: Detail, sourceEvidence: string, unrelatedEvidence: string,
    unrelatedRequest: Detail, family: "approval" | "finance",
) {
    await openWorkspaceTask(page, family === "approval" ? /付款申请/ : /供应商付款处理|待付款申请/, request.request_no, family)
    const workItemId = new URL(page.url()).searchParams.get("currentWorkItemId")!
    expect(workItemId, "材料读取必须携带正式当前任务身份").toBeTruthy()
    const context = await apiGet<Detail>(token, `/admin/work-items/${workItemId}/purchase-payment-request`)
    expect(context.id).toBe(request.id)
    expect(context.purchase_previews).toEqual(request.purchase_previews)
    expect(context.sales_previews).toEqual(request.sales_previews)
    expect(context.attachments).toEqual(request.attachments)
    expect(context.purchase_previews).toHaveLength(2)
    expect(context.sales_previews).toHaveLength(2)
    for (const purchase of context.purchase_previews) {
        await expectDenied(page, token, `/admin/purchase-orders/${purchase.purchase_order_id}`)
        const preview = page.locator(`#ppr-purchase-preview-${segment(purchase.purchase_order_id)}`)
        if (family === "approval") {
            await preview.click()
            const paper = page.getByRole("dialog", { name: "采购单完整预览" })
            await expect(paper).toContainText(purchase.purchase_no)
            await expect(paper).toContainText("采购成本")
            await page.locator("#ppr-paper-close").click()
        } else {
            await expect(preview).toHaveCount(0)
        }
    }
    for (const sales of context.sales_previews) {
        await expectDenied(page, token, `/admin/sales-orders/${sales.sales_order_id}`)
        const preview = page.locator(`#ppr-sales-preview-${segment(sales.revision_id)}`)
        if (family === "approval") {
            await preview.click()
            await expect(page.getByRole("dialog", { name: "销售单完整预览" })).toContainText("客户成交金额")
            await page.locator("#ppr-paper-close").click()
        } else {
            await expect(preview).toHaveCount(0)
        }
    }
    await expectDenied(page, token, `/admin/file-assets/${sourceEvidence}`)
    const materialPath = `${ROOT}/${request.id}/materials/${sourceEvidence}/download?work_item_id=${workItemId}`
    const read = await page.request.get(`${API_BASE}${materialPath}`, { headers: { Authorization: `Bearer ${token}` } })
    expect(read.ok(), await read.text()).toBe(true)
    expect(read.headers()["cache-control"]).toBe("private, no-store")
    expect((await read.body()).length).toBeGreaterThan(0)
    const attachment = page.locator(`#ppr-attachment-${segment(sourceEvidence)}-download`)
    if (family === "approval") {
        const [download] = await Promise.all([
            page.waitForEvent("download"),
            attachment.click(),
        ])
        expect(await download.failure()).toBeNull()
        const path = await download.path()
        expect(path).not.toBeNull()
        expect(readFileSync(path!)).toEqual(await read.body())
    } else {
        // 出纳执行面只展示付款信息；任务材料读取资格仍由上述专用接口复验。
        await expect(attachment).toHaveCount(0)
        await expect(page.locator("#ppr-pay-amount")).toBeVisible(VISIBLE)
    }
    // 即使当前审批人另有另一申请任务，也不能借本任务展开其来源或附件。
    await expectDenied(page, token, `${ROOT}/${request.id}/materials/${unrelatedEvidence}/download?work_item_id=${workItemId}`)
    await expectDenied(page, token, `${ROOT}/${unrelatedRequest.id}/materials/${unrelatedEvidence}/download?work_item_id=${workItemId}`)
    return workItemId
}

/** 浏览器发出正式 multipart 付款命令，返回可用于后续状态准入拒绝的完整命令依据。 */
async function payOne(page: Page, amount = "1.00", succeeds = true) {
    await page.locator("#ppr-pay-amount").fill(amount)
    await page.locator("#ppr-pay-bank-receipt-input").setInputFiles(BANK_RECEIPT_FILE)
    await page.locator("#ppr-pay-submit").click()
    const responsePending = page.waitForResponse((response) => response.request().method() === "POST" && /\/payments\/commit$/.test(new URL(response.url()).pathname), { timeout: 60_000 })
    await page.locator("#ppr-pay-confirm-confirm").click()
    const response = await responsePending
    const body = await response.json() as { success: boolean; errorMessage?: string; data: { id: string; amount: string; status: string } }
    expect(response.ok(), body.errorMessage).toBe(succeeds)
    expect(body.success, body.errorMessage).toBe(succeeds)
    const text = response.request().postDataBuffer()!.toString("utf8")
    const encoded = /name="command"\r\n(?:Content-Type:[^\r]+\r\n)?\r\n([^\r]+)/.exec(text)
    expect(encoded, "浏览器付款必须包含完整正式命令").not.toBeNull()
    const command = JSON.parse(encoded![1]!) as PaymentCommand
    expect(command.payment.amount).toBe(amount)
    if (!succeeds) {
        expect([409, 422], body.errorMessage).toContain(response.status())
        expect(body.errorMessage).toMatch(/账户|账号|版本|变化|停用/)
    }
    return { result: body.data, command }
}

async function rawPayment(page: Page, token: string, request: Detail, original: PaymentCommand) {
    const reference = "pending-file:authority-bank-receipt"
    const command = {
        ...original, expected_request_version: request.version, idempotency_key: randomUUID(),
        payment: { ...original.payment, payment_no: `E2E-AUTH-${randomUUID()}`, bank_receipt_asset_id: reference },
    }
    return page.request.post(`${API_BASE}${ROOT}/${request.id}/payments/commit`, {
        headers: { Authorization: `Bearer ${token}` },
        multipart: { command: JSON.stringify(command), [reference]: BANK_RECEIPT_FILE }, timeout: 60_000,
    })
}

/** 正式资产 ID 不能借付款命令升级为当前任务材料；失败不产生账务或授权关系。 */
async function rejectUnrelatedReceipt(page: Page, token: string, request: Detail, workItemId: string, assetId: string) {
    const task = await apiGet<{ task_version: string }>(token, `/admin/work-items/${workItemId}`)
    const entry = request.current_lines[0]!.payable_entries.find((row) => !/^0(?:\.0+)?$/.test(row.open_total))!
    expect(entry, "正式申请视图提供当前可核销分录").toBeTruthy()
    const command = {
        payment_request_id: request.id, expected_request_version: request.version,
        work_item_id: workItemId, expected_task_version: task.task_version,
        additional_work_items: [], expected_payee_bank_account_id: request.payee_account.account_id,
        expected_payee_bank_account_version: request.payee_account.version,
        payment: { payment_no: `E2E-DENIED-RECEIPT-${randomUUID()}`, supplier_id: request.supplier_id,
            paid_at: Math.floor(Date.now() / 1000), amount: "1.00", bank_reference: null, bank_receipt_asset_id: assetId },
        allocations: [{ payable_entry_id: entry.id, allocated_amount: "1.00" }], idempotency_key: randomUUID(),
    }
    await expectDenied(page, token, `/admin/file-assets/${assetId}`)
    const material = `${ROOT}/${request.id}/materials/${assetId}/download?work_item_id=${workItemId}`
    await expectDenied(page, token, material)
    const denied = await page.request.post(`${API_BASE}${ROOT}/${request.id}/payments/commit`, {
        headers: { Authorization: `Bearer ${token}` }, multipart: { command: JSON.stringify(command) }, timeout: 60_000,
    })
    const body = await denied.json() as { success: boolean; errorMessage?: string }
    expect(denied.status(), body.errorMessage).toBe(400)
    expect(body.success).toBe(false)
    expect(body.errorMessage).toMatch(/上传本次银行回单/)
    const unchanged = await detail(await apiToken("caigou"), request.id)
    expect(unchanged.version).toBe(request.version)
    expect(unchanged.lines).toEqual(request.lines)
    expect(unchanged.payments).toEqual(request.payments)
    expect(unchanged.current_lines).toEqual(request.current_lines)
    await expectDenied(page, token, material)
}

async function changeBank(page: Page, token: string, supplierId: string, bank: boolean, suffix: string) {
    const source = await apiGet<SupplierDetail>(token, `/admin/suppliers/${supplierId}`)
    const profile = source.current_profile
    const qualifications = source.qualifications.filter((item) => item.status === "active").map((item) => ({
        qualification_type: item.qualification_type, certificate_no: item.certificate_no,
        issuer: item.issuer, valid_from: item.valid_from, valid_to: item.valid_to, attachment_id: item.attachment_id,
        capability_codes: item.capability_ids.map((id) => source.capabilities.find((capability) => capability.id === id)!.capability_code),
    }))
    await write(page, token, "PUT", `/admin/supplier-profiles/${supplierId}`, {
        idempotency_key: randomUUID(), expected_party_version: source.party_version, expected_supplier_version: source.version,
        legal_name: source.legal_name, short_name: source.short_name, unified_credit_code: source.unified_credit_code,
        ...profile, bank_account: bank ? { bank_name: "E2E 正式开户行", account_number: `6222${Date.now()}${suffix.replace(/\D/g, "").slice(-2)}` } : null,
        clear_bank_account: !bank, maintainer_user_id: source.maintainer_user_id,
        capability_codes: source.capability_codes, capability_owners: [], qualifications,
        effective_from: analyticsDate(), change_reason: "E2E 收款账户生命周期校验",
    })
    return apiGet<SupplierDetail>(token, `/admin/suppliers/${supplierId}`)
}

async function approveRejectedByAccount(page: Page) {
    await page.getByRole("button", { name: /^(通过|同意审批)$/ }).first().click()
    const dialog = page.getByRole("dialog", { name: "确认通过" })
    const responsePending = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === "/admin/approval-decisions", { timeout: 60_000 })
    await dialog.locator('[id$="-decision-dialog-submit"]').click()
    const response = await responsePending
    const body = await response.json() as { success: boolean; errorMessage?: string }
    expect(response.ok(), body.errorMessage).toBe(false)
    expect(body.success).toBe(false)
    expect([409, 422], body.errorMessage).toContain(response.status())
    expect(body.errorMessage).toMatch(/账户|账号|版本|停用|变化/)
}

test("正式付款主体分组、受限任务材料、账户失效和已关闭申请冲正不恢复授权", async ({ browser }) => {
    test.setTimeout(18 * 60 * 1000)
    const suffix = Date.now().toString(36)
    const procurement = await openLoggedInWorkspace(browser, "caigou")
    const adminToken = await apiToken("admin")
    const salesToken = await apiToken("xiaoshou")
    const procurementToken = await apiToken("caigou")
    const fixture = await prepareAnalyticsOrders(salesToken, adminToken, procurement.page, `AUTH${suffix}`)
    const purchases: AnalyticsPurchase[] = []
    for (const order of fixture.orders) purchases.push(await createAnalyticsPurchase(order, procurement.page, procurementToken))
    const firstRows = (await candidates(procurementToken)).filter((row) => purchases.some((purchase) => purchase.purchaseId === row.purchase_order_id))
    expect(firstRows).toHaveLength(2)

    // PPR-03：第二付款主体必须来自另一张正式销售及其采购，不改候选或数据库字段。
    const companies = await apiGet<{ items: Array<{ id: string; legal_name: string }> }>(adminToken, "/admin/companies", { status: "active", page_size: 100 })
    const secondCompany = companies.items.find((company) => company.id !== firstRows[0]!.payer_party_id)!
    expect(secondCompany, "种子包含多家已建档我方公司").toBeTruthy()
    const skuPool = await apiGet<{ items: Array<{ sku_id: string; sku_revision_id: string; sku_no: string }> }>(salesToken, "/admin/sellable-skus", { q: "SVC-INSTALL-01", page_size: 100 })
    const sku = skuPool.items.find((item) => item.sku_no === "SVC-INSTALL-01")!
    const unrelatedEvidence = await uploadAnalyticsEvidence(salesToken)
    const created = await analyticsCommand<AnalyticsOrder>(salesToken, "/admin/sales-orders", {
        order_no: `XS-AUTH-${suffix}-C`, business_type: "GOODS_SERVICE", contract_id: null,
        customer_id: fixture.customerId, idempotency_key: randomUUID(), intent: "SUBMIT", evidence_file_asset_ids: [unrelatedEvidence],
        draft: { editor_user_id: fixture.orders[0].owner_user_id, requested_contract_revision_id: null,
            no_contract_terms: { settlement_party_id: secondCompany.id, payment_term_code: "POSTPAY_NET15", payment_term_name: "货到 15 天", invoice_type: "增值税专用发票", tax_point: "0" },
            lines: [{ line_no: 1, line_type: "GOODS_SERVICE", sales_tax_rate: "0", item_name_snapshot: "家电上门安装（标准台）", spec_snapshot: "标准台", unit_snapshot: "次",
                goods: { sku_id: sku.sku_id, sku_revision_id: sku.sku_revision_id, service_region: "北京", fulfillment_due_at: Math.floor(Date.now() / 1000) + 30 * 86400, quantity: "2", base_unit_code: "CI", unit_price_gross: "100.00", pricing_mode: "MANUAL" } }],
        },
    })
    await openWorkspaceTask(procurement.page, "销售单审批", created.order_no, "approval")
    await approveCurrentDocument(procurement.page)
    const effective = await apiGet<AnalyticsOrder>(salesToken, `/admin/sales-orders/${created.id}`)
    expect(effective.commercial_status).toBe("EFFECTIVE")
    const secondPurchase = await createAnalyticsPurchase(effective, procurement.page, procurementToken)
    const secondRow = (await candidates(procurementToken)).find((row) => row.purchase_order_id === secondPurchase.purchaseId)!
    expect(secondRow.supplier_id).toBe(firstRows[0]!.supplier_id)
    expect(secondRow.payer_party_id).toBe(secondCompany.id)
    expect(secondRow.payer_party_id).not.toBe(firstRows[0]!.payer_party_id)
    const cross = await procurement.page.request.post(`${API_BASE}${ROOT}`, {
        headers: { Authorization: `Bearer ${procurementToken}` }, data: draft([firstRows[0]!, secondRow]),
    })
    const crossBody = await cross.json() as { success: boolean; errorMessage?: string }
    expect(cross.ok(), "客户端手工跨主体参数不得合并").toBe(false)
    expect(crossBody.success).toBe(false)
    expect(crossBody.errorMessage).toMatch(/主体|供应商|分别|一致/)

    // 同一正式我方主体下，再从种子狮峰供给创建一张真实采购，验证跨供应商不能借主体一致合并。
    const physicalPool = await apiGet<{ items: Array<{ sku_id: string; sku_revision_id: string; sku_no: string }> }>(salesToken, "/admin/sellable-skus", { q: "TEA-SF-LJ-250", page_size: 100 })
    const physicalSku = physicalPool.items.find((item) => item.sku_no === "TEA-SF-LJ-250")!
    expect(physicalSku, "固定狮峰供给提供另一供应商的正式采购来源").toBeTruthy()
    const physicalSale = await analyticsCommand<AnalyticsOrder>(salesToken, "/admin/sales-orders", {
        order_no: `XS-AUTH-${suffix}-D`, business_type: "GOODS_SERVICE", contract_id: null,
        customer_id: fixture.customerId, idempotency_key: randomUUID(), intent: "SUBMIT", evidence_file_asset_ids: [fixture.evidenceId],
        draft: { editor_user_id: fixture.orders[0].owner_user_id, requested_contract_revision_id: null,
            no_contract_terms: { settlement_party_id: firstRows[0]!.payer_party_id, payment_term_code: "POSTPAY_NET15", payment_term_name: "货到 15 天", invoice_type: "增值税专用发票", tax_point: "0" },
            lines: [{ line_no: 1, line_type: "GOODS_SERVICE", sales_tax_rate: "0", item_name_snapshot: "狮峰明前龙井礼盒 250g", spec_snapshot: "250g/盒", unit_snapshot: "盒",
                goods: { sku_id: physicalSku.sku_id, sku_revision_id: physicalSku.sku_revision_id, fulfillment_due_at: Math.floor(Date.now() / 1000) + 30 * 86400, quantity: "6", base_unit_code: "HE", unit_price_gross: "1000.00", pricing_mode: "MANUAL" } }],
        },
    })
    await openWorkspaceTask(procurement.page, "销售单审批", physicalSale.order_no, "approval")
    await approveCurrentDocument(procurement.page)
    await openWorkspaceTask(procurement.page, "待供给分配", physicalSale.order_no, "procurement")
    await expandSourcingEditor(procurement.page, "狮峰明前龙井礼盒")
    const warehouse = procurement.page.getByRole("combobox", { name: "仓库", exact: true })
    if (await warehouse.count()) await chooseOption(procurement.page, warehouse.first(), /北京通州仓|BJ-TZ-01/, "BJ-TZ-01")
    await procurement.page.locator("#procurement-orders-create-preview").click()
    const physicalPending = procurement.page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === "/admin/purchase-orders/from-sourcing")
    await procurement.page.locator("#procurement-orders-create-preview-confirm").click()
    const physicalResponse = await physicalPending
    expect(physicalResponse.ok(), await physicalResponse.text()).toBe(true)
    const physicalResult = (await physicalResponse.json()).data as { orders: Array<{ purchase_order_id: string; purchase_no: string }> }
    expect(physicalResult.orders).toHaveLength(1)
    const physicalPurchase = physicalResult.orders[0]!
    const purchaseApprover = await openLoggedInWorkspace(browser, "caigoushenpi")
    await openWorkspaceTask(purchaseApprover.page, "采购单审批", physicalPurchase.purchase_no, "approval")
    await approveCurrentDocument(purchaseApprover.page)
    const supplierRowsBefore = await candidates(procurementToken)
    const otherSupplier = supplierRowsBefore.find((row) => row.purchase_order_id === physicalPurchase.purchase_order_id)!
    expect(otherSupplier.payer_party_id).toBe(firstRows[0]!.payer_party_id)
    expect(otherSupplier.supplier_id).not.toBe(firstRows[0]!.supplier_id)
    const crossSupplier = await procurement.page.request.post(`${API_BASE}${ROOT}`, {
        headers: { Authorization: `Bearer ${procurementToken}` }, data: draft([firstRows[0]!, otherSupplier]),
    })
    const supplierDenied = await crossSupplier.json() as { success: boolean; errorMessage?: string }
    expect(crossSupplier.ok(), "跨供应商来源即使我方主体相同也必须拒绝").toBe(false)
    expect(supplierDenied.success).toBe(false)
    expect(supplierDenied.errorMessage).toMatch(/供应商|来源|一致/)
    const wrongAccountDraft = draft([firstRows[0]!])
    const foreignAccount = otherSupplier.bank_accounts.find((item) => item.is_default) ?? otherSupplier.bank_accounts[0]!
    expect(foreignAccount, "另一供应商具有正式有效收款账户").toBeTruthy()
    const crossAccount = await procurement.page.request.post(`${API_BASE}${ROOT}`, {
        headers: { Authorization: `Bearer ${procurementToken}` },
        data: { data: { ...wrongAccountDraft.data, payee_bank_account_id: foreignAccount.id } },
    })
    const accountDenied = await crossAccount.json() as { success: boolean; errorMessage?: string }
    expect(crossAccount.ok(), "客户端改为另一供应商账户必须拒绝").toBe(false)
    expect(accountDenied.success).toBe(false)
    expect(accountDenied.errorMessage).toMatch(/账户|账号|供应商|收款/)
    const supplierRowsAfter = await candidates(procurementToken)
    for (const id of [firstRows[0]!.payable_account_id, otherSupplier.payable_account_id]) {
        const before = supplierRowsBefore.find((row) => row.payable_account_id === id)!
        const after = supplierRowsAfter.find((row) => row.payable_account_id === id)!
        expect(after.available_total).toBe(before.available_total)
        expect(after.reserved_total).toBe(before.reserved_total)
    }
    await procurement.page.goto("/procurement/payment-requests")
    await procurement.page.locator("#purchase-payment-requests-create").click()
    const selectedRows = [...firstRows, secondRow, otherSupplier]
    for (const row of selectedRows) {
        await procurement.page.locator(`#ppr-select-${segment(row.payable_account_id)}`).check()
    }
    await procurement.page.locator("#ppr-create-next").click()
    await expect(procurement.page.locator('#ppr-create-form [data-slot="collapsible-trigger"][id^="ppr-group-"]')).toHaveCount(3)
    for (const rows of [firstRows, [secondRow], [otherSupplier]]) {
        const first = rows[0]!
        const trigger = procurement.page.locator(`#ppr-group-${segment(`${first.supplier_id}:${first.payer_party_id}`)}-trigger`)
        await expect(trigger).toContainText(first.supplier_name)
        await expect(trigger).toContainText(`${rows.length} 张采购单 · ${first.payer_party_name}`)
        if (await trigger.getAttribute("aria-expanded") !== "true") await trigger.click()
        const group = procurement.page.locator('[data-slot="collapsible"]').filter({ has: trigger })
        await expect(group.locator('input[id^="ppr-create-line-"]')).toHaveCount(rows.length)
        for (const row of rows) {
            await expect(group.locator(`#ppr-create-line-${segment(row.payable_account_id)}`)).toBeVisible()
        }
        for (const excluded of selectedRows.filter((row) => !rows.some((included) => included.payable_account_id === row.payable_account_id))) {
            await expect(group.locator(`#ppr-create-line-${segment(excluded.payable_account_id)}`)).toHaveCount(0)
        }
    }
    await procurement.page.locator("#ppr-create-dialog-close").click()

    const linkedEvidence = await uploadAnalyticsEvidence(procurementToken)
    const main = await createSubmitted(procurement.page, firstRows, [linkedEvidence])
    const other = await createSubmitted(procurement.page, [secondRow])
    const finance = await openLoggedInWorkspace(browser, "caiwu")
    const financeToken = await apiToken("caiwu")
    const restoreFinance = await taskOnlyRole(procurement.page, adminToken, "caiwu", suffix)
    try {
        await expectTaskMaterials(finance.page, financeToken, main, linkedEvidence, unrelatedEvidence, other, "approval")
        await approveCurrentDocument(finance.page)
    } finally { await restoreFinance() }
    const approved = await detail(procurementToken, main.id)
    expect(approved.status).toBe("approved")
    const cashier = await openLoggedInWorkspace(browser, "fukuan")
    const cashierToken = await apiToken("fukuan")
    const restoreCashier = await taskOnlyRole(procurement.page, adminToken, "fukuan", suffix)
    let paid: Awaited<ReturnType<typeof payOne>>
    try {
        const workItemId = await expectTaskMaterials(cashier.page, cashierToken, approved, linkedEvidence, unrelatedEvidence, other, "finance")
        await rejectUnrelatedReceipt(cashier.page, cashierToken, approved, workItemId, unrelatedEvidence)
        paid = await payOne(cashier.page)
        const afterFreshUpload = await detail(procurementToken, main.id)
        expect(afterFreshUpload.payments).toHaveLength(1)
        const replay = await cashier.page.request.post(`${API_BASE}${ROOT}/${main.id}/payments/commit`, {
            headers: { Authorization: `Bearer ${cashierToken}` },
            multipart: { command: JSON.stringify(paid.command) }, timeout: 60_000,
        })
        const replayBody = await replay.json() as { success: boolean; errorMessage?: string; data: { id: string } }
        expect(replay.ok(), replayBody.errorMessage).toBe(true)
        expect(replayBody.success).toBe(true)
        expect(replayBody.data.id).toBe(paid.result.id)
        const afterReplay = await detail(procurementToken, main.id)
        expect(afterReplay.version).toBe(afterFreshUpload.version)
        expect(afterReplay.lines).toEqual(afterFreshUpload.lines)
        expect(afterReplay.payments).toEqual(afterFreshUpload.payments)
        expect(afterReplay.current_lines).toEqual(afterFreshUpload.current_lines)
        const receipt = afterReplay.payments[0] as Detail["payments"][number] & { receipt: { file_asset_id: string } }
        const originalReceipt = await cashier.page.request.get(`${API_BASE}${ROOT}/${main.id}/materials/${receipt.receipt.file_asset_id}/download?work_item_id=${workItemId}`, {
            headers: { Authorization: `Bearer ${cashierToken}` },
        })
        expect(originalReceipt.ok(), await originalReceipt.text()).toBe(true)
        expect(await originalReceipt.body()).toEqual(BANK_RECEIPT_FILE.buffer)
    } finally { await restoreCashier() }
    expect(paid!.result.status).toBe("posted")
    const partial = await detail(procurementToken, main.id)
    expect(partial.status).toBe("partially_paid")
    await purchasePaymentRequestCommand(procurement.page, "caigou", "POST", `${ROOT}/${main.id}/close`, {
        expected_version: partial.version, idempotency_key: randomUUID(), reason: "未付部分取消，本次付款另按正式冲正核对",
    })
    const beforeReversal = await detail(procurementToken, main.id)
    expect(beforeReversal.status).toBe("closed")

    // PPR-14：冲正必须完成采购与财务真实审批；关闭申请只释放可新申请余额，不恢复执行资格。
    const reversal = await write<{ id: string; reversal_no: string; status: string }>(cashier.page, cashierToken, "POST", "/admin/payment-reversals/commit", {
        source_fact_id: paid!.result.id, amount: null, reason: "E2E 已关闭申请的原付款按原分配冲正", idempotency_key: randomUUID(),
    })
    await openWorkspaceTask(procurement.page, /付款冲正审批/, reversal.reversal_no, "approval")
    await approveCurrentDocument(procurement.page)
    await openWorkspaceTask(finance.page, /付款冲正审批/, reversal.reversal_no, "approval")
    await approveCurrentDocument(finance.page)
    const postedReversal = await apiGet<{ status: string }>(cashierToken, `/admin/payment-reversals/${reversal.id}`)
    expect(postedReversal.status).toBe("posted")
    const reversed = await detail(procurementToken, main.id)
    expect(reversed.status).toBe("closed")
    for (const line of reversed.lines) {
        expect(line.paid_amount).toMatch(/^0(?:\.0+)?$/)
        expect(line.closed_amount).toBe(line.requested_amount)
        expect(reversed.current_lines.find((item) => item.payable_account_id === line.payable_account_id)?.current_reserved_total).toMatch(/^0(?:\.0+)?$/)
        expect((await candidates(procurementToken)).find((item) => item.payable_account_id === line.payable_account_id)?.available_total).toBe(line.requested_amount)
    }
    await cashier.page.goto("/workspace?family=finance")
    await expect(cashier.page.getByRole("button", { name: /供应商付款处理|待付款申请/ }).filter({ hasText: main.request_no })).toHaveCount(0)
    const closedPay = await rawPayment(cashier.page, cashierToken, reversed, paid!.command)
    expect(closedPay.ok(), "冲正后不能通过旧申请执行新付款").toBe(false)
    expect([403, 409, 422], await closedPay.text()).toContain(closedPay.status())

    // PPR-11：正式账户停用会推进原账户版本；财审不得批准失效冻结依据。
    await openWorkspaceTask(finance.page, /付款申请/, other.request_no, "approval")
    try {
        const disabled = await changeBank(procurement.page, adminToken, secondRow.supplier_id, false, suffix)
        const oldAccount = disabled.bank_accounts.find((item) => item.id === other.payee_account.account_id)!
        expect(oldAccount.status).toBe("disabled")
        expect(oldAccount.version).toBeGreaterThan(other.payee_account.version)
        await approveRejectedByAccount(finance.page)
        const unapproved = await detail(procurementToken, other.id)
        expect(unapproved.status).toBe("in_approval")
        expect(unapproved.payee_account).toEqual(other.payee_account)
        await purchasePaymentRequestCommand(procurement.page, "caigou", "POST", `${ROOT}/${other.id}/withdraw`, {
            expected_version: unapproved.version, reason: "账户失效，撤回后按新账户重新申请", idempotency_key: randomUUID(),
        })
    } finally {
        // 清库保留主数据；即使断言失败，也为后续 spec 留下正式有效账户。
        await changeBank(procurement.page, adminToken, secondRow.supplier_id, true, suffix)
    }
    const refreshedRow = (await candidates(procurementToken)).find((item) => item.payable_account_id === secondRow.payable_account_id)!
    const pendingPayment = await createSubmitted(procurement.page, [refreshedRow])
    await openWorkspaceTask(finance.page, /付款申请/, pendingPayment.request_no, "approval")
    await approveCurrentDocument(finance.page)
    const paymentApproved = await detail(procurementToken, pendingPayment.id)
    expect(paymentApproved.status).toBe("approved")
    await openWorkspaceTask(cashier.page, /供应商付款处理|待付款申请/, pendingPayment.request_no, "finance")
    await changeBank(procurement.page, adminToken, secondRow.supplier_id, true, suffix)
    await payOne(cashier.page, "1.00", false)
    const blocked = await detail(procurementToken, pendingPayment.id)
    expect(blocked.status).toBe("approved")
    expect(blocked.payee_account).toEqual(paymentApproved.payee_account)
    expect(blocked.lines[0]!.paid_amount).toMatch(/^0(?:\.0+)?$/)
    expect(blocked.current_lines[0]!.current_reserved_total).toBe(blocked.lines[0]!.requested_amount)
    await purchasePaymentRequestCommand(procurement.page, "caigou", "POST", `${ROOT}/${blocked.id}/close`, {
        expected_version: blocked.version, reason: "收款账户变化，关闭后须采购重新申请和审核", idempotency_key: randomUUID(),
    })
    expect((await detail(procurementToken, blocked.id)).status).toBe("closed")

    // PPR-11/14：正式减额保留尚有余额的 1 元占用；旧批准绑定仍须因采购版本切换停止付款。
    const changeCandidate = (await candidates(procurementToken)).find((item) => item.payable_account_id === firstRows[0]!.payable_account_id)!
    const pendingChange = await createSubmitted(procurement.page, [changeCandidate], [], "1.00")
    await openWorkspaceTask(finance.page, /付款申请/, pendingChange.request_no, "approval")
    await approveCurrentDocument(finance.page)
    const changeApproved = await detail(procurementToken, pendingChange.id)
    expect(changeApproved.status).toBe("approved")
    await openWorkspaceTask(cashier.page, /供应商付款处理|待付款申请/, pendingChange.request_no, "finance")
    const sourcePurchase = await apiGet<{ id: string; version: number; purchase_no: string; current_revision_id: string; totals: { gross: string } }>(procurementToken, `/admin/purchase-orders/${changeCandidate.purchase_order_id}`)
    const started = await write<{ change_id: string }>(procurement.page, procurementToken, "POST", `/admin/purchase-orders/${sourcePurchase.id}/changes`, {
        expected_lock_version: sourcePurchase.version, reason: "E2E 已批准未付申请的正式采购减额", idempotency_key: randomUUID(),
    })
    const changeDraft = await apiGet<{ version: number; payment_term_code: string; lines: Array<{ line_type: string; unit_cost_gross: string | null; [key: string]: unknown }> }>(procurementToken, `/admin/purchase-change-orders/${started.change_id}/draft`)
    const targetLines = changeDraft.lines.map((line) => line.line_type === "ITEM_SERVICE" ? { ...line, unit_cost_gross: "1.00" } : line)
    expect(targetLines.some((line) => line.unit_cost_gross === "1.00")).toBe(true)
    await write(procurement.page, procurementToken, "POST", `/admin/purchase-change-orders/${started.change_id}/submit`, {
        expected_lock_version: changeDraft.version, payment_term_code: changeDraft.payment_term_code, lines: targetLines, idempotency_key: randomUUID(),
    })
    const warehouseReviewer = await openLoggedInWorkspace(browser, "cangchu")
    await openWorkspaceTask(warehouseReviewer.page, /采购变更单审批/, sourcePurchase.purchase_no, "approval")
    await approveCurrentDocument(warehouseReviewer.page)
    await openWorkspaceTask(finance.page, /采购变更单审批/, sourcePurchase.purchase_no, "approval")
    await approveCurrentDocument(finance.page)
    const reducedPurchase = await apiGet<{ current_revision_id: string; totals: { gross: string } }>(procurementToken, `/admin/purchase-orders/${sourcePurchase.id}`)
    expect(reducedPurchase.current_revision_id).not.toBe(sourcePurchase.current_revision_id)
    expect(reducedPurchase.totals.gross).toBe("2.00")
    const sourceChanged = await detail(procurementToken, changeApproved.id)
    expect(sourceChanged.purchase_previews[0]!.revision_id).toBe(sourcePurchase.current_revision_id)
    expect(sourceChanged.current_lines[0]!.source_changed).toBe(true)
    expect(sourceChanged.current_lines[0]!.current_reserved_total).toBe("1.00")
    await payOne(cashier.page, "1.00", false)
    const stillApproved = await detail(procurementToken, changeApproved.id)
    expect(stillApproved.status).toBe("approved")
    expect(stillApproved.version).toBe(changeApproved.version)
    expect(stillApproved.lines[0]!.paid_amount).toMatch(/^0(?:\.0+)?$/)
    await purchasePaymentRequestCommand(procurement.page, "caigou", "POST", `${ROOT}/${stillApproved.id}/close`, {
        expected_version: stillApproved.version, reason: "正式采购减额后关闭旧批准申请并重新核对", idempotency_key: randomUUID(),
    })
    const changedReleased = (await candidates(procurementToken)).find((item) => item.payable_account_id === changeCandidate.payable_account_id)!
    expect(changedReleased.reserved_total).toMatch(/^0(?:\.0+)?$/)
    expect(changedReleased.available_total).toBe("2.00")

    // PPR-04：销售正式 head 推进后，旧采购来源和原批准快照仍是 v1，不能继续付款。
    const untouchedPurchase = purchases.find((purchase) => purchase.purchaseId !== sourcePurchase.id)!
    const salesHeadCandidate = (await candidates(procurementToken)).find((item) => item.purchase_order_id === untouchedPurchase.purchaseId)!
    const salesHeadRequest = await createSubmitted(procurement.page, [salesHeadCandidate], [], "1.00")
    await openWorkspaceTask(finance.page, /付款申请/, salesHeadRequest.request_no, "approval")
    await approveCurrentDocument(finance.page)
    const salesHeadApproved = await detail(procurementToken, salesHeadRequest.id)
    expect(salesHeadApproved.status).toBe("approved")
    expect(salesHeadApproved.payments).toHaveLength(0)
    await openWorkspaceTask(cashier.page, /供应商付款处理|待付款申请/, salesHeadApproved.request_no, "finance")
    const originalSalesHeadTask = new URL(cashier.page.url()).searchParams.get("currentWorkItemId")!
    expect(originalSalesHeadTask).toBeTruthy()
    const purchaseBeforeSalesChange = await apiGet<PurchaseSource>(procurementToken, `/admin/purchase-orders/${untouchedPurchase.purchaseId}`)
    const salesBeforeChange = await apiGet<SalesHead>(salesToken, `/admin/sales-orders/${purchaseBeforeSalesChange.sales_order_id}`)
    const oldSalesRevision = salesBeforeChange.revisions.find((revision) => revision.id === salesBeforeChange.current_revision_id)!
    expect(oldSalesRevision.revision_no).toBe(1)
    expect(purchaseBeforeSalesChange.source_sales_order.revision_id).toBe(salesBeforeChange.current_revision_id)
    expect(salesHeadApproved.sales_previews[0]!.revision_id).toBe(salesBeforeChange.current_revision_id)
    const salesChange = await write<{ id: string }>(procurement.page, salesToken, "POST", "/admin/sales-change-orders", {
        sales_order_id: salesBeforeChange.id, expected_base_revision_no: oldSalesRevision.revision_no,
        change_type: "AMOUNT", reason: "E2E 正式销售成交价变化后停止旧来源批准付款", idempotency_key: randomUUID(),
    })
    const salesChangeDraft = await apiGet<SalesChangeDraft>(salesToken, `/admin/sales-change-orders/${salesChange.id}/draft`)
    const salesTargetLines = salesChangeDraft.lines.map((line) => line.goods ? { ...line, goods: { ...line.goods, unit_price_gross: "101.00" } } : line)
    expect(salesTargetLines.some((line) => line.goods?.unit_price_gross === "101.00")).toBe(true)
    const savedSalesChange = await write<SalesChangeDraft>(procurement.page, salesToken, "POST", `/admin/sales-change-orders/${salesChange.id}/draft`, {
        expected_version: salesChangeDraft.version, expected_working_copy_version: salesChangeDraft.working_copy_version,
        reason: "只调整销售成交单价，保留原数量与我方结算主体", business_remark: salesChangeDraft.business_remark, lines: salesTargetLines,
    })
    await write(procurement.page, salesToken, "POST", `/admin/sales-change-orders/${salesChange.id}/submit-impact`, {
        version: savedSalesChange.version, idempotency_key: randomUUID(),
    })
    await openWorkspaceTask(procurement.page, /销售变更单审批/, salesBeforeChange.order_no, "approval")
    await approveCurrentDocument(procurement.page)
    await openWorkspaceTask(finance.page, /销售变更单审批/, salesBeforeChange.order_no, "approval")
    await approveCurrentDocument(finance.page)
    const salesAfterChange = await apiGet<SalesHead>(salesToken, `/admin/sales-orders/${salesBeforeChange.id}`)
    expect(salesAfterChange.commercial_status).toBe("EFFECTIVE")
    expect(salesAfterChange.current_revision_id).not.toBe(salesBeforeChange.current_revision_id)
    expect(salesAfterChange.revisions.find((revision) => revision.id === salesAfterChange.current_revision_id)?.revision_no).toBe(2)
    expect(salesAfterChange.settlement_party_id).toBe(salesBeforeChange.settlement_party_id)
    expect(salesAfterChange.revisions.find((revision) => revision.id === salesBeforeChange.current_revision_id)).toEqual(oldSalesRevision)
    expect(salesAfterChange.purchase_coverage).toMatchObject({
        source_changed: true, outdated_purchase_quantity: "2", outdated_purchase_count: 1,
    })
    const salesViewer = await openLoggedInWorkspace(browser, "xiaoshou")
    await salesViewer.page.goto(`/sales/orders/${salesBeforeChange.id}?section=overview`)
    const sourceNotice = salesViewer.page.locator("#sales-order-overview-source-changed-notice")
    await expect(sourceNotice).toBeVisible(VISIBLE)
    await expect(sourceNotice).toContainText("采购依据待更新")
    await expect(sourceNotice).toContainText("已有 1 张采购单，数量 2")
    await expect(sourceNotice).toContainText("引用变更前的销售版本。请先完成相关采购变更，再继续供给分配。")
    await salesViewer.page.getByRole("tab", { name: /采购/ }).click()
    const purchaseSourceNotice = salesViewer.page.locator("#sales-order-purchase-source-changed-notice")
    await expect(purchaseSourceNotice).toBeVisible(VISIBLE)
    await expect(purchaseSourceNotice).toContainText("采购依据待更新")
    await expect(purchaseSourceNotice).toContainText("已有 1 张采购单，数量 2")
    await expect(purchaseSourceNotice).toContainText("引用变更前的销售版本。请先完成相关采购变更，再继续供给分配。")
    const purchaseAfterSalesChange = await apiGet<PurchaseSource>(procurementToken, `/admin/purchase-orders/${untouchedPurchase.purchaseId}`)
    expect(purchaseAfterSalesChange.current_revision_id).toBe(purchaseBeforeSalesChange.current_revision_id)
    expect(purchaseAfterSalesChange.source_sales_order.revision_id).toBe(salesBeforeChange.current_revision_id)
    expect(purchaseAfterSalesChange.source_sales_order.revision_no).toBe(1)
    const salesSourceChanged = await detail(procurementToken, salesHeadApproved.id)
    expect(salesSourceChanged.sales_previews).toEqual(salesHeadApproved.sales_previews)
    expect(salesSourceChanged.purchase_previews).toEqual(salesHeadApproved.purchase_previews)
    expect(salesSourceChanged.version).toBe(salesHeadApproved.version)
    expect(salesSourceChanged.payee_account).toEqual(salesHeadApproved.payee_account)
    expect(salesSourceChanged.current_lines[0]!.source_changed).toBe(true)
    expect(salesSourceChanged.current_lines[0]!.current_reserved_total).toBe("1.00")
    const salesHeadPayment = await payOne(cashier.page, "1.00", false)
    expect(salesHeadPayment.command.payment_request_id).toBe(salesHeadApproved.id)
    expect(salesHeadPayment.command.expected_request_version).toBe(salesHeadApproved.version)
    expect(salesHeadPayment.command.work_item_id).toBe(originalSalesHeadTask)
    expect(salesHeadPayment.command.expected_payee_bank_account_id).toBe(salesHeadApproved.payee_account.account_id)
    expect(salesHeadPayment.command.expected_payee_bank_account_version).toBe(salesHeadApproved.payee_account.version)
    const salesPaymentBlocked = await detail(procurementToken, salesHeadApproved.id)
    expect(salesPaymentBlocked.status).toBe("approved")
    expect(salesPaymentBlocked.version).toBe(salesHeadApproved.version)
    expect(salesPaymentBlocked.lines).toEqual(salesHeadApproved.lines)
    expect(salesPaymentBlocked.payments).toEqual(salesHeadApproved.payments)
    expect(salesPaymentBlocked.current_lines).toEqual(salesSourceChanged.current_lines)
    await purchasePaymentRequestCommand(procurement.page, "caigou", "POST", `${ROOT}/${salesHeadApproved.id}/close`, {
        expected_version: salesPaymentBlocked.version, reason: "销售正式版本变化，关闭旧来源批准的未付额度", idempotency_key: randomUUID(),
    })
    const closedSalesHeadRequest = await detail(procurementToken, salesHeadApproved.id)
    expect(closedSalesHeadRequest.status).toBe("closed")
    expect(closedSalesHeadRequest.current_lines[0]!.current_reserved_total).toMatch(/^0(?:\.0+)?$/)
    expect(closedSalesHeadRequest.current_lines[0]!.current_available_total).toBe(closedSalesHeadRequest.current_lines[0]!.current_open_total)
    expect(closedSalesHeadRequest.payments).toHaveLength(0)
})
