import { randomUUID } from "node:crypto"
import { readFileSync } from "node:fs"

import { expect, type Page } from "@playwright/test"

import { API_BASE, apiGet, apiToken } from "./api"
import { openLoggedInWorkspace } from "./login"
import { approveCurrentDocument, openFulfillmentWorkspaceForm, openWorkspaceTask } from "./ui"

export const BANK_RECEIPT_FILE = {
    name: "e2e-bank-receipt.png",
    mimeType: "image/png",
    buffer: Buffer.from(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j5xkAAAAASUVORK5CYII=",
        "base64",
    ),
}

type SupplierPaymentResult = {
    id: string; payment_no: string; status: string; amount: string; effective_amount: string;
    allocated_total: string; unallocated_amount: string;
    bank_receipt: { asset_id: string; file_name: string; content_type: string; byte_size: number };
    allocations: Array<{
        allocation_action: string; payable_account_id: string; source_type: string;
        source_document_no: string; allocated_amount: string;
    }>
}

const paymentCents = (value: string): bigint => {
    const [whole, fraction = ""] = value.split(".")
    return BigInt(whole!) * 100n + BigInt(fraction.padEnd(2, "0"))
}

const paymentRequestByPurchase = new Map<string, string>()

/** 付款任务按申请单号识别，不能再以采购应付任务号判断申请资格。 */
export function supplierPaymentTaskPattern(purchaseNo: string): RegExp {
    const requestNo = paymentRequestByPurchase.get(purchaseNo) ?? purchaseNo
    const escaped = requestNo.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
    return new RegExp(`(?:供应商付款处理|待付款申请)[\\s\\S]*${escaped}|${escaped}[\\s\\S]*(?:供应商付款处理|待付款申请)`)
}

export type PaymentRequestCandidate = {
    purchase_order_id: string
    purchase_no: string
    supplier_id: string
    supplier_name: string
    payer_party_id: string
    payer_party_name: string
    payable_account_id: string
    open_total: string
    reserved_total: string
    available_total: string
    bank_accounts: Array<{ id: string; version: number; is_default: boolean }>
}

export type PurchasePaymentRequestResult = {
    id: string
    version: number
    submission_version: number
    work_item_id: string | null
    request_no: string
    status: string
    total_amount: string
    lines: Array<{ payable_account_id: string; requested_amount: string; paid_amount: string; closed_amount: string; source_snapshot?: { purchase_preview: { purchase_no: string } } }>
}

/** 使用正式命令接口建立申请，执行结果须同时符合 HTTP 和信封约定。 */
export async function purchasePaymentRequestCommand<T>(
    page: Page,
    identity: string,
    method: "POST" | "PUT",
    path: string,
    data: unknown,
): Promise<T> {
    const token = await apiToken(identity)
    const response = await page.request.fetch(`${API_BASE}${path}`, {
        method,
        headers: { Authorization: `Bearer ${token}` },
        data,
        timeout: 60_000,
    })
    expect(response.ok(), await response.text()).toBe(true)
    const envelope = await response.json()
    expect(envelope.success, envelope.errorMessage).toBe(true)
    return envelope.data as T
}

/** 采购按当前可申请余额逐单发起，财务在真实冻结任务中批准后才产生付款资格。 */
export async function authorizePurchasePaymentRequest(
    page: Page,
    purchaseNo?: string,
): Promise<PurchasePaymentRequestResult> {
    const token = await apiToken("caigou")
    const candidates = await apiGet<PaymentRequestCandidate[]>(
        token,
        "/admin/purchase-payment-requests/candidates",
    )
    const candidate = candidates.find((row) =>
        (!purchaseNo || row.purchase_no === purchaseNo) &&
        /[1-9]/.test(row.available_total),
    )
    expect(candidate, `应存在可申请余额的采购单 ${purchaseNo ?? ""}`).toBeTruthy()
    const account = candidate!.bank_accounts.find((row) => row.is_default)
        ?? candidate!.bank_accounts[0]
    expect(account, "申请必须明确选择供应商有效收款账户").toBeTruthy()
    const draft = await purchasePaymentRequestCommand<PurchasePaymentRequestResult>(
        page, "caigou", "POST", "/admin/purchase-payment-requests", {
            data: {
                supplier_id: candidate!.supplier_id,
                payee_bank_account_id: account!.id,
                lines: [{ payable_account_id: candidate!.payable_account_id, amount: candidate!.available_total }],
                desired_payment_date: new Intl.DateTimeFormat("en-CA", { timeZone: "Asia/Shanghai" }).format(new Date()),
                reason: `E2E采购核对后申请支付 ${candidate!.purchase_no}`,
                attachment_ids: [],
            },
        },
    )
    const submitted = await purchasePaymentRequestCommand<PurchasePaymentRequestResult>(
        page, "caigou", "POST", `/admin/purchase-payment-requests/${draft.id}/submit`, {
            expected_version: draft.version,
            idempotency_key: randomUUID(),
        },
    )
    expect(submitted.status).toBe("in_approval")
    const browser = page.context().browser()
    if (!browser) throw new Error("付款申请审核需要浏览器上下文")
    const finance = await openLoggedInWorkspace(browser, "caiwu")
    await openWorkspaceTask(finance.page, /付款申请/, draft.request_no, "approval")
    await approveCurrentDocument(finance.page)
    const approved = await apiGet<PurchasePaymentRequestResult>(
        token, `/admin/purchase-payment-requests/${draft.id}`,
    )
    expect(approved.status).toBe("approved")
    paymentRequestByPurchase.set(candidate!.purchase_no, approved.request_no)
    return approved
}

/** 打开本采购已批准申请的付款任务；没有申请时先完成采购申请和财务审核。 */
export async function openAuthorizedSupplierPaymentTask(
    page: Page,
    purchaseNo?: string,
): Promise<PurchasePaymentRequestResult> {
    await page.goto("/workspace?family=finance")
    await expect(page.locator("#workspace-home-refresh")).toBeEnabled({ timeout: 20_000 })
    const token = await apiToken("caigou")
    const requests = await apiGet<{ items: PurchasePaymentRequestResult[] }>(token, "/admin/purchase-payment-requests", { page: 1, page_size: 100 })
    let request = requests.items.find((item) => ["approved", "partially_paid"].includes(item.status) &&
        (!purchaseNo || item.lines.some((line) => line.source_snapshot?.purchase_preview.purchase_no === purchaseNo)))
    if (!request) {
        request = await authorizePurchasePaymentRequest(page, purchaseNo)
        await page.goto("/workspace?family=finance")
    }
    if (purchaseNo) paymentRequestByPurchase.set(purchaseNo, request.request_no)
    expect(request.work_item_id, "已批准申请须绑定正式出纳任务").toBeTruthy()
    const taskId = request.work_item_id!
    const taskSegment = taskId.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")
    const task = page.locator(`[id="workspace-task-${taskSegment}"]`)
    await expect(task).toHaveCount(1, { timeout: 20_000 })
    await expect(task).toBeVisible({ timeout: 20_000 })
    await expect(task).toContainText(request.request_no)
    await task.click()
    await expect(task).toHaveAttribute("aria-current", "true", { timeout: 20_000 })
    await expect.poll(() => new URL(page.url()).searchParams.get("currentWorkItemId"), { timeout: 20_000 }).toBe(taskId)
    // 任务可能已经自动选中，Query 缓存也可能已加载；正式读取精确任务上下文核对身份。
    const cashierToken = await apiToken("fukuan")
    const profile = await apiGet<{ userid: string; account: string }>(cashierToken, "/account/profile")
    const authority = await apiGet<{
        id: string; work_item_type: string; business_object_type: string; business_object_id: string;
        owner_user_id: string | null; status: string; task_version: string; subject_version: string; allowed_actions: string[];
    }>(cashierToken, `/admin/work-items/${taskId}`)
    expect(profile.account).toBe("fukuan")
    expect(authority).toMatchObject({
        id: taskId, work_item_type: "SUPPLIER_PAYMENT_EXECUTION", business_object_type: "purchase_payment_request",
        business_object_id: request.id, owner_user_id: profile.userid, status: "OPEN", subject_version: String(request.submission_version),
    })
    expect(authority.task_version).toMatch(/^\d+$/)
    expect(authority.allowed_actions).toContain("PROCESS")
    const context = await apiGet<PurchasePaymentRequestResult>(cashierToken, `/admin/work-items/${taskId}/purchase-payment-request`)
    expect(context).toMatchObject({ id: request.id, work_item_id: taskId, submission_version: request.submission_version })
    expect(["approved", "partially_paid"]).toContain(context.status)
    const surface = page.getByRole("region", { name: "当前工作台任务" })
    await expect(surface.getByRole("region", { name: "付款申请摘要" }).getByText(request.request_no, { exact: true })).toBeVisible({ timeout: 20_000 })
    await expect(surface.locator("#ppr-pay-form")).toBeVisible({ timeout: 20_000 })
    for (const line of context.lines) {
        const segment = line.payable_account_id.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")
        await expect(surface.locator(`[id="ppr-pay-line-${segment}"]`)).toBeVisible({ timeout: 20_000 })
    }
    return context
}

/** 出纳从已批准申请付款，逐单登记并等待正式事务提交。 */
export async function payOnlySupplierTask(
    page: Page,
    documentHint?: string,
): Promise<SupplierPaymentResult> {
    const request = await openAuthorizedSupplierPaymentTask(page, documentHint)
    const expectedByAccount = new Map<string, bigint>()
    for (const line of request.lines) {
        const remaining = paymentCents(line.requested_amount) - paymentCents(line.paid_amount) - paymentCents(line.closed_amount)
        if (remaining <= 0n) continue
        expectedByAccount.set(line.payable_account_id, remaining)
        const id = line.payable_account_id.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")
        await page.locator(`[id="ppr-pay-line-${id}"]`).fill(`${remaining / 100n}.${String(remaining % 100n).padStart(2, "0")}`)
    }
    await page.locator("#ppr-pay-bank-receipt").setInputFiles(BANK_RECEIPT_FILE)
    await page.locator("#ppr-pay-submit").click()
    const dialog = page.getByRole("alertdialog").filter({ hasText: "确认登记付款" })
    await expect(dialog).toBeVisible()
    await expect(dialog.getByText("提交审批")).toHaveCount(0)
    const committed = page.waitForResponse(
        (response) => response.request().method() === "POST" &&
            /\/admin\/purchase-payment-requests\/[^/]+\/payments\/commit/.test(response.url()),
        { timeout: 60_000 },
    )
    await dialog.locator("#ppr-pay-confirm-confirm").click()
    const response = await committed
    expect(response.ok(), await response.text()).toBe(true)
    const result = (await response.json()).data as SupplierPaymentResult
    expect(result).toMatchObject({ status: "posted" })
    expect(result.id).toBeTruthy()
    expect(result.payment_no).toBeTruthy()
    const total = [...expectedByAccount.values()].reduce((sum, amount) => sum + amount, 0n)
    expect(paymentCents(result.amount)).toBe(total)
    expect(paymentCents(result.effective_amount)).toBe(total)
    expect(paymentCents(result.allocated_total)).toBe(total)
    expect(paymentCents(result.unallocated_amount)).toBe(0n)
    const actualByAccount = new Map<string, bigint>()
    for (const allocation of result.allocations) {
        expect(allocation).toMatchObject({ allocation_action: "apply", source_type: "purchase_order" })
        actualByAccount.set(allocation.payable_account_id,
            (actualByAccount.get(allocation.payable_account_id) ?? 0n) + paymentCents(allocation.allocated_amount))
    }
    expect(actualByAccount, "实际付款必须按申请逐采购金额核销，不得挪用其他行额度").toEqual(expectedByAccount)
    expect(result.bank_receipt).toMatchObject({
        file_name: BANK_RECEIPT_FILE.name, content_type: BANK_RECEIPT_FILE.mimeType, byte_size: BANK_RECEIPT_FILE.buffer.length,
    })
    await expect(dialog).toBeHidden({ timeout: 20_000 })
    return result
}

/** 同一采购履约任务提供来源采购回单；验证下载后恢复原任务的履约表单。 */
export async function expectProcurementPaymentReceipt(
    page: Page,
    payment: SupplierPaymentResult,
    purchaseNo: string,
): Promise<void> {
    await expect
        .poll(() => new URL(page.url()).searchParams.get("currentWorkItemId"), {
            timeout: 20_000,
        })
        .toBeTruthy()
    const workItemId = new URL(page.url()).searchParams.get(
        "currentWorkItemId",
    )!
    expect(payment.allocations.some((row) => row.source_document_no === purchaseNo), "回单必须属于当前履约来源采购的实际付款分配").toBe(true)
    const opened = page.getByRole("dialog")
        .filter({ has: page.locator('[aria-label="当前履约任务"]') })
        .filter({ visible: true })
    const wasOpen = await opened.count()
    expect([0, 1]).toContain(wasOpen)
    const dialogId = `workspace-fulfillment-process-dialog-${workItemId}`
    if (wasOpen === 1) {
        await expect(opened).toHaveAttribute("id", dialogId)
        await expect(opened.getByRole("region", { name: "来源单据", exact: true }).getByText(purchaseNo, { exact: true })).toBeVisible({ timeout: 20_000 })
        await opened.locator('[data-slot="dialog-close"]').click()
        await expect(opened).toBeHidden({ timeout: 20_000 })
    }
    expect(new URL(page.url()).searchParams.get("currentWorkItemId")).toBe(workItemId)
    const segment = workItemId.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")
    const task = page.locator(`[id="workspace-task-${segment}"]`)
    await expect(task).toHaveCount(1)
    await expect(task).toHaveAttribute("aria-current", "true")
    const surface = page.getByRole("region", { name: "当前工作台任务", exact: true })
    const receipts = surface.getByRole("region", { name: "财务付款回单", exact: true })
    await expect(receipts).toHaveCount(1)
    await expect(receipts.getByRole("heading", { name: "财务付款回单", exact: true })).toBeVisible({ timeout: 20_000 })
    const token = await apiToken("caigou")
    const headers = { Authorization: `Bearer ${token}` }
    const listPath = `/admin/work-items/${workItemId}/payment-receipts`
    const listed = await page.request.get(`${API_BASE}${listPath}`, { headers })
    expect(listed.ok(), await listed.text()).toBeTruthy()
    const files = (await listed.json()).data as Array<{
        document_id: string
        document_no: string
        file_asset_id: string
        file_name: string
        content_type: string
        byte_size: number
    }>
    expect(files).toHaveLength(1)
    expect(files[0]).toMatchObject({
        document_id: payment.id,
        document_no: payment.payment_no,
        file_asset_id: payment.bank_receipt.asset_id,
        file_name: BANK_RECEIPT_FILE.name,
        content_type: BANK_RECEIPT_FILE.mimeType,
        byte_size: BANK_RECEIPT_FILE.buffer.length,
    })
    const row = receipts.getByRole("listitem")
    await expect(row).toHaveCount(1)
    await expect(row.getByText(BANK_RECEIPT_FILE.name, { exact: true })).toBeVisible()
    await expect(row).toContainText(payment.payment_no, { timeout: 20_000 })
    const downloadButton = row.getByRole("button", { name: "下载", exact: true })
    await expect(downloadButton).toHaveAttribute("id", `purchase-payment-${workItemId}-${payment.id}-${payment.bank_receipt.asset_id}-download`)
    const downloadPath = `${listPath}/${payment.id}/download`
    const [download, response] = await Promise.all([
        page.waitForEvent("download"),
        page.waitForResponse(
            (result) => new URL(result.url()).pathname === downloadPath,
        ),
        downloadButton.click(),
    ])
    expect(download.suggestedFilename()).toBe(BANK_RECEIPT_FILE.name)
    expect(await download.failure()).toBeNull()
    expect(response.ok()).toBeTruthy()
    expect(response.headers()["content-type"]).toBe(BANK_RECEIPT_FILE.mimeType)
    expect(response.headers()["cache-control"]).toBe("private, no-store")
    const downloadedPath = await download.path()
    expect(downloadedPath, "浏览器下载应生成真实回单文件").not.toBeNull()
    expect(readFileSync(downloadedPath!)).toEqual(BANK_RECEIPT_FILE.buffer)
    const unrelated = await page.request.get(
        `${API_BASE}${listPath}/payment-not-linked-to-task/download`,
        { headers },
    )
    expect(unrelated.status(), await unrelated.text()).toBe(404)
    expect(new URL(page.url()).searchParams.get("currentWorkItemId")).toBe(workItemId)
    if (wasOpen === 1) {
        const reopened = await openFulfillmentWorkspaceForm(page)
        await expect(reopened).toHaveAttribute("id", dialogId)
        await expect(reopened.getByRole("region", { name: "来源单据", exact: true }).getByText(purchaseNo, { exact: true })).toBeVisible({ timeout: 20_000 })
        expect(new URL(page.url()).searchParams.get("currentWorkItemId")).toBe(workItemId)
    }
}
