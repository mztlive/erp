/**
 * [flow-35] 付清授权关闭、原付款冲正和改派出纳的恢复资格。
 * 所有业务与账号变更走正式命令；只读临时库核对 API 未暴露的任务责任键。
 */
import { execFileSync } from "node:child_process"
import { randomUUID } from "node:crypto"

import { test, expect, type Page } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { analyticsDate, createAnalyticsPurchase, prepareAnalyticsOrders } from "../helpers/financial-analytics"
import { openLoggedInWorkspace } from "../helpers/login"
import { BANK_RECEIPT_FILE, purchasePaymentRequestCommand, type PaymentRequestCandidate, type PurchasePaymentRequestResult } from "../helpers/payments"
import { approveCurrentDocument, openWorkspaceTask } from "../helpers/ui"

const ROOT = "/admin/purchase-payment-requests"
type Detail = PurchasePaymentRequestResult & {
    supplier_id: string
    payee_account: { account_id: string; version: number }
    payments: Array<{ payment_id: string; amount: string; effective_amount: string }>
    current_lines: Array<{
        payable_account_id: string; current_reserved_total: string; current_open_total: string
        current_available_total: string; current_settled_total: string
    }>
}
type Task = {
    id: string; status: string; owner_user_id: string | null; task_version: string
    subject_version: string; business_object_id: string; impact_summary: string | null
}
type Account = { id: string; account: string; role_ids: string[] }
type Supplier = {
    id: string; party_version: number; version: number; legal_name: string; short_name: string | null
    unified_credit_code: string | null; maintainer_user_id: string; capability_codes: string[]
    capabilities: Array<{ id: string; capability_code: string }>
    bank_accounts: Array<{ id: string; status: string; version: number }>
    qualifications: Array<{
        qualification_type: string; certificate_no: string; issuer: string | null; valid_from: string | null
        valid_to: string | null; attachment_id: string | null; capability_ids: string[]; status: string
    }>
    current_profile: Record<string, unknown>
}

const detail = (token: string, id: string) => apiGet<Detail>(token, `${ROOT}/${id}`)
const candidates = (token: string) => apiGet<PaymentRequestCandidate[]>(token, `${ROOT}/candidates`)
const isZero = (amount: string) => /^0(?:\.0+)?$/.test(amount)

async function write<T>(page: Page, token: string, method: string, path: string, data: unknown): Promise<T> {
    const response = await page.request.fetch(`${API_BASE}${path}`, {
        method, headers: { Authorization: `Bearer ${token}` }, data, timeout: 60_000,
    })
    const body = await response.json() as { success: boolean; errorMessage?: string; data: T }
    expect(response.ok(), `${method} ${path}: ${body.errorMessage ?? response.status()}`).toBe(true)
    expect(body.success, body.errorMessage).toBe(true)
    return body.data
}

/** 查询范围固定为本次隔离运行；不得回退到开发配置，且脚本不包含写操作。 */
function storedTaskIdentity(taskId: string) {
    const configPath = process.env.ERP_E2E_CONFIG_PATH
    if (process.env.ERP_E2E_ISOLATED !== "1" || !configPath) throw new Error("任务责任验收必须使用本次隔离 E2E 配置")
    const database = JSON.parse(execFileSync("python3", ["-c",
        "import json,sys,tomllib; print(json.dumps(tomllib.load(open(sys.argv[1], 'rb'))['database']))", configPath,
    ], { encoding: "utf8", timeout: 10_000 })) as { uri: string; db_name: string }
    if (!/^erp_e2e_[0-9]{8}t[0-9]{6}_[0-9a-f]{12}_[0-9]+$/.test(database.db_name)) {
        throw new Error("只允许读取本次 runner 所有的临时库任务责任")
    }
    const script = `const target = db.getSiblingDB(${JSON.stringify(database.db_name)});
        const task = target.work_items.findOne({id: ${JSON.stringify(taskId)}}, {_id: 0, id: 1, responsibility_key: 1, owner_user_id: 1, status: 1});
        if (!task) throw new Error("Expected current payment task"); print(JSON.stringify(task));`
    return JSON.parse(execFileSync("mongosh", ["--norc", "--quiet", database.uri, "--eval", script], {
        encoding: "utf8", timeout: 30_000,
    }).trim()) as { id: string; responsibility_key: string; owner_user_id: string; status: string }
}

async function submitApproved(page: Page, finance: Page, row: PaymentRequestCandidate, amount: string) {
    const account = row.bank_accounts.find((item) => item.is_default) ?? row.bank_accounts[0]!
    expect(account, "采购必须选定当前有效供应商收款账户").toBeTruthy()
    const saved = await purchasePaymentRequestCommand<PurchasePaymentRequestResult>(page, "caigou", "POST", ROOT, { data: {
        supplier_id: row.supplier_id, payee_bank_account_id: account.id, desired_payment_date: analyticsDate(),
        reason: "E2E 原付款纠错与现任出纳恢复资格", attachment_ids: [],
        lines: [{ payable_account_id: row.payable_account_id, amount }],
    } })
    const submitted = await purchasePaymentRequestCommand<PurchasePaymentRequestResult>(page, "caigou", "POST", `${ROOT}/${saved.id}/submit`, {
        expected_version: saved.version, idempotency_key: randomUUID(),
    })
    await openWorkspaceTask(finance, /付款申请/, submitted.request_no, "approval")
    await approveCurrentDocument(finance)
    const approved = await detail(await apiToken("caigou"), saved.id)
    expect(approved.status).toBe("approved")
    expect(approved.work_item_id).toBeTruthy()
    return approved
}

async function pay(page: Page, request: Detail, amount: string) {
    await openWorkspaceTask(page, /供应商付款处理|待付款申请/, request.request_no, "finance")
    expect(new URL(page.url()).searchParams.get("currentWorkItemId")).toBe(request.work_item_id)
    await page.locator("#ppr-pay-amount").fill(amount)
    await page.locator("#ppr-pay-bank-receipt-input").setInputFiles(BANK_RECEIPT_FILE)
    await page.locator("#ppr-pay-submit").click()
    const committed = page.waitForResponse((response) => response.request().method() === "POST" &&
        new URL(response.url()).pathname === `${ROOT}/${request.id}/payments/commit`, { timeout: 60_000 })
    await page.locator("#ppr-pay-confirm-confirm").click()
    const response = await committed
    const body = await response.json() as { success: boolean; errorMessage?: string; data: { id: string; amount: string; status: string } }
    expect(response.ok(), body.errorMessage).toBe(true)
    expect(body.success).toBe(true)
    expect(body.data).toMatchObject({ amount, status: "posted" })
    return body.data
}

async function changeBank(page: Page, token: string, supplierId: string, enabled: boolean) {
    const source = await apiGet<Supplier>(token, `/admin/suppliers/${supplierId}`)
    await write(page, token, "PUT", `/admin/supplier-profiles/${supplierId}`, {
        idempotency_key: randomUUID(), expected_party_version: source.party_version, expected_supplier_version: source.version,
        legal_name: source.legal_name, short_name: source.short_name, unified_credit_code: source.unified_credit_code,
        ...source.current_profile,
        bank_account: enabled ? { bank_name: "E2E 冲正验收开户行", account_number: `6222${Date.now()}` } : null,
        clear_bank_account: !enabled, maintainer_user_id: source.maintainer_user_id,
        capability_codes: source.capability_codes, capability_owners: [],
        qualifications: source.qualifications.filter((item) => item.status === "active").map((item) => ({
            qualification_type: item.qualification_type, certificate_no: item.certificate_no,
            issuer: item.issuer, valid_from: item.valid_from, valid_to: item.valid_to, attachment_id: item.attachment_id,
            capability_codes: item.capability_ids.map((id) => source.capabilities.find((capability) => capability.id === id)!.capability_code),
        })), effective_from: analyticsDate(), change_reason: "E2E 付清后正常停用并更新收款账户",
    })
    return apiGet<Supplier>(token, `/admin/suppliers/${supplierId}`)
}

async function reversal(page: Page, token: string, procurement: Page, paymentId: string) {
    const created = await write<{ id: string; reversal_no: string }>(page, token, "POST", "/admin/payment-reversals/commit", {
        source_fact_id: paymentId, amount: null, reason: "E2E 按原付款分配完整冲正", idempotency_key: randomUUID(),
    })
    await openWorkspaceTask(procurement, /付款冲正审批/, created.reversal_no, "approval")
    await approveCurrentDocument(procurement)
    return created
}

async function finishReversal(finance: Page, token: string, created: { id: string; reversal_no: string }) {
    await openWorkspaceTask(finance, /付款冲正审批/, created.reversal_no, "approval")
    await approveCurrentDocument(finance)
    expect((await apiGet<{ status: string }>(token, `/admin/payment-reversals/${created.id}`)).status).toBe("posted")
}

test("付清后显式关闭授权再冲正；改派任务恢复现任出纳，撤权时原子拒绝恢复", async ({ browser }) => {
    test.setTimeout(14 * 60 * 1000)
    const suffix = Date.now().toString(36)
    const procurement = await openLoggedInWorkspace(browser, "caigou")
    const finance = await openLoggedInWorkspace(browser, "caiwu")
    const cashier = await openLoggedInWorkspace(browser, "fukuan")
    const adminToken = await apiToken("admin")
    const procurementToken = await apiToken("caigou")
    const cashierToken = await apiToken("fukuan")
    const fixture = await prepareAnalyticsOrders(await apiToken("xiaoshou"), adminToken, procurement.page, `REV${suffix}`)
    const purchases: Array<Awaited<ReturnType<typeof createAnalyticsPurchase>>> = []
    for (const order of fixture.orders) purchases.push(await createAnalyticsPurchase(order, procurement.page, procurementToken))

    // Paid 没有未付额度，关闭仅终止原批准授权；账户变化不能阻断随后账务纠错。
    const first = (await candidates(procurementToken)).find((row) => row.purchase_order_id === purchases[0]!.purchaseId)!
    const approved = await submitApproved(procurement.page, finance.page, first, first.available_total)
    const originalTask = await apiGet<Task>(adminToken, `/admin/work-items/${approved.work_item_id}`)
    const paid = await pay(cashier.page, approved, first.available_total)
    const completed = await detail(procurementToken, approved.id)
    expect(completed.status).toBe("paid")
    expect(isZero(completed.current_lines[0]!.current_reserved_total)).toBe(true)
    expect((await apiGet<Task>(adminToken, `/admin/work-items/${originalTask.id}`)).status).toBe("COMPLETED")
    try {
        const disabled = await changeBank(procurement.page, adminToken, first.supplier_id, false)
        const oldAccount = disabled.bank_accounts.find((account) => account.id === completed.payee_account.account_id)!
        expect(oldAccount.status).toBe("disabled")
        expect(oldAccount.version).toBeGreaterThan(completed.payee_account.version)
        await purchasePaymentRequestCommand(procurement.page, "caigou", "POST", `${ROOT}/${completed.id}/close`, {
            expected_version: completed.version, idempotency_key: randomUUID(), reason: "已付清；终止原批准授权后仅纠正历史付款",
        })
        const closed = await detail(procurementToken, completed.id)
        expect(closed.status).toBe("closed")
        expect(closed.lines[0]!.paid_amount).toBe(first.available_total)
        expect(isZero(closed.lines[0]!.closed_amount)).toBe(true)
        const created = await reversal(cashier.page, cashierToken, procurement.page, paid.id)
        await finishReversal(finance.page, adminToken, created)
        const reversed = await detail(procurementToken, completed.id)
        expect(reversed.status).toBe("closed")
        expect(isZero(reversed.lines[0]!.paid_amount)).toBe(true)
        expect(reversed.lines[0]!.closed_amount).toBe(reversed.lines[0]!.requested_amount)
        expect(isZero(reversed.current_lines[0]!.current_reserved_total)).toBe(true)
        expect(reversed.current_lines[0]!.current_available_total).toBe(first.available_total)
        expect(reversed.work_item_id).toBe(originalTask.id)
        expect((await apiGet<Task>(adminToken, `/admin/work-items/${originalTask.id}`)).status).toBe("COMPLETED")
        await cashier.page.goto("/workspace?family=finance")
        await expect(cashier.page.getByRole("button", { name: /供应商付款处理|待付款申请/ }).filter({ hasText: completed.request_no })).toHaveCount(0)
    } finally { await changeBank(procurement.page, adminToken, first.supplier_id, true) }
    const availableAgain = (await candidates(procurementToken)).find((row) => row.payable_account_id === first.payable_account_id)!
    expect(availableAgain.available_total).toBe(first.available_total)
    expect(isZero(availableAgain.reserved_total)).toBe(true)
    const renewed = await submitApproved(procurement.page, finance.page, availableAgain, "1.00")
    expect(renewed.id).not.toBe(completed.id)
    expect(renewed.work_item_id).not.toBe(originalTask.id)
    expect(renewed.payee_account).not.toEqual(completed.payee_account)
    await purchasePaymentRequestCommand(procurement.page, "caigou", "POST", `${ROOT}/${renewed.id}/close`, {
        expected_version: renewed.version, idempotency_key: randomUUID(), reason: "重新申请与审核已验收，结束本次未付测试授权",
    })

    // 合格具体出纳 B 通过正式改派接管；配置默认出纳 A 不发生变化。
    const accounts = await apiGet<Account[]>(adminToken, "/admin/admins")
    const defaultCashier = accounts.find((account) => account.account === "fukuan")!
    const target = accounts.find((account) => account.account === "xitong")!
    const originalRoles = [...target.role_ids]
    const roleCatalog = await apiGet<Array<{ id: string; permissions: string[] }>>(adminToken, "/admin/roles")
    const originalPermissions = roleCatalog.filter((role) => originalRoles.includes(role.id)).flatMap((role) => role.permissions)
    expect(originalPermissions.some((permission) => ["*", "purchase_payment_request:*", "purchase_payment_request:pay"].includes(permission)),
        "恢复原系统岗位角色必须确实撤销付款执行权").toBe(false)
    const rulesBefore = await apiGet<unknown[]>(adminToken, "/admin/finance-responsibility-rules")
    await write(procurement.page, adminToken, "PUT", `/admin/admins/${target.id}/role`, { role_ids: defaultCashier.role_ids })
    try {
        const replacement = await openLoggedInWorkspace(browser, "xitong")
        const replacementToken = await apiToken("xitong")
        const second = (await candidates(procurementToken)).find((row) => row.purchase_order_id === purchases[1]!.purchaseId)!
        const toReassign = await submitApproved(procurement.page, finance.page, second, "2.00")
        const before = await apiGet<Task>(adminToken, `/admin/work-items/${toReassign.work_item_id}`)
        expect(before.owner_user_id).toBe(defaultCashier.id)
        const storedBefore = storedTaskIdentity(before.id)
        expect(storedBefore.responsibility_key).toContain(`:request:${toReassign.id}`)
        const eligible = await apiGet<Array<{ user_id: string }>>(adminToken, `/admin/work-items/${before.id}/reassign-candidates`)
        expect(eligible.some((account) => account.user_id === target.id), "新出纳必须经真实完整执行资格筛选").toBe(true)
        const assigned = await write<Task>(procurement.page, adminToken, "POST", `/admin/work-items/${before.id}/reassign`, {
            expected_task_version: before.task_version, target_user_id: target.id,
            reason: "E2E 当前付款任务交接给合格新出纳，不改默认责任", idempotency_key: randomUUID(),
        })
        expect(assigned.owner_user_id).toBe(target.id)
        expect(assigned.task_version).not.toBe(before.task_version)
        expect(storedTaskIdentity(before.id)).toMatchObject({
            responsibility_key: storedBefore.responsibility_key, owner_user_id: target.id, status: "OPEN",
        })
        const actualPayment = await pay(replacement.page, await detail(procurementToken, toReassign.id), "1.00")
        const partial = await detail(procurementToken, toReassign.id)
        expect(partial.status).toBe("partially_paid")
        expect(partial.lines[0]!.paid_amount).toBe("1.00")
        expect(partial.current_lines[0]!.current_reserved_total).toBe("1.00")
        const partialTask = await apiGet<Task>(adminToken, `/admin/work-items/${before.id}`)
        expect(partialTask.owner_user_id).toBe(target.id)
        const created = await reversal(replacement.page, replacementToken, procurement.page, actualPayment.id)
        await finishReversal(finance.page, adminToken, created)
        const restored = await detail(procurementToken, partial.id)
        expect(restored.status).toBe("approved")
        expect(isZero(restored.lines[0]!.paid_amount)).toBe(true)
        expect(restored.current_lines[0]!.current_reserved_total).toBe("2.00")
        expect(restored.work_item_id).toBe(before.id)
        const restoredTask = await apiGet<Task>(replacementToken, `/admin/work-items/${before.id}`)
        expect(restoredTask).toMatchObject({ id: before.id, owner_user_id: target.id, status: "OPEN", subject_version: before.subject_version })
        expect(restoredTask.task_version).not.toBe(partialTask.task_version)
        expect(storedTaskIdentity(before.id)).toMatchObject({
            responsibility_key: storedBefore.responsibility_key, owner_user_id: target.id, status: "OPEN",
        })
        expect(await apiGet<unknown[]>(adminToken, "/admin/finance-responsibility-rules")).toEqual(rulesBefore)

        // 同一恢复入口须重验现任出纳，撤权不能靠原批准与历史责任恢复占用。
        const nextPayment = await pay(replacement.page, restored, "1.00")
        const nextReversal = await reversal(replacement.page, replacementToken, procurement.page, nextPayment.id)
        await write(procurement.page, adminToken, "PUT", `/admin/admins/${target.id}/role`, { role_ids: originalRoles })
        expect((await apiGet<Account[]>(adminToken, "/admin/admins")).find((account) => account.id === target.id)!.role_ids).toEqual(originalRoles)
        const ledgerBeforeDenied = await detail(procurementToken, restored.id)
        const identityBeforeDenied = storedTaskIdentity(before.id)
        const reversalBeforeDenied = await apiGet<{ status: string }>(adminToken, `/admin/payment-reversals/${nextReversal.id}`)
        await openWorkspaceTask(finance.page, /付款冲正审批/, nextReversal.reversal_no, "approval")
        await finance.page.getByRole("button", { name: /^(通过|同意审批)$/ }).first().click()
        const confirmation = finance.page.getByRole("dialog", { name: "确认通过" })
        const rejected = finance.page.waitForResponse((response) => response.request().method() === "POST" &&
            new URL(response.url()).pathname === "/admin/approval-decisions", { timeout: 60_000 })
        await confirmation.locator('[id$="-decision-dialog-submit"]').click()
        const reply = await rejected
        const rejection = await reply.json() as { success: boolean; errorMessage?: string }
        expect([403, 409, 422], rejection.errorMessage).toContain(reply.status())
        expect(rejection.success).toBe(false)
        expect(rejection.errorMessage).toMatch(/权限|资格|责任|执行/)
        const ledgerAfterDenied = await detail(procurementToken, restored.id)
        expect(ledgerAfterDenied.version).toBe(ledgerBeforeDenied.version)
        expect(ledgerAfterDenied.lines).toEqual(ledgerBeforeDenied.lines)
        expect(ledgerAfterDenied.payments).toEqual(ledgerBeforeDenied.payments)
        expect(ledgerAfterDenied.current_lines).toEqual(ledgerBeforeDenied.current_lines)
        expect(storedTaskIdentity(before.id)).toEqual(identityBeforeDenied)
        expect((await apiGet<{ status: string }>(adminToken, `/admin/payment-reversals/${nextReversal.id}`)).status).toBe(reversalBeforeDenied.status)
        await write(procurement.page, adminToken, "PUT", `/admin/admins/${target.id}/role`, { role_ids: defaultCashier.role_ids })
        await finance.page.goto("/workspace?family=approval")
        await finishReversal(finance.page, adminToken, nextReversal)
        expect((await detail(procurementToken, restored.id)).current_lines[0]!.current_reserved_total).toBe("2.00")
        expect(storedTaskIdentity(before.id)).toMatchObject({
            responsibility_key: storedBefore.responsibility_key, owner_user_id: target.id, status: "OPEN",
        })
    } finally {
        await write(procurement.page, adminToken, "PUT", `/admin/admins/${target.id}/role`, { role_ids: originalRoles })
    }
})
