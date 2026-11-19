/**
 * flow-26: 旧数据导入批次、业务确认、幂等恢复及未接通执行器的拒绝边界。
 * 来源系统、身份映射、客户、批次及任务均经真实 API/页面创建。
 * 未启动批次不得登记导入结果；当前正式执行器尚未开放。
 */
import { workPage } from "../helpers/workspace-page"
import { test, expect, type Response } from "../helpers/test"
import { API_BASE, apiGet, apiToken } from "../helpers/api"
import { createCustomerViaUi } from "../helpers/customers"
import { loginViaUi } from "../helpers/login"

type Envelope<T> = { success?: boolean; errorMessage?: string; code?: string; data: T }
type ApiPage<T> = { items: T[]; total: number }
type Batch = {
    id: string
    batch_no: string
    status: string
    version: number
    total_rows: number
    success_rows: number
    failed_rows: number
    background_job_id: string
}
type ImportRow = {
    id: string
    source_row_key: string
    import_status: string
    parse_status: string
    mapping_status: string
    target_document_id: string | null
    error_code: string | null
    version: number
}
type Confirmation = {
    id: string
    work_item_id: string
    status: string
    work_item: { task_version: string; subject_version: string; status: string }
}
type Confirmed = { result_status: string; next_step: string; audit_receipt: string; work_item: { status: string }; confirmation: { status: string } }

async function post<T>(token: string, endpoint: string, body: unknown): Promise<T> {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method: "POST",
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify(body), signal: AbortSignal.timeout(20_000),
    })
    const result = (await response.json()) as Envelope<T>
    expect(response.ok && result.success !== false, `${endpoint}: ${result.errorMessage}`).toBe(true)
    return result.data
}

async function rejected(token: string, endpoint: string, body: unknown, status: 400 | 422, reason?: RegExp) {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method: "POST", headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify(body), signal: AbortSignal.timeout(20_000),
    })
    const result = (await response.json()) as Envelope<unknown>
    expect(response.status, result.errorMessage).toBe(status)
    expect(result.success).toBe(false)
    expect(result.code).toBe(status === 400 ? "INVALID_REQUEST" : "BUSINESS_RULE_BLOCKED")
    if (reason) expect(result.errorMessage).toMatch(reason)
}

async function uiResult<T>(response: Response): Promise<T> {
    const result = (await response.json()) as Envelope<T>
    expect(response.ok() && result.success !== false, result.errorMessage).toBe(true)
    return result.data
}

test("flow-26 业务确认不启动导入，执行器未接通时正式启动和结果登记均被拒绝", async ({ page }) => {
    await loginViaUi(page, "admin")
    const token = await apiToken("admin")
    const marker = `E2E-IMP-${Date.now()}`
    const createdCustomer = page.waitForResponse((response) => response.request().method() === "POST"
        && new URL(response.url()).pathname === "/admin/customer-profiles")
    await createCustomerViaUi(page, { legalName: `${marker} 导入客户`, creditCode: "", paymentTermLabel: "货到 15 天" })
    const customerId = (await uiResult<{ customer_id: string }>(await createdCustomer)).customer_id
    const customer = await apiGet<{ id: string; party_id: string }>(token, `/admin/customer-profiles/${customerId}`)
    const source = await post<{ id: string }>(token, "/admin/source-systems", { code: marker, name: `${marker} 旧系统`, system_type: "ERP" })
    const identities: Array<{ id: string }> = []
    for (const key of ["1", "2"]) {
        identities.push(await post<{ id: string }>(token, "/admin/external-identity-maps", {
            source_system_id: source.id, object_type: "customer", external_id: `${marker}-${key}`,
            internal_object_type: "party", internal_object_id: customer.party_id, relation_role: "PRIMARY",
            valid_from: Math.floor(Date.now() / 1000),
        }))
    }
    const create = {
        batch_no: marker, source_system_id: source.id, source_object_set: "CUSTOMER", baseline_date: "2026-10-01",
        import_rule_version: "e2e-v1", rows: ["1", "2"].map((key) => ({ source_object_type: "CUSTOMER",
            source_row_key: key, normalized_payload_reference: `whitelist:${marker}:${key}` })),
    }
    await rejected(token, "/admin/legacy-import-batches", { ...create, batch_no: `${marker}-DUP`, rows: [create.rows[0], create.rows[0]] }, 422, /重复/)
    const batch = await post<Batch>(token, "/admin/legacy-import-batches", create)
    expect(batch.status).toBe("pending_validation")
    expect(batch.total_rows).toBe(2)
    expect(batch.background_job_id).toBeTruthy()
    expect((await post<Batch>(token, "/admin/legacy-import-batches", create)).id).toBe(batch.id)
    const rows = await apiGet<ApiPage<ImportRow>>(token, `/admin/legacy-import-batches/${batch.id}/rows`, { page: 1, page_size: 100 })
    expect(rows.total).toBe(2)
    const first = rows.items.find((row) => row.source_row_key === "1")!
    expect(first.import_status).toBe("pending_import")
    await rejected(token, `/admin/legacy-import-batches/${batch.id}/commands`, {
        batch_id: batch.id, expected_batch_version: String(batch.version), expected_trial_version: "1",
        action: "START_APPLY", request_id: `${marker}-early-start`,
    }, 422)
    await rejected(token, `/admin/legacy-import-batches/${batch.id}/apply`, { results: [{ row_id: first.id,
        outcome: "imported", external_identity_map_id: identities[0]!.id, target_document_id: customer.party_id }] }, 422, /未到导入阶段/)

    const confirmationBody = { batch_id: batch.id, confirmation_scope: "SALES", batch_version: 1, trial_version: 1, import_rule_version: "e2e-v1" }
    const confirmation = await post<Confirmation>(token, "/admin/legacy-import-confirmations", confirmationBody)
    expect(confirmation.work_item_id).toBeTruthy()
    const replayedConfirmation = await post<Confirmation>(token, "/admin/legacy-import-confirmations", confirmationBody)
    expect(replayedConfirmation.id).toBe(confirmation.id)
    expect(replayedConfirmation.work_item_id).toBe(confirmation.work_item_id)
    await page.goto(`/governance/imports?environment=PRODUCTION&batchId=${batch.id}&section=confirm`)
    await expect(workPage(page).getByText(marker, { exact: true }).first()).toBeVisible()
    await workPage(page).getByRole("button", { name: "确认本范围", exact: true }).click()
    const confirmButton = workPage(page).locator(`#operations-import-batch-detail-confirm-${confirmation.id}-dialog-confirm`)
    await expect(confirmButton).toBeVisible()
    const [confirmedResponse] = await Promise.all([
        page.waitForResponse((response) => response.request().method() === "POST"
            && new URL(response.url()).pathname === "/admin/legacy-import-confirmations/complete"),
        confirmButton.click(),
    ])
    const confirmed = await uiResult<Confirmed>(confirmedResponse)
    expect(confirmed.result_status).toBe("CONFIRMED")
    expect(confirmed.next_step).toBe("START_APPLY")
    expect(confirmed.confirmation.status).toBe("CONFIRMED")
    const confirmReplay = await post<Confirmed>(token, "/admin/legacy-import-confirmations/complete", confirmedResponse.request().postDataJSON())
    expect(confirmReplay.audit_receipt).toBe(confirmed.audit_receipt)
    const ready = await apiGet<Batch>(token, `/admin/legacy-import-batches/${batch.id}`)
    expect(ready.status).toBe("ready_to_apply")
    expect(ready.success_rows).toBe(0)
    expect(ready.failed_rows).toBe(0)
    expect((await apiGet<ApiPage<ImportRow>>(token, `/admin/legacy-import-batches/${batch.id}/rows`, { page: 1, page_size: 100 })).items.every((row) => row.import_status === "pending_import")).toBe(true)

    const rowsBefore = await apiGet<ApiPage<ImportRow>>(token, `/admin/legacy-import-batches/${batch.id}/rows`, { page: 1, page_size: 100 })
    const jobPath = `/admin/background-jobs/${batch.background_job_id}`
    const jobBefore = await apiGet<{ status: string; version: number; processed_count: number }>(token, jobPath)
    expect(jobBefore.status).toBe("pending")
    expect(jobBefore.processed_count).toBe(0)
    await page.goto(`/governance/imports?environment=PRODUCTION&batchId=${batch.id}&section=progress`)
    await expect(workPage(page).getByText(/期初数据正式写入功能暂未开放/)).toBeVisible()
    await expect(workPage(page).locator("#operations-import-batch-detail-execution-start-apply")).toBeDisabled()

    const commands = `/admin/legacy-import-batches/${batch.id}/commands`
    for (const action of ["START_APPLY", "RETRY_FAILED"]) {
        await rejected(token, commands, {
            batch_id: batch.id, expected_batch_version: String(ready.version), expected_trial_version: "1",
            action, request_id: `${marker}-${action}`,
        }, 422, /当前业务条件不允许继续操作/)
    }
    await rejected(token, `/admin/legacy-import-batches/${batch.id}/apply`, { results: [{ row_id: first.id,
        outcome: "imported", external_identity_map_id: identities[0]!.id, target_document_id: customer.party_id }] }, 422, /未到导入阶段/)
    expect(await apiGet<Batch>(token, `/admin/legacy-import-batches/${batch.id}`)).toEqual(ready)
    expect(await apiGet<{ status: string; version: number; processed_count: number }>(token, jobPath)).toEqual(jobBefore)
    expect(await apiGet<ApiPage<ImportRow>>(token, `/admin/legacy-import-batches/${batch.id}/rows`, { page: 1, page_size: 100 })).toEqual(rowsBefore)
})
