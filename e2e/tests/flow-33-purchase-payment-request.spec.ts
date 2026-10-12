/**
 * [flow-33] 采购付款申请：内部审批、并发占用、财审、部分付款冲正与未付关闭。
 * 生产建档及正式单据命令形成来源；浏览器执行申请录入、财审和付款，API 验证额度及生命周期。
 * 合同：docs/purchase-payment-request-contract.md PPR-01/02/05/06/07/08/09/12/13/14。
 */
import { randomUUID } from "node:crypto"

import { test, expect, type Page, type Response } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { analyticsDate, createAnalyticsPurchase, prepareAnalyticsOrders, uploadAnalyticsEvidence, type AnalyticsPurchase } from "../helpers/financial-analytics"
import { rejectFinancialOriginal } from "../helpers/financial-draft-edit"
import { openLoggedInWorkspace } from "../helpers/login"
import { BANK_RECEIPT_FILE, purchasePaymentRequestCommand, type PaymentRequestCandidate, type PurchasePaymentRequestResult } from "../helpers/payments"
import { approveCurrentDocument, chooseOption, openWorkspaceTask, pickCalendarDay } from "../helpers/ui"

const ROOT = "/admin/purchase-payment-requests"
const VISIBLE = { timeout: 20_000 }

type Detail = PurchasePaymentRequestResult & {
    total_amount: string
    submission_version: number
    approval_instance_id: string | null
    work_item_id: string | null
    supplier_id: string
    data: ReturnType<typeof draftData>["data"]
    payee_account: { account_id: string; version: number } | null
    lines: Array<{ payable_account_id: string; purchase_order_id: string; requested_amount: string; paid_amount: string; closed_amount: string; payment_terms: { payment_term_code: string; prepay_gate: boolean } }>
    purchase_previews: Array<{ purchase_order_id: string }>
    sales_previews: Array<{ sales_order_id: string }>
    current_lines: Array<{
        payable_account_id: string
        current_open_total: string
        current_reserved_total: string
        current_available_total: string
        current_settled_total: string
        payable_entries: Array<{ id: string; open_total: string; settled_total: string; offset_total: string }>
    }>
    attachments: Array<{ file_asset_id: string }>
    payments: Array<{ payment_id: string; amount: string; effective_amount: string }>
}

type SupplierPeriodProfile = {
    id: string; party_version: number; version: number; legal_name: string;
    short_name: string | null; unified_credit_code: string | null; maintainer_user_id: string;
    current_commercial_profile_revision_id: string;
    capability_codes: string[];
    capabilities: Array<{ id: string; capability_code: string; owner_user_id: string }>;
    qualifications: Array<{
        qualification_type: string; certificate_no: string; issuer: string | null;
        valid_from: string | null; valid_to: string | null; attachment_id: string | null;
        capability_ids: string[]; status: string;
    }>;
    current_profile: {
        id: string; settlement_mode: string; reconciliation_cycle: string; payment_term_snapshot: string;
        business_category: string | null; invoice_type: string; invoice_tax_rate: string | null;
        invoice_tax_rates?: string[]; signing_entity_party_id: string; payment_entity_party_id: string;
    };
    commercial_profiles: Array<{ id: string; settlement_mode: string; reconciliation_cycle: string; payment_term_snapshot: string }>;
}

/** 完整正式商档修订；周期付款与每月对账分别保存，不修改任何采购冻结来源。 */
async function revisePeriodProfile(page: Page, token: string, supplierId: string, period: "QUARTER" | "YEAR") {
    const source = await apiGet<SupplierPeriodProfile>(token, `/admin/suppliers/${supplierId}`)
    const mode = period === "QUARTER" ? "quarterly" : "yearly"
    const code = `PERIOD_${period}_15`
    const qualifications = source.qualifications.filter((item) => item.status === "active").map((item) => ({
        qualification_type: item.qualification_type, certificate_no: item.certificate_no,
        issuer: item.issuer, valid_from: item.valid_from, valid_to: item.valid_to, attachment_id: item.attachment_id,
        capability_codes: item.capability_ids.map((id) => source.capabilities.find((capability) => capability.id === id)!.capability_code),
    }))
    if (period === "QUARTER") {
        const browser = page.context().browser()
        if (!browser) throw new Error("周期付款商档验收需要真实浏览器")
        const editor = await openLoggedInWorkspace(browser, "admin")
        try {
            await editor.page.goto(`/master-data/suppliers/${supplierId}`)
            await expect(editor.page.locator("#master-data-supplier-basic-name")).toBeVisible(VISIBLE)
            await editor.page.locator("#master-data-supplier-editor-tab-commercial").click()
            const reconciliation = editor.page.locator("#master-data-supplier-commercial-reconciliation-cycle-combobox")
            await chooseOption(editor.page, reconciliation, "每月对账")
            await expect(reconciliation).toHaveValue("每月对账")
            await chooseOption(editor.page, editor.page.locator("#master-data-supplier-commercial-settlement-combobox"), "季结")
            await editor.page.locator("#master-data-supplier-commercial-payment-days").fill("15")
            await expect(reconciliation, "修改付款周期不得覆盖已选对账频率").toHaveValue("每月对账")
            await expect(editor.page.locator("#master-data-supplier-commercial-settlement-combobox")).toHaveValue("季结")
            await expect(editor.page.locator("#master-data-supplier-commercial-payment-days")).toHaveValue("15")
            await editor.page.locator("#master-data-supplier-document-header-submit").click()
            await editor.page.locator("#supplier-save-reason").fill("E2E 季度付款独立保留每月对账，正式采购按商档冻结")
            const saved = editor.page.waitForResponse((item) => item.request().method() === "PUT" &&
                new URL(item.url()).pathname === `/admin/supplier-profiles/${supplierId}`, { timeout: 60_000 })
            await editor.page.locator("#master-data-supplier-supplier-save-reason-dialog-button-2").click()
            const response = await saved
            expect(response.request().postDataJSON()).toMatchObject({
                expected_party_version: source.party_version, expected_supplier_version: source.version,
                settlement_mode: mode, reconciliation_cycle: "monthly", payment_term_snapshot: code,
            })
            expect(response.ok(), await response.text()).toBe(true)
            const envelope = await response.json() as { success: boolean; errorMessage?: string }
            expect(envelope.success, envelope.errorMessage).toBe(true)
            await editor.page.reload()
            await editor.page.locator("#master-data-supplier-editor-tab-commercial").click()
            await expect(editor.page.locator("#master-data-supplier-commercial-settlement-combobox")).toHaveValue("季结")
            await expect(editor.page.locator("#master-data-supplier-commercial-payment-days")).toHaveValue("15")
            await expect(reconciliation).toHaveValue("每月对账")
        } finally {
            await editor.context.close()
        }
    } else {
        const response = await page.request.put(`${API_BASE}/admin/supplier-profiles/${supplierId}`, {
            headers: { Authorization: `Bearer ${token}` }, timeout: 60_000,
            data: {
                idempotency_key: randomUUID(), expected_party_version: source.party_version, expected_supplier_version: source.version,
                legal_name: source.legal_name, short_name: source.short_name, unified_credit_code: source.unified_credit_code,
                ...source.current_profile, settlement_mode: mode, reconciliation_cycle: "monthly", payment_term_snapshot: code,
                bank_account: null, clear_bank_account: false, maintainer_user_id: source.maintainer_user_id,
                capability_codes: source.capability_codes,
                capability_owners: source.capabilities.filter((capability) => source.capability_codes.includes(capability.capability_code))
                    .map((capability) => ({ capability_code: capability.capability_code, owner_user_id: capability.owner_user_id })),
                qualifications,
                effective_from: analyticsDate(), change_reason: `E2E ${period}周期付款保持每月对账，采购按此真实商档冻结`,
            },
        })
        expect(response.ok(), await response.text()).toBe(true)
        const envelope = await response.json() as { success: boolean; errorMessage?: string }
        expect(envelope.success, envelope.errorMessage).toBe(true)
    }
    const revised = await apiGet<SupplierPeriodProfile>(token, `/admin/suppliers/${supplierId}`)
    expect(revised.current_profile).toMatchObject({ settlement_mode: mode, reconciliation_cycle: "monthly", payment_term_snapshot: code })
    expect(revised.current_commercial_profile_revision_id).not.toBe(source.current_commercial_profile_revision_id)
    expect(revised.capability_codes.sort()).toEqual(source.capability_codes.sort())
    return { supplierId, profileId: revised.current_commercial_profile_revision_id, code, mode }
}

function moneyCents(value: string): bigint {
    const [whole, fraction = ""] = value.split(".")
    return BigInt(whole!) * 100n + BigInt(fraction.padEnd(2, "0"))
}
function money(value: bigint): string {
    return `${value / 100n}.${String(value % 100n).padStart(2, "0")}`
}
async function candidates(token: string) {
    return apiGet<PaymentRequestCandidate[]>(token, `${ROOT}/candidates`)
}
async function detail(token: string, id: string) {
    return apiGet<Detail>(token, `${ROOT}/${id}`)
}
async function rawCommand(page: Page, token: string, path: string, data: unknown) {
    return page.request.post(`${API_BASE}${path}`, {
        headers: { Authorization: `Bearer ${token}` }, data, timeout: 60_000,
    })
}
function draftData(rows: PaymentRequestCandidate[]) {
    const first = rows[0]!
    const account = first.bank_accounts.find((row) => row.is_default) ?? first.bank_accounts[0]!
    return {
        data: {
            supplier_id: first.supplier_id,
            payee_bank_account_id: account.id,
            desired_payment_date: analyticsDate(),
            reason: "采购已核对本期服务采购，申请按逐单金额支付",
            attachment_ids: [] as string[],
            lines: rows.map((row) => ({ payable_account_id: row.payable_account_id, amount: row.available_total })),
        },
    }
}

function allocationsFor(request: Detail, accountId: string, amount: string) {
    const line = request.current_lines.find((row) => row.payable_account_id === accountId)
    expect(line, "分配必须使用本申请真实应付分录").toBeTruthy()
    let remaining = moneyCents(amount)
    const allocations: Array<{ payable_entry_id: string; allocated_amount: string }> = []
    for (const entry of line!.payable_entries) {
        const open = moneyCents(entry.open_total)
        const allocated = remaining < open ? remaining : open
        if (allocated > 0n) allocations.push({ payable_entry_id: entry.id, allocated_amount: money(allocated) })
        remaining -= allocated
        if (remaining === 0n) break
    }
    expect(remaining, "负向命令仍须符合当前应付余额，证明拒绝源于申请授权").toBe(0n)
    return allocations
}

function monetaryFacts(request: Detail) {
    return {
        version: request.version, status: request.status, total: request.total_amount,
        lines: request.lines.map((line) => ({ ...line })),
        current_lines: request.current_lines,
        payments: request.payments,
    }
}

/** 金额按整数分比较，仍保留逐行身份、冻结条件和实际分录的完整事实。 */
function exactLineBalances(request: Detail) {
    return {
        lines: request.lines.map((line) => ({
            ...line,
            requested_amount: moneyCents(line.requested_amount),
            paid_amount: moneyCents(line.paid_amount),
            closed_amount: moneyCents(line.closed_amount),
        })),
        current_lines: request.current_lines.map((line) => ({
            ...line,
            current_open_total: moneyCents(line.current_open_total),
            current_reserved_total: moneyCents(line.current_reserved_total),
            current_available_total: moneyCents(line.current_available_total),
            current_settled_total: moneyCents(line.current_settled_total),
            payable_entries: line.payable_entries.map((entry) => ({
                ...entry,
                open_total: moneyCents(entry.open_total),
                settled_total: moneyCents(entry.settled_total),
                offset_total: moneyCents(entry.offset_total),
            })),
        })),
    }
}

/** 完整且当前版本的付款命令及真实回单，业务拒绝不得改变付款、应付或申请占用。 */
async function expectPaymentRejected(
    page: Page, token: string, readerToken: string, request: Detail, approved: Detail,
    allocations: Array<{ payable_entry_id: string; allocated_amount: string }>, message: RegExp,
) {
    expect(approved.work_item_id).toBeTruthy()
    const task = await apiGet<{ id: string; task_version: string; business_object_id: string }>(token, `/admin/work-items/${approved.work_item_id}`)
    expect(task.business_object_id).toBe(approved.id)
    const reference = "pending-file:ppr-negative-bank-receipt"
    const command = {
        payment_request_id: request.id,
        expected_request_version: request.version,
        work_item_id: task.id,
        expected_task_version: task.task_version,
        additional_work_items: [],
        expected_payee_bank_account_id: approved.payee_account!.account_id,
        expected_payee_bank_account_version: approved.payee_account!.version,
        payment: {
            payment_no: `E2E-PPR-${randomUUID()}`,
            supplier_id: request.supplier_id,
            paid_at: Math.floor(Date.now() / 1000),
            amount: money(allocations.reduce((sum, row) => sum + moneyCents(row.allocated_amount), 0n)),
            bank_reference: "E2E完整付款负向命令",
            bank_receipt_asset_id: reference,
        },
        allocations,
        idempotency_key: randomUUID(),
    }
    const before = {
        request: monetaryFacts(await detail(readerToken, request.id)),
        approved: monetaryFacts(await detail(readerToken, approved.id)),
        candidates: await candidates(readerToken),
    }
    const response = await page.request.post(`${API_BASE}${ROOT}/${request.id}/payments/commit`, {
        headers: { Authorization: `Bearer ${token}` },
        multipart: { command: JSON.stringify(command), [reference]: BANK_RECEIPT_FILE }, timeout: 60_000,
    })
    const envelope = await response.json() as { success: boolean; errorMessage?: string }
    expect(response.ok(), envelope.errorMessage).toBe(false)
    expect(envelope.success, envelope.errorMessage).toBe(false)
    expect([403, 409, 422], "必须拒绝完整业务命令，不得依赖 schema 或 multipart 错误").toContain(response.status())
    expect(envelope.errorMessage).toMatch(message)
    expect({
        request: monetaryFacts(await detail(readerToken, request.id)),
        approved: monetaryFacts(await detail(readerToken, approved.id)),
        candidates: await candidates(readerToken),
    }, "拒绝付款必须保持实际付款、逐行净已付、应付余额及占用不变").toEqual(before)
}

async function submit(page: Page, request: PurchasePaymentRequestResult) {
    return purchasePaymentRequestCommand<PurchasePaymentRequestResult>(page, "caigou", "POST", `${ROOT}/${request.id}/submit`, {
        expected_version: request.version, idempotency_key: randomUUID(),
    })
}

/** 另一账号审批或 API 冲正后，重新加载并打开该申请绑定的原出纳任务。 */
async function openPaymentRequestTask(page: Page, token: string, request: Detail) {
    expect(["approved", "partially_paid"]).toContain(request.status)
    expect(request.work_item_id, "已批准申请必须绑定正式出纳任务").toBeTruthy()
    const taskId = request.work_item_id!
    const profile = await apiGet<{ userid: string; account: string }>(token, "/account/profile")
    const authority = await apiGet<{
        id: string; work_item_type: string; business_object_type: string; business_object_id: string;
        owner_user_id: string | null; status: string; task_version: string; subject_version: string; allowed_actions: string[];
    }>(token, `/admin/work-items/${taskId}`)
    expect(profile.account).toBe("fukuan")
    expect(authority).toMatchObject({
        id: taskId, work_item_type: "SUPPLIER_PAYMENT_EXECUTION", business_object_type: "purchase_payment_request",
        business_object_id: request.id, owner_user_id: profile.userid, status: "OPEN", subject_version: String(request.submission_version),
    })
    expect(authority.task_version).toMatch(/^\d+$/)
    expect(authority.allowed_actions).toContain("PROCESS")
    const current = await detail(token, request.id)
    expect(current).toMatchObject({
        id: request.id, request_no: request.request_no, work_item_id: taskId, supplier_id: request.supplier_id,
        submission_version: request.submission_version, version: request.version, status: request.status,
        data: request.data, payee_account: request.payee_account,
    })
    expect(moneyCents(current.total_amount)).toBe(moneyCents(request.total_amount))
    const frozenLines = (value: Detail) => value.lines.map(({ paid_amount: _paid, closed_amount: _closed, ...line }) => ({
        ...line, requested_amount: moneyCents(line.requested_amount),
    }))
    expect(frozenLines(current), "原申请逐行身份、批准金额和冻结条件必须保持不变").toEqual(frozenLines(request))
    const context = await apiGet<Detail>(token, `/admin/work-items/${taskId}/purchase-payment-request`)
    expect(context).toMatchObject({
        id: current.id, request_no: current.request_no, work_item_id: taskId,
        submission_version: current.submission_version, version: current.version, status: current.status,
    })
    expect(exactLineBalances(context), "任务上下文必须与即时授权详情的逐行金额、当前占用和应付分录完全一致").toEqual(exactLineBalances(current))
    await page.goto(`/workspace?family=finance&currentWorkItemId=${encodeURIComponent(taskId)}`)
    const segment = taskId.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")
    const task = page.locator(`[id="workspace-task-${segment}"]`)
    await expect(task).toHaveCount(1, VISIBLE)
    await expect(task).toBeVisible(VISIBLE)
    await expect(task).toContainText(request.request_no)
    await task.click()
    await expect(task).toHaveAttribute("aria-current", "true", VISIBLE)
    await expect.poll(() => new URL(page.url()).searchParams.get("currentWorkItemId"), VISIBLE).toBe(taskId)
    const surface = page.getByRole("region", { name: "当前工作台任务" })
    await expect(surface.getByRole("region", { name: "付款任务摘要" })).toBeVisible(VISIBLE)
    await expect(surface.locator("#ppr-pay-form")).toBeVisible(VISIBLE)
    await expect(surface.locator("#ppr-pay-amount")).toHaveValue(money(context.lines.reduce((total, line) =>
        total + moneyCents(line.requested_amount) - moneyCents(line.paid_amount) - moneyCents(line.closed_amount), 0n)), VISIBLE)
}

function uiPaymentCommand(response: Response) {
    const encoded = /name="command"\r\n(?:Content-Type:[^\r]+\r\n)?\r\n([^\r]+)/.exec(response.request().postData() ?? "")
    expect(encoded, "真实 UI 付款必须包含完整 multipart 命令").toBeTruthy()
    return JSON.parse(encoded![1]!) as {
        payment_request_id: string; work_item_id: string; idempotency_key: string; payment: { amount: string };
    }
}

async function commitUiPayment(page: Page, amount: string, filename: string, loseResponse = false) {
    await page.locator("#ppr-pay-amount").fill(amount)
    await page.locator("#ppr-pay-bank-receipt-input").setInputFiles({ ...BANK_RECEIPT_FILE, name: filename })
    await page.locator("#ppr-pay-submit").click()
    const dialog = page.getByRole("alertdialog").filter({ hasText: "确认登记付款" })
    await expect(dialog).toBeVisible(VISIBLE)
    if (loseResponse) {
        type SavedCommand = { url: string; headers: Record<string, string>; bytes: Buffer; result: { id: string; payment_no: string; amount: string; status: string } }
        let settle: (command: SavedCommand) => void = () => undefined
        const applied = new Promise<SavedCommand>((resolve) => { settle = resolve })
        const match = "**/admin/purchase-payment-requests/*/payments/commit"
        await page.route(match, async (route) => {
            const request = route.request()
            const source = new URL(request.url())
            const commandUrl = `${API_BASE}${source.pathname}${source.search}`
            const server = await route.fetch({ url: commandUrl })
            expect(server.ok(), await server.text()).toBe(true)
            const result = (await server.json()).data as SavedCommand["result"]
            const bytes = request.postDataBuffer()
            expect(bytes, "恢复必须保留原命令和回单请求").not.toBeNull()
            settle({ url: commandUrl, headers: await request.allHeaders(), bytes: bytes!, result })
            await route.abort("failed")
        }, { times: 1 })
        await dialog.locator("#ppr-pay-confirm-confirm").click()
        const original = await applied
        await page.unroute(match)
        // PPR-12：服务端已经提交，客户端响应丢失；同一命令及原幂等键恢复同一付款。
        const replay = await page.request.post(original.url, { headers: original.headers, data: original.bytes, timeout: 60_000 })
        expect(replay.ok(), await replay.text()).toBe(true)
        const recovered = (await replay.json()).data as SavedCommand["result"]
        expect(recovered.id).toBe(original.result.id)
        expect(recovered.payment_no).toBe(original.result.payment_no)
        expect(recovered.amount).toBe(original.result.amount)
        const forbiddenLegacy = await page.request.post(`${API_BASE}/admin/supplier-payments/commit`, {
            headers: original.headers, data: original.bytes, timeout: 60_000,
        })
        expect(forbiddenLegacy.ok(), "有效申请付款命令也不得通过旧直付接口执行").toBe(false)
        expect(forbiddenLegacy.status(), "必须是业务拒绝，不得仅依赖错误 Content-Type").not.toBe(415)
        return recovered
    }
    const cashierToken = await apiToken("fukuan")
    const readerToken = await apiToken("caigou")
    const workItemId = new URL(page.url()).searchParams.get("currentWorkItemId")
    expect(workItemId, "正式付款须锁定当前出纳任务").toBeTruthy()
    const request = await apiGet<Detail>(cashierToken, `/admin/work-items/${workItemId}/purchase-payment-request`)
    const readback = async () => ({
        request: monetaryFacts(await detail(cashierToken, request.id)),
        candidates: await candidates(readerToken),
    })
    const before = await readback()
    const waitForCommit = () => page.waitForResponse((response) => response.request().method() === "POST" &&
        new URL(response.url()).pathname === `${ROOT}/${request.id}/payments/commit`, { timeout: 60_000 })
    const committed = waitForCommit()
    await dialog.locator("#ppr-pay-confirm-confirm").click()
    let response = await committed
    const originalCommand = uiPaymentCommand(response)
    expect(originalCommand).toMatchObject({ payment_request_id: request.id, work_item_id: workItemId })
    expect(originalCommand.idempotency_key).toMatch(/^ppr-pay:/)
    expect(moneyCents(originalCommand.payment.amount)).toBe(moneyCents(amount))
    if (response.status() === 429) {
        expect(await response.json()).toMatchObject({ status: 429, code: "RATE_LIMITED", retryable: true, success: false, data: null })
        const retryAfter = response.headers()["retry-after"]
        expect(retryAfter, "准入拒绝必须给出明确等待秒数").toMatch(/^[1-9]\d*$/)
        expect(Number(retryAfter), "上传滑动窗口为 60 秒").toBeLessThanOrEqual(60)
        expect(await readback(), "handler 前限流拒绝不得写入付款、核销或申请占用").toEqual(before)
        await expect(dialog).toBeVisible(VISIBLE)
        await expect(dialog.getByText("请求过于频繁，请稍后重试", { exact: true })).toBeVisible(VISIBLE)
        await expect(dialog.locator("#ppr-pay-confirm-confirm")).toBeEnabled(VISIBLE)
        // 所有上传入口按账号共享 10 次/60 秒。释放完整窗口，后续真实关闭/付款竞争也必须获准进入 handler。
        await page.waitForTimeout(60_000)
        await page.waitForTimeout(250)
        expect(await readback(), "等待期间原申请、已付款事实及应付占用必须完全不变").toEqual(before)
        const retried = waitForCommit()
        await dialog.locator("#ppr-pay-confirm-confirm").click()
        response = await retried
        expect(uiPaymentCommand(response), "真实 UI 重试必须沿用原幂等键、任务版本、逐项分配和回单引用").toEqual(originalCommand)
    }
    expect(response.ok(), await response.text()).toBe(true)
    return (await response.json()).data as { id: string; payment_no: string; amount: string; status: string }

}

test("采购付款申请原子占用、拒绝旧直付、释放重提、冻结多来源及部分付款关闭", async ({ browser }) => {
    test.setTimeout(12 * 60 * 1000)
    const procurement = await openLoggedInWorkspace(browser, "caigou")
    const salesToken = await apiToken("xiaoshou")
    const adminToken = await apiToken("admin")
    const procurementToken = await apiToken("caigou")
    const fixture = await prepareAnalyticsOrders(salesToken, adminToken, procurement.page, `PPR${Date.now().toString(36)}`)
    const suppliers = await apiGet<{ items: Array<{ id: string; legal_name: string }> }>(adminToken, "/admin/suppliers", { keyword: "北京安达家电服务有限公司", page_size: 100 })
    const supplier = suppliers.items.find((item) => item.legal_name === "北京安达家电服务有限公司")
    expect(supplier, "正式服务供给应引用固定供应商，不新增伪造财务来源").toBeTruthy()
    const purchases: AnalyticsPurchase[] = []
    const periodByPurchase = new Map<string, Awaited<ReturnType<typeof revisePeriodProfile>>>()
    for (const [index, order] of fixture.orders.entries()) {
        const period = await revisePeriodProfile(procurement.page, adminToken, supplier!.id, index === 0 ? "QUARTER" : "YEAR")
        const purchase = await createAnalyticsPurchase(order, procurement.page, procurementToken)
        const frozenPurchase = await apiGet<{ supplier_id: string; payment_term_code: string }>(procurementToken, `/admin/purchase-orders/${purchase.purchaseId}`)
        expect(frozenPurchase).toMatchObject({ supplier_id: supplier!.id, payment_term_code: period.code })
        purchases.push(purchase)
        periodByPurchase.set(purchase.purchaseId, period)
    }
    const currentSupplier = await apiGet<SupplierPeriodProfile>(adminToken, `/admin/suppliers/${supplier!.id}`)
    for (const period of periodByPurchase.values()) {
        expect(currentSupplier.commercial_profiles.find((profile) => profile.id === period.profileId)).toMatchObject({
            settlement_mode: period.mode, reconciliation_cycle: "monthly", payment_term_snapshot: period.code,
        })
    }
    const rows = (await apiGet<Array<PaymentRequestCandidate & { payment_terms: { payment_term_code: string; prepay_gate: boolean } }>>(procurementToken, `${ROOT}/candidates`))
        .filter((row) => purchases.some((purchase) => purchase.purchaseId === row.purchase_order_id))
    expect(rows).toHaveLength(2)
    expect(new Set(rows.map((row) => row.supplier_id)).size).toBe(1)
    expect(new Set(rows.map((row) => row.payer_party_id)).size).toBe(1)
    expect(rows.every((row) => row.payer_party_id && row.payer_party_name)).toBe(true)
    for (const row of rows) expect(row.payment_terms).toMatchObject({
        payment_term_code: periodByPurchase.get(row.purchase_order_id)!.code, prepay_gate: false,
    })

    // PPR-01/08：只有应付不会派发出纳任务，旧入口不能登记付款。
    const cashier = await openLoggedInWorkspace(browser, "fukuan")
    await cashier.page.goto("/workspace?family=finance")
    await expect(cashier.page.getByRole("button", { name: /供应商付款处理|待付款申请/ })).toHaveCount(0)
    const legacy = await rawCommand(cashier.page, await apiToken("fukuan"), "/admin/supplier-payments/commit", {})
    expect(legacy.ok(), "采购应付旧直付入口必须拒绝").toBe(false)

    // PPR-05：草稿不占额；两个同余额申请并发提交只允许一个成功。
    const originalEvidence = await uploadAnalyticsEvidence(procurementToken)
    const originalData = draftData([rows[0]!])
    originalData.data.attachment_ids.push(originalEvidence)
    const draftA = await purchasePaymentRequestCommand<PurchasePaymentRequestResult>(procurement.page, "caigou", "POST", ROOT, originalData)
    const draftB = await purchasePaymentRequestCommand<PurchasePaymentRequestResult>(procurement.page, "caigou", "POST", ROOT, originalData)
    expect((await candidates(procurementToken)).find((row) => row.payable_account_id === rows[0]!.payable_account_id)?.available_total).toBe(rows[0]!.available_total)
    const submitted = await Promise.all([draftA, draftB].map((draft) => rawCommand(procurement.page, procurementToken, `${ROOT}/${draft.id}/submit`, {
        expected_version: draft.version, idempotency_key: randomUUID(),
    })))
    expect(submitted.filter((response) => response.ok())).toHaveLength(1)
    const winnerIndex = submitted[0]!.ok() ? 0 : 1
    const winner = (await submitted[winnerIndex]!.json()).data as Detail
    const blocked = (await submitted[1 - winnerIndex]!.json()) as { success: boolean }
    expect(blocked.success).toBe(false)
    expect(winner.status).toBe("in_approval")
    const occupied = (await candidates(procurementToken)).find((row) => row.payable_account_id === rows[0]!.payable_account_id)
    // 候选入口可过滤没有新增可申请余额的采购。
    if (occupied) expect(occupied.available_total).toMatch(/^0(?:\.0+)?$/)

    // PPR-06：撤回取消旧审批且释放，重提冻结新提交；驳回保持终止，不自动开新轮次。
    await purchasePaymentRequestCommand(procurement.page, "caigou", "POST", `${ROOT}/${winner.id}/withdraw`, {
        expected_version: winner.version, reason: "对账说明待补充，撤回本次申请", idempotency_key: randomUUID(),
    })
    const withdrawn = await detail(procurementToken, winner.id)
    expect(withdrawn.status).toBe("withdrawn")
    expect((await candidates(procurementToken)).find((row) => row.payable_account_id === rows[0]!.payable_account_id)?.available_total).toBe(rows[0]!.available_total)
    const changedEvidence = await uploadAnalyticsEvidence(procurementToken)
    const changedData = draftData([rows[0]!])
    changedData.data.reason = "撤回后逐单复核，仅申请原金额的一半并替换本次对账附件"
    changedData.data.lines[0]!.amount = money(moneyCents(rows[0]!.available_total) / 2n)
    changedData.data.attachment_ids.push(changedEvidence)
    const edited = await purchasePaymentRequestCommand<PurchasePaymentRequestResult>(procurement.page, "caigou", "PUT", `${ROOT}/${winner.id}`, {
        expected_version: withdrawn.version, ...changedData,
    })
    const resubmitted = await submit(procurement.page, edited)
    const refreshed = await detail(procurementToken, winner.id)
    expect(refreshed.submission_version).toBeGreaterThan(winner.submission_version)
    expect(refreshed.approval_instance_id).not.toBe(winner.approval_instance_id)
    expect(refreshed.data.reason).toBe(changedData.data.reason)
    expect(refreshed.lines[0]!.requested_amount).toBe(changedData.data.lines[0]!.amount)
    const originalSubmission = await apiGet<Detail>(procurementToken, `${ROOT}/${winner.id}/submissions/1`)
    expect(originalSubmission.data).toEqual(originalData.data)
    expect(originalSubmission.lines[0]!.requested_amount).toBe(rows[0]!.available_total)
    expect(originalSubmission.approval_instance_id).toBe(winner.approval_instance_id)
    expect(originalSubmission.attachments.map((attachment) => attachment.file_asset_id)).toContain(originalEvidence)
    expect(originalSubmission.attachments.map((attachment) => attachment.file_asset_id)).not.toContain(changedEvidence)
    const historicalEvidence = await procurement.page.request.get(`${API_BASE}${ROOT}/${winner.id}/materials/${originalEvidence}/download?submission_version=1`, {
        headers: { Authorization: `Bearer ${procurementToken}` },
    })
    expect(historicalEvidence.ok(), await historicalEvidence.text()).toBe(true)
    expect((await historicalEvidence.body()).length).toBeGreaterThan(0)
    const forbiddenHistoricalEvidence = await procurement.page.request.get(`${API_BASE}${ROOT}/${winner.id}/materials/${changedEvidence}/download?submission_version=1`, {
        headers: { Authorization: `Bearer ${procurementToken}` },
    })
    expect(forbiddenHistoricalEvidence.ok(), "历史提交材料不能包含后来替换的附件").toBe(false)
    const finance = await openLoggedInWorkspace(browser, "caiwu")
    await openWorkspaceTask(finance.page, /付款申请/, resubmitted.request_no, "approval")
    await rejectFinancialOriginal(finance.page, "本期对账金额需由采购重新核对")
    const rejected = await detail(procurementToken, winner.id)
    expect(rejected.status).toBe("rejected")
    await finance.page.goto("/workspace?family=approval")
    await expect(finance.page.getByRole("button", { name: /付款申请/ }).filter({ hasText: winner.request_no })).toHaveCount(0)
    expect((await candidates(procurementToken)).find((row) => row.payable_account_id === rows[0]!.payable_account_id)?.available_total).toBe(rows[0]!.available_total)

    // 为未批准状态准入保留一条真实审批中申请；之后使用真实已批准付款任务发送完整命令。
    const pendingData = draftData([rows[0]!])
    pendingData.data.lines[0]!.amount = "1.00"
    const pendingDraft = await purchasePaymentRequestCommand<PurchasePaymentRequestResult>(procurement.page, "caigou", "POST", ROOT, pendingData)
    await submit(procurement.page, pendingDraft)
    const multiAmounts = new Map(rows.map((row, index) => [row.payable_account_id, index === 0 ? money(moneyCents(row.available_total) / 2n) : row.available_total]))

    // PPR-02：浏览器勾选采购、核对预填金额后批量提交，同一付款关系合并为一份正式申请。
    const availableForBatch = await candidates(procurementToken)
    await procurement.page.goto("/procurement/payment-requests")
    await procurement.page.locator("#purchase-payment-requests-create").click()
    for (const row of rows) {
        const segment = row.payable_account_id.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")
        await procurement.page.locator(`[id="ppr-select-${segment}"]`).check()
    }
    await procurement.page.locator("#ppr-create-next").click()
    await expect(procurement.page.locator("#ppr-create-submit")).toHaveText("提交 1 份付款申请")
    const amountInputs = procurement.page.locator('input[id^="ppr-create-line-"]')
    await expect(amountInputs).toHaveCount(2, VISIBLE)
    for (const row of rows) {
        const input = procurement.page.locator(`[id="ppr-create-line-${row.payable_account_id.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")}"]`)
        const current = availableForBatch.find((candidate) => candidate.payable_account_id === row.payable_account_id)!
        await expect(input, "周期结算按当前可申请余额预填，不得覆盖已存在的申请占用").toHaveValue(money(moneyCents(current.available_total)))
        await input.fill(multiAmounts.get(row.payable_account_id)!)
    }
    await pickCalendarDay(procurement.page, procurement.page.locator("#ppr-create-desired-payment-date"), analyticsDate())
    await procurement.page.locator("#ppr-create-description").fill("采购已完成本期对账，按两张采购分别申请")
    await procurement.page.locator("#ppr-create-attachments").setInputFiles({ ...BANK_RECEIPT_FILE, name: "ppr-own-draft-evidence.png" })
    const saved = procurement.page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `${ROOT}/batch`)
    await procurement.page.locator("#ppr-create-submit").click()
    const batchResponse = await saved
    expect(batchResponse.ok(), await batchResponse.text()).toBe(true)
    const batchRequests = (await batchResponse.json()).data as Detail[]
    expect(batchRequests).toHaveLength(1)
    const batchRequest = batchRequests[0]!
    expect(batchRequest.status, "新建入口须直接完成财审提交").toBe("in_approval")
    expect(batchRequest.lines.map((line) => line.payable_account_id).sort()).toEqual(rows.map((row) => row.payable_account_id).sort())
    await procurement.page.locator(`#ppr-result-${batchRequest.id.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")}`).click()
    // 已提交申请正式撤回后沿原单修改，继续覆盖草稿取消确认、附件归属及重新提交的未知结果恢复。
    await purchasePaymentRequestCommand(procurement.page, "caigou", "POST", `${ROOT}/${batchRequest.id}/withdraw`, {
        expected_version: batchRequest.version, reason: "补充本次对账说明后沿原申请重新提交", idempotency_key: randomUUID(),
    })
    const multi = await detail(procurementToken, batchRequest.id)
    expect(multi.status).toBe("withdrawn")
    await procurement.page.reload()
    const ownDraftAttachmentIds = [...multi.data.attachment_ids]
    expect(ownDraftAttachmentIds, "正常上传必须保存本申请自有附件身份").toHaveLength(1)
    const initialDraftView = await detail(procurementToken, multi.id)
    const inheritedAttachmentIds = initialDraftView.attachments.map((file) => file.file_asset_id)
        .filter((id) => !ownDraftAttachmentIds.includes(id))
    expect(inheritedAttachmentIds, "完整材料展示必须包含正式销售来源附件").toContain(fixture.evidenceId)
    const draftMaterials = procurement.page.getByRole("region", { name: "冻结采购销售与审核材料", exact: true })
    await expect(draftMaterials.getByText("ppr-own-draft-evidence.png", { exact: true })).toBeVisible(VISIBLE)
    const sourceEvidenceSegment = fixture.evidenceId.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")
    await expect(draftMaterials.locator(`[id="ppr-attachment-${sourceEvidenceSegment}-download"]`)).toBeVisible(VISIBLE)
    await expect(procurement.page.locator("#ppr-detail-submit")).toBeVisible(VISIBLE)
    await procurement.page.locator("#ppr-detail-submit").click()
    // Review D：未发送的确认取消后允许修改草稿；第一次真实提交必须使用保存后的版本。
    await procurement.page.locator("#ppr-submit-confirm-cancel").click()
    await procurement.page.locator("#ppr-detail-edit").click()
    const revisedReason = "采购已重新核对两张采购，本次按最新草稿提交财务审核"
    await procurement.page.locator("#ppr-create-description").fill(revisedReason)
    const editedResponsePending = procurement.page.waitForResponse((response) => response.request().method() === "PUT" &&
        new URL(response.url()).pathname === `${ROOT}/${multi.id}`)
    await procurement.page.locator("#ppr-create-submit").click()
    const editedResponse = await editedResponsePending
    expect(editedResponse.ok(), await editedResponse.text()).toBe(true)
    const revisedDraft = (await editedResponse.json()).data as Detail
    expect(revisedDraft.version).toBeGreaterThan(multi.version)
    expect(revisedDraft.data.reason).toBe(revisedReason)
    expect(revisedDraft.data.attachment_ids, "编辑必须完整保留本申请自有附件").toEqual(ownDraftAttachmentIds)
    expect(revisedDraft.data.attachment_ids.some((id) => inheritedAttachmentIds.includes(id)), "来源附件只供完整材料展示，不得混入草稿命令自有附件").toBe(false)
    expect(editedResponse.request().postDataJSON().data.attachment_ids).toEqual(ownDraftAttachmentIds)
    await procurement.page.locator(`#ppr-result-${multi.id.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "")}`).click()
    await expect(procurement.page.locator("#ppr-create-dialog")).toBeHidden(VISIBLE)
    // 服务端先提交成功，再丢失响应；未知结果必须查询原次命令，而不能改草稿或新建审批。
    let firstSubmitCommand: unknown
    let completeSubmission: (result: { ok: boolean; success: boolean }) => void = () => undefined
    const appliedSubmission = new Promise<{ ok: boolean; success: boolean }>((resolve) => { completeSubmission = resolve })
    const submitRoute = `**${ROOT}/${multi.id}/submit`
    await procurement.page.route(submitRoute, async (route) => {
        firstSubmitCommand = route.request().postDataJSON()
        const response = await route.fetch({ url: `${API_BASE}${ROOT}/${multi.id}/submit` })
        const envelope = await response.json() as { success: boolean }
        completeSubmission({ ok: response.ok(), success: envelope.success })
        await route.abort("failed")
    }, { times: 1 })
    await procurement.page.locator("#ppr-detail-submit").click()
    await procurement.page.locator("#ppr-submit-confirm-confirm").click()
    expect(await appliedSubmission, "取消后修改的草稿第一次实际提交必须成功，不能触发旧版本冲突").toEqual({ ok: true, success: true })
    expect(firstSubmitCommand).toMatchObject({ expected_version: revisedDraft.version })
    await expect.poll(async () => (await detail(procurementToken, multi.id)).status).toBe("in_approval")
    const applied = await detail(procurementToken, multi.id)
    await expect(procurement.page.getByText("提交结果暂无法确认，请再次确认本次提交以查询原次结果。", { exact: true })).toBeVisible(VISIBLE)
    await procurement.page.locator("#ppr-submit-confirm-cancel").click()
    await expect(procurement.page.locator("#ppr-detail-edit")).toBeDisabled(VISIBLE)
    await procurement.page.locator("#ppr-detail-submit").click()
    const retriedSubmission = procurement.page.waitForResponse((response) => response.request().method() === "POST" &&
        new URL(response.url()).pathname === `${ROOT}/${multi.id}/submit`)
    await procurement.page.locator("#ppr-submit-confirm-confirm").click()
    const retriedResponse = await retriedSubmission
    expect(retriedResponse.request().postDataJSON(), "未知结果重试必须沿用原版本和幂等键").toEqual(firstSubmitCommand)
    expect(retriedResponse.ok(), await retriedResponse.text()).toBe(true)
    expect((await retriedResponse.json()).success).toBe(true)
    await procurement.page.unroute(submitRoute)
    const frozen = await detail(procurementToken, multi.id)
    expect(frozen.approval_instance_id, "未知结果恢复必须返回同一个审批实例").toBe(applied.approval_instance_id)
    expect(frozen.version).toBe(applied.version)
    expect(exactLineBalances(frozen), "查询已成功的原次提交不得重复占用逐行额度").toEqual(exactLineBalances(applied))
    for (const line of frozen.lines) expect(line.payment_terms).toMatchObject({
        payment_term_code: periodByPurchase.get(line.purchase_order_id)!.code, prepay_gate: false,
    })
    expect(frozen.purchase_previews.map((row) => row.purchase_order_id).sort()).toEqual(purchases.map((row) => row.purchaseId).sort())
    expect(new Set(frozen.sales_previews.map((row) => row.sales_order_id)).size).toBe(2)
    expect(frozen.total_amount).toBe(money([...multiAmounts.values()].reduce((total, amount) => total + moneyCents(amount), 0n)))
    await openWorkspaceTask(finance.page, /付款申请/, multi.request_no, "approval")
    for (const row of rows) await expect(finance.page.getByText(row.purchase_no).first()).toBeVisible(VISIBLE)
    await approveCurrentDocument(finance.page)
    const approved = await detail(procurementToken, multi.id)
    expect(approved.status).toBe("approved")
    const amended = await procurement.page.request.put(`${API_BASE}${ROOT}/${multi.id}`, {
        headers: { Authorization: `Bearer ${procurementToken}` },
        data: { ...draftData(rows), expected_version: approved.version },
    })
    expect(amended.ok(), "批准后不能直接改写逐单金额和审核依据").toBe(false)
    expect((await detail(procurementToken, multi.id)).version).toBe(approved.version)

    // PPR-08：完整 DTO 和真实回单仍不能让草稿、审批中、已驳回或已撤回申请产生付款。
    const cashierToken = await apiToken("fukuan")
    const validFirstAllocation = allocationsFor(approved, rows[0]!.payable_account_id, "1.00")
    const loser = winnerIndex === 0 ? draftB : draftA
    for (const requestId of [loser.id, pendingDraft.id, winner.id]) {
        const unapproved = await detail(procurementToken, requestId)
        expect(["draft", "in_approval", "rejected"]).toContain(unapproved.status)
        await expectPaymentRejected(cashier.page, cashierToken, procurementToken, unapproved, approved, validFirstAllocation, /申请|批准|任务/)
    }
    const pendingBeforeWithdraw = await detail(procurementToken, pendingDraft.id)
    await purchasePaymentRequestCommand(procurement.page, "caigou", "POST", `${ROOT}/${pendingDraft.id}/withdraw`, {
        expected_version: pendingBeforeWithdraw.version, reason: "状态准入验证后撤回未批准申请", idempotency_key: randomUUID(),
    })
    const pendingWithdrawn = await detail(procurementToken, pendingDraft.id)
    expect(pendingWithdrawn.status).toBe("withdrawn")
    await expectPaymentRejected(cashier.page, cashierToken, procurementToken, pendingWithdrawn, approved, validFirstAllocation, /申请|批准|任务/)

    // PPR-07/09：只付款第一行的一半，第二行批准额度不能被挪用；剩余占用随关闭释放。
    await openPaymentRequestTask(cashier.page, cashierToken, approved)
    const firstLine = approved.lines[0]!
    const half = money(moneyCents(firstLine.requested_amount) / 2n)
    const payment = await commitUiPayment(cashier.page, half, "ppr-partial-bank-receipt.png", true)
    expect(payment.status).toBe("posted")
    let partial = await detail(procurementToken, multi.id)
    expect(partial.status).toBe("partially_paid")
    expect(partial.lines.find((line) => line.payable_account_id === firstLine.payable_account_id)!.paid_amount).toBe(half)
    expect(partial.lines.find((line) => line.payable_account_id !== firstLine.payable_account_id)!.paid_amount).toMatch(/^0(?:\.0+)?$/)
    expect(partial.work_item_id).toBe(approved.work_item_id)
    for (const row of await candidates(procurementToken)) {
        const source = rows.find((source) => source.payable_account_id === row.payable_account_id)
        if (source) expect(row.available_total).toBe(money(moneyCents(source.available_total) - moneyCents(multiAmounts.get(row.payable_account_id)!)))
    }
    const remainingFirst = moneyCents(firstLine.requested_amount) - moneyCents(half)
    const excessiveFirst = money(remainingFirst + 1n)
    expect(moneyCents(partial.current_lines.find((line) => line.payable_account_id === firstLine.payable_account_id)!.current_open_total)).toBeGreaterThan(moneyCents(excessiveFirst))
    // 单行金额仍在该采购当前应付内，但超过本申请该行尚未支付授权，必须业务拒绝。
    await expectPaymentRejected(cashier.page, cashierToken, procurementToken, partial, partial,
        allocationsFor(partial, firstLine.payable_account_id, excessiveFirst), /金额|批准|额度|超过|范围/)
    const secondLine = partial.lines.find((line) => line.payable_account_id !== firstLine.payable_account_id)!
    const remainingSecond = moneyCents(secondLine.requested_amount) - moneyCents(secondLine.paid_amount)
    expect(remainingSecond).toBeGreaterThan(1n)
    // 合计等于本申请全部未付批准额度，仍不能从第二采购挪一分钱到第一采购。
    const shifted = [
        ...allocationsFor(partial, firstLine.payable_account_id, excessiveFirst),
        ...allocationsFor(partial, secondLine.payable_account_id, money(remainingSecond - 1n)),
    ]
    expect(shifted.reduce((sum, allocation) => sum + moneyCents(allocation.allocated_amount), 0n)).toBe(remainingFirst + remainingSecond)
    await expectPaymentRejected(cashier.page, cashierToken, procurementToken, partial, partial, shifted, /金额|批准|额度|超过|范围/)

    // PPR-14：未关闭申请的部分付款真实冲正，恢复原批准额度并保留原开放出纳任务身份。
    const taskBeforeReversal = await apiGet<{
        id: string; status: string; owner_user_id: string | null; subject_version: string; task_version: string;
        business_object_type: string; business_object_id: string;
    }>(cashierToken, `/admin/work-items/${partial.work_item_id}`)
    expect(taskBeforeReversal).toMatchObject({
        id: approved.work_item_id, status: "OPEN", business_object_type: "purchase_payment_request",
        business_object_id: multi.id, subject_version: String(approved.submission_version),
    })
    const candidatesBeforeReversal = await candidates(procurementToken)
    const reversal = await purchasePaymentRequestCommand<{ id: string; reversal_no: string }>(
        cashier.page, "fukuan", "POST", "/admin/payment-reversals/commit", {
            source_fact_id: payment.id, amount: null,
            reason: "E2E 未关闭付款申请的部分付款原分配冲正并重新支付", idempotency_key: randomUUID(),
        },
    )
    await openWorkspaceTask(procurement.page, /付款冲正审批/, reversal.reversal_no, "approval")
    await approveCurrentDocument(procurement.page)
    await openWorkspaceTask(finance.page, /付款冲正审批/, reversal.reversal_no, "approval")
    await approveCurrentDocument(finance.page)
    expect((await apiGet<{ status: string }>(cashierToken, `/admin/payment-reversals/${reversal.id}`)).status).toBe("posted")
    const restoredApproved = await detail(procurementToken, multi.id)
    expect(restoredApproved.status).toBe("approved")
    expect(restoredApproved.version).toBeGreaterThan(partial.version)
    expect(restoredApproved.submission_version).toBe(approved.submission_version)
    expect(restoredApproved.work_item_id).toBe(approved.work_item_id)
    for (const line of restoredApproved.lines) {
        expect(moneyCents(line.paid_amount)).toBe(0n)
        expect(moneyCents(line.closed_amount)).toBe(0n)
        const before = partial.current_lines.find((row) => row.payable_account_id === line.payable_account_id)!
        const after = restoredApproved.current_lines.find((row) => row.payable_account_id === line.payable_account_id)!
        const restoredAmount = line.payable_account_id === firstLine.payable_account_id ? moneyCents(half) : 0n
        expect(moneyCents(after.current_open_total)).toBe(moneyCents(before.current_open_total) + restoredAmount)
        expect(moneyCents(after.current_settled_total)).toBe(moneyCents(before.current_settled_total) - restoredAmount)
        expect(moneyCents(after.current_reserved_total)).toBe(moneyCents(before.current_reserved_total) + restoredAmount)
        expect(moneyCents(after.current_available_total)).toBe(moneyCents(before.current_available_total))
    }
    const reversedPayment = restoredApproved.payments.find((row) => row.payment_id === payment.id)
    expect(reversedPayment, "原实际付款应保留且净核销归零").toBeTruthy()
    expect(moneyCents(reversedPayment!.effective_amount)).toBe(0n)
    const candidatesAfterReversal = await candidates(procurementToken)
    for (const row of candidatesBeforeReversal) {
        if (!rows.some((source) => source.payable_account_id === row.payable_account_id)) continue
        const after = candidatesAfterReversal.find((item) => item.payable_account_id === row.payable_account_id)
        expect(after, "冲正恢复批准占用不得改变可新申请余额").toBeTruthy()
        expect(moneyCents(after!.available_total)).toBe(moneyCents(row.available_total))
    }
    const taskAfterReversal = await apiGet<typeof taskBeforeReversal>(cashierToken, `/admin/work-items/${restoredApproved.work_item_id}`)
    expect(taskAfterReversal).toMatchObject({
        id: taskBeforeReversal.id, status: "OPEN", owner_user_id: taskBeforeReversal.owner_user_id,
        business_object_type: "purchase_payment_request", business_object_id: multi.id,
        subject_version: taskBeforeReversal.subject_version,
    })
    expect(BigInt(taskAfterReversal.task_version)).toBeGreaterThan(BigInt(taskBeforeReversal.task_version))
    await openPaymentRequestTask(cashier.page, cashierToken, restoredApproved)
    const replacement = await commitUiPayment(cashier.page, half, "ppr-repayment-bank-receipt.png")
    expect(replacement.status).toBe("posted")
    expect(replacement.id).not.toBe(payment.id)
    const beforeRepayment = partial
    partial = await detail(procurementToken, multi.id)
    expect(partial.status).toBe("partially_paid")
    expect(partial.work_item_id).toBe(approved.work_item_id)
    expect(partial.submission_version).toBe(approved.submission_version)
    expect(exactLineBalances(partial)).toEqual(exactLineBalances(beforeRepayment))
    const replacementPayment = partial.payments.find((row) => row.payment_id === replacement.id)
    expect(replacementPayment).toBeTruthy()
    expect(moneyCents(replacementPayment!.effective_amount)).toBe(moneyCents(half))
    // 即使再次付款恰好利用一个已过期槽而未触发 429，也须释放完整上传窗口再验证业务竞争。
    const raceReadback = async () => ({
        request: monetaryFacts(await detail(procurementToken, multi.id)),
        candidates: await candidates(procurementToken),
    })
    const beforeRaceWindow = await raceReadback()
    await test.step("关闭和付款竞争前释放完整上传窗口且财务事实保持不变", async () => {
        await cashier.page.waitForTimeout(60_000)
        await cashier.page.waitForTimeout(250)
        expect(await raceReadback(), "等待准入窗口不得改变付款、逐行净已付、应付或申请占用").toEqual(beforeRaceWindow)
    })
    await openPaymentRequestTask(cashier.page, cashierToken, partial)
    await cashier.page.locator("#ppr-pay-amount").fill(money(moneyCents(firstLine.requested_amount) - moneyCents(half)))
    await cashier.page.locator("#ppr-pay-bank-receipt-input").setInputFiles({ ...BANK_RECEIPT_FILE, name: "ppr-close-payment-race.png" })
    await cashier.page.locator("#ppr-pay-submit").click()
    let settled: (responses: boolean[]) => void = () => undefined
    let raceStatuses: { payment: number; closed: number } | undefined
    const raced = new Promise<boolean[]>((resolve) => { settled = resolve })
    const paymentMatch = "**/admin/purchase-payment-requests/*/payments/commit"
    await cashier.page.route(paymentMatch, async (route) => {
        const source = new URL(route.request().url())
        const [paid, closed] = await Promise.all([
            route.fetch({ url: `${API_BASE}${source.pathname}${source.search}` }),
            rawCommand(procurement.page, procurementToken, `${ROOT}/${multi.id}/close`, {
                expected_version: partial.version,
                reason: "余款计划调整，并发关闭与付款必须互斥", idempotency_key: randomUUID(),
            }),
        ])
        raceStatuses = { payment: paid.status(), closed: closed.status() }
        settled([paid.ok(), closed.ok()])
        await route.fulfill({ response: paid })
    }, { times: 1 })
    await cashier.page.locator("#ppr-pay-confirm-confirm").click()
    const racingResults = await raced
    expect(raceStatuses!.payment, "关闭/付款竞争必须通过真实准入，不能以限流代替财务版本互斥").not.toBe(429)
    expect(raceStatuses!.closed, "关闭/付款竞争必须由业务版本判定").not.toBe(429)
    expect(racingResults.filter(Boolean), "同一版本关闭和付款只能有一个合法结果").toHaveLength(1)
    await cashier.page.unroute(paymentMatch)
    const afterRace = await detail(procurementToken, multi.id)
    if (afterRace.status !== "closed") {
        await purchasePaymentRequestCommand(procurement.page, "caigou", "POST", `${ROOT}/${multi.id}/close`, {
            expected_version: afterRace.version, reason: "余款计划调整，保留已付回单后重新申请", idempotency_key: randomUUID(),
        })
    }
    const closed = await detail(procurementToken, multi.id)
    expect(closed.status).toBe("closed")
    const closedFirst = closed.lines.find((line) => line.payable_account_id === firstLine.payable_account_id)!
    const closedSecond = closed.lines.find((line) => line.payable_account_id === secondLine.payable_account_id)!
    expect(moneyCents(closedFirst.paid_amount)).toBe(moneyCents(afterRace.lines.find((line) => line.payable_account_id === firstLine.payable_account_id)!.paid_amount))
    expect(moneyCents(closedFirst.closed_amount)).toBe(moneyCents(firstLine.requested_amount) - moneyCents(closedFirst.paid_amount))
    expect(moneyCents(closedSecond.closed_amount)).toBe(moneyCents(closedSecond.requested_amount))
    const restored = await candidates(procurementToken)
    for (const line of closed.lines) {
        const candidate = restored.find((row) => row.payable_account_id === line.payable_account_id)
        const initial = rows.find((row) => row.payable_account_id === line.payable_account_id)!
        const available = moneyCents(initial.available_total) - moneyCents(line.requested_amount) + moneyCents(line.closed_amount)
        if (available > 0n) {
            expect(candidate).toBeTruthy()
            expect(moneyCents(candidate!.available_total)).toBe(available)
        } else if (candidate) expect(moneyCents(candidate.available_total)).toBe(0n)
    }
    await cashier.page.goto("/workspace?family=finance")
    await expect(cashier.page.getByRole("button", { name: /供应商付款处理|待付款申请/ }).filter({ hasText: multi.request_no })).toHaveCount(0)
})
