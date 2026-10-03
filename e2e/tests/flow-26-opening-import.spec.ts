/**
 * flow-26: 旧数据导入批次、业务确认、独立应用、失败结果修正与幂等恢复。
 * 来源系统、身份映射、客户、批次及任务均经真实 API/页面创建。
 * /apply 验收后台逐行结果登记，不声称覆盖旧系统文件解析或自动搬迁。
 */
import { test, expect, type Page, type Response } from "../helpers/test"
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
type Execution = { result_status: string; affected_items: number; batch_status: string; background_job_status: string; audit_receipt: string }
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

async function rejected(token: string, endpoint: string, body: unknown, reason: RegExp | string) {
    const response = await fetch(`${API_BASE}${endpoint}`, {
        method: "POST", headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify(body), signal: AbortSignal.timeout(20_000),
    })
    const result = (await response.json()) as Envelope<unknown>
    expect(response.ok && result.success !== false).toBe(false)
    expect([400, 409, 422], result.errorMessage).toContain(response.status)
    if (typeof reason === "string") {
        expect(response.status).toBe(400)
        expect(result.code).toBe(reason)
    } else {
        expect(result.errorMessage).toMatch(reason)
    }
}

async function uiResult<T>(response: Response): Promise<T> {
    const result = (await response.json()) as Envelope<T>
    expect(response.ok() && result.success !== false, result.errorMessage).toBe(true)
    return result.data
}

async function executeViaUi(page: Page, batchId: string, action: "start-apply" | "retry-failed") {
    await page.goto(`/governance/imports?environment=PRODUCTION&batchId=${batchId}&section=progress`)
    await page.locator(`#operations-import-batch-detail-execution-${action}`).click()
    const confirm = page.locator(`#operations-import-batch-detail-execution-${action}-confirm`)
    await expect(confirm).toBeVisible()
    const [returned] = await Promise.all([
        page.waitForResponse((candidate) => candidate.request().method() === "POST"
            && new URL(candidate.url()).pathname === `/admin/legacy-import-batches/${batchId}/commands`),
        confirm.click(),
    ])
    return { body: returned.request().postDataJSON() as unknown, result: await uiResult<Execution>(returned) }
}

test("flow-26 业务确认不启动导入，应用部分失败后只重试失败行并保留成功事实", async ({ page }) => {
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
    await rejected(token, "/admin/legacy-import-batches", { ...create, batch_no: `${marker}-DUP`, rows: [create.rows[0], create.rows[0]] }, /重复/)
    const batch = await post<Batch>(token, "/admin/legacy-import-batches", create)
    expect(batch.status).toBe("pending_validation")
    expect(batch.total_rows).toBe(2)
    expect(batch.background_job_id).toBeTruthy()
    expect((await post<Batch>(token, "/admin/legacy-import-batches", create)).id).toBe(batch.id)
    const rows = await apiGet<ApiPage<ImportRow>>(token, `/admin/legacy-import-batches/${batch.id}/rows`, { page: 1, page_size: 100 })
    expect(rows.total).toBe(2)
    const first = rows.items.find((row) => row.source_row_key === "1")!
    const second = rows.items.find((row) => row.source_row_key === "2")!
    expect(first.import_status).toBe("pending_import")
    await rejected(token, `/admin/legacy-import-batches/${batch.id}/commands`, {
        batch_id: batch.id, expected_batch_version: String(batch.version), expected_trial_version: "1",
        action: "START_APPLY", request_id: `${marker}-early-start`,
    }, /确认|待应用/)
    await rejected(token, `/admin/legacy-import-batches/${batch.id}/apply`, { results: [{ row_id: first.id,
        outcome: "imported", external_identity_map_id: identities[0]!.id, target_document_id: customer.party_id }] }, /尚未进入导入/)

    const confirmationBody = { batch_id: batch.id, confirmation_scope: "SALES", batch_version: 1, trial_version: 1, import_rule_version: "e2e-v1" }
    const confirmation = await post<Confirmation>(token, "/admin/legacy-import-confirmations", confirmationBody)
    expect(confirmation.work_item_id).toBeTruthy()
    const replayedConfirmation = await post<Confirmation>(token, "/admin/legacy-import-confirmations", confirmationBody)
    expect(replayedConfirmation.id).toBe(confirmation.id)
    expect(replayedConfirmation.work_item_id).toBe(confirmation.work_item_id)
    await page.goto(`/governance/imports?environment=PRODUCTION&batchId=${batch.id}&section=confirm`)
    await expect(page.getByText(marker, { exact: true }).first()).toBeVisible()
    await page.getByRole("button", { name: "确认本范围", exact: true }).click()
    const confirmButton = page.locator(`#operations-import-batch-detail-confirm-${confirmation.id}-dialog-confirm`)
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

    const started = await executeViaUi(page, batch.id, "start-apply")
    expect(started.result.result_status).toBe("STARTED")
    expect(started.result.batch_status).toBe("importing")
    expect(started.result.affected_items).toBe(2)
    expect((await post<Execution>(token, `/admin/legacy-import-batches/${batch.id}/commands`, started.body)).audit_receipt).toBe(started.result.audit_receipt)
    await rejected(token, `/admin/legacy-import-batches/${batch.id}/apply`, { results: [{ row_id: first.id, outcome: "imported" }] }, "INVALID_REQUEST")
    await rejected(token, `/admin/legacy-import-batches/${batch.id}/apply`, { results: [first, first].map((row) => ({ row_id: row.id,
        outcome: "imported", external_identity_map_id: identities[0]!.id, target_document_id: customer.party_id })) }, "INVALID_REQUEST")
    await rejected(token, `/admin/legacy-import-batches/${batch.id}/apply`, { results: [{ row_id: `${marker}-missing-row`,
        outcome: "imported", external_identity_map_id: identities[0]!.id, target_document_id: customer.party_id }] }, "INVALID_REQUEST")
    const beforeResults = await apiGet<ApiPage<ImportRow>>(token, `/admin/legacy-import-batches/${batch.id}/rows`, { page: 1, page_size: 100 })
    expect(beforeResults.items.every((row) => row.import_status === "pending_import")).toBe(true)
    const results = { results: [{ row_id: first.id, outcome: "imported", external_identity_map_id: identities[0]!.id,
        target_document_id: customer.party_id }, { row_id: second.id, outcome: "imported", external_identity_map_id: identities[1]!.id,
        target_document_id: `${marker}-missing-party` }] }
    const partial = await post<Batch>(token, `/admin/legacy-import-batches/${batch.id}/apply`, results)
    expect(partial.status).toBe("partial_failed")
    expect(partial.success_rows).toBe(1)
    expect(partial.failed_rows).toBe(1)
    const afterResults = await apiGet<ApiPage<ImportRow>>(token, `/admin/legacy-import-batches/${batch.id}/rows`, { page: 1, page_size: 100 })
    const saved = afterResults.items.find((row) => row.id === first.id)!
    expect(saved.target_document_id).toBe(customer.party_id)
    expect(saved.import_status).toBe("imported")
    expect(afterResults.items.find((row) => row.id === second.id)?.error_code).toBe("CUSTOMER_NOT_FOUND")
    const partialReplay = await post<Batch>(token, `/admin/legacy-import-batches/${batch.id}/apply`, results)
    expect(partialReplay.version).toBe(partial.version)
    await page.goto(`/governance/imports?environment=PRODUCTION&batchId=${batch.id}&section=trial`)
    const issueTable = page.getByRole("table", { name: "导入问题明细（不含成功行）" })
    const issueRow = issueTable.getByRole("row").filter({ hasText: "CUSTOMER_NOT_FOUND" })
    await expect(issueRow).toBeVisible()
    await expect(issueRow.getByRole("cell", { name: "客户不存在", exact: true })).toBeVisible()
    await expect(issueTable.getByRole("row")).toHaveCount(2)

    const retried = await executeViaUi(page, batch.id, "retry-failed")
    expect(retried.result.affected_items).toBe(1)
    expect(retried.result.batch_status).toBe("ready_to_apply")
    const retryRows = await apiGet<ApiPage<ImportRow>>(token, `/admin/legacy-import-batches/${batch.id}/rows`, { page: 1, page_size: 100 })
    expect(retryRows.items.find((row) => row.id === first.id)).toEqual(saved)
    expect(retryRows.items.find((row) => row.id === second.id)?.import_status).toBe("pending_import")
    const restarted = await executeViaUi(page, batch.id, "start-apply")
    expect(restarted.result.affected_items).toBe(1)
    const fixed = { results: [{ row_id: second.id, outcome: "imported", target_document_id: customer.party_id }] }
    const completed = await post<Batch>(token, `/admin/legacy-import-batches/${batch.id}/apply`, fixed)
    expect(completed.status).toBe("completed")
    expect(completed.success_rows).toBe(2)
    expect(completed.failed_rows).toBe(0)
    expect((await post<Batch>(token, `/admin/legacy-import-batches/${batch.id}/apply`, fixed)).version).toBe(completed.version)
    const completedRows = await apiGet<ApiPage<ImportRow>>(token, `/admin/legacy-import-batches/${batch.id}/rows`, { page: 1, page_size: 100 })
    expect(completedRows.items.find((row) => row.id === first.id)).toEqual(saved)
    expect(completedRows.items.every((row) => row.import_status === "imported" && row.target_document_id === customer.party_id)).toBe(true)
    await page.goto(`/governance/imports?environment=PRODUCTION&batchId=${batch.id}&section=result`)
    await expect(page.getByText(marker, { exact: true }).first()).toBeVisible()
    await expect(page.locator("#operations-import-batch-detail-execution-retry-failed")).toHaveCount(0)
})
