import { expect, type Page } from "@playwright/test"

import { API_BASE, apiGet, apiToken } from "./api"

type FinancialKind =
    | "customer_receipt"
    | "receipt_reversal"
    | "payment_reversal"

const DEFINITIONS = {
    customer_receipt: {
        resource: "customer-receipts",
        label: "客户回款单",
        side: "customer",
        preview: "receipt",
    },
    receipt_reversal: {
        resource: "receipt-reversals",
        label: "回款冲正单",
        side: "customer",
        preview: "reversal",
    },
    payment_reversal: {
        resource: "payment-reversals",
        label: "付款冲正单",
        side: "supplier",
        preview: "reversal",
    },
} as const

export type FinancialOriginal = {
    id: string
    version: number
    receipt_no?: string
    reversal_no?: string
    status: string
    amount: string
    bank_reference?: string | null
    received_at?: number
    reason_text?: string
    counterparty_party_id?: string
    customer_id?: string | null
    original_customer_receipt_id?: string
    original_supplier_payment_id?: string
    pending_allocations?: {
        receivable_entry_id: string
        allocated_amount: string
    }[]
    approval: {
        instance: {
            id: string
            status: string
            current_round_no: number
            subject_version?: string | number
        }
    }
}

/** 回款专用读保留原登记人、完整资金源与冻结核销，冲正按正式详情读取。 */
export async function readFinancialOriginal(
    kind: FinancialKind,
    id: string,
): Promise<FinancialOriginal> {
    const token = await apiToken("fukuan")
    const suffix = kind === "customer_receipt" ? "/draft" : ""
    return apiGet(token, `/admin/${DEFINITIONS[kind].resource}/${id}${suffix}`)
}

/** 只读本经办人实际发起的精确实例，冻结版本来自统一审批列表投影。 */
export async function readFinancialApprovalVersion(
    documentType: string,
    documentId: string,
    instanceId: string,
): Promise<number> {
    const token = await apiToken("fukuan")
    let cursor: string | undefined
    const seen = new Set<string>()
    for (;;) {
        const page = await apiGet<{
            items: {
                instance_id: string
                document_id: string
                document_type: string
                subject_version: number | null
            }[]
            next_cursor?: string | null
        }>(token, "/admin/approval-instances", {
            view: "started",
            document_type: documentType,
            cursor,
            limit: 50,
        })
        const row = page.items.find((item) => item.instance_id === instanceId)
        if (row) {
            expect(row.document_id).toBe(documentId)
            expect(row.document_type).toBe(documentType)
            expect(row.subject_version).not.toBeNull()
            expect(Number.isInteger(row.subject_version)).toBe(true)
            expect(row.subject_version!).toBeGreaterThan(0)
            return row.subject_version!
        }
        if (!page.next_cursor || seen.has(page.next_cursor)) {
            throw new Error(
                `未读到原经办人发起的审批实例 ${documentType}/${documentId}/${instanceId}`,
            )
        }
        seen.add(page.next_cursor)
        cursor = page.next_cursor
    }
}

/** 使用实际审批动作驳回，并等待该轮决定事务成功后继续。 */
export async function rejectFinancialOriginal(
    page: Page,
    reason: string,
): Promise<void> {
    await page.getByRole("button", { name: "驳回", exact: true }).click()
    const dialog = page.getByRole("dialog", { name: "确认驳回", exact: true })
    await expect(dialog).toBeVisible({ timeout: 40_000 })
    await dialog.getByLabel(/^驳回原因\s*\*?$/).fill(reason)
    const applied = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            new URL(response.url()).pathname === "/admin/approval-decisions",
        { timeout: 90_000 },
    )
    await dialog.getByRole("button", { name: "确认驳回", exact: true }).click()
    const response = await applied
    expect(response.ok(), await response.text()).toBeTruthy()
    await expect(dialog).toBeHidden({ timeout: 40_000 })
}

/** 原金额、来源与核销保留；修改字段后新审批消费保存后的版本。 */
export async function reviseFinancialOriginal(
    page: Page,
    input: {
        kind: FinancialKind
        id: string
        documentNo: string
        changedText: string
    },
): Promise<void> {
    const definition = DEFINITIONS[input.kind]
    const before = await readFinancialOriginal(input.kind, input.id)
    expect(before.id).toBe(input.id)
    expect(before.receipt_no ?? before.reversal_no).toBe(input.documentNo)
    expect(before.status).toBe("IN_APPROVAL")
    expect(before.approval.instance.status).toBe("RUNNING")
    expect(before.approval.instance.current_round_no).toBe(2)
    const beforeSubjectVersion = await readFinancialApprovalVersion(
        input.kind,
        input.id,
        before.approval.instance.id,
    )
    const previewKey = definition.side === "customer" ? "previewId" : "detailId"
    const params = new URLSearchParams({
        view: definition.side === "customer" ? "receipt" : "payment",
        previewKind: definition.preview,
        [previewKey]: input.id,
    })
    await page.goto(`/finance/${definition.side}-accounts?${params}`)
    const revise = page.getByRole("button", { name: "修改原单", exact: true })
    await expect(revise).toBeEnabled({ timeout: 40_000 })
    await revise.click()
    const cancel = page.getByRole("dialog", { name: "修改原单", exact: true })
    await expect(cancel).toBeVisible({ timeout: 40_000 })
    await cancel.getByLabel(/^原因\s*\*?$/).fill("按驳回意见补齐原单依据")
    await cancel
        .getByRole("button", { name: "撤回并修改原单", exact: true })
        .click()
    const editor = page.getByRole("dialog", {
        name: `修改${definition.label}`,
        exact: true,
    })
    await expect(editor).toBeVisible({ timeout: 40_000 })
    const draft = await readFinancialOriginal(input.kind, input.id)
    expect(draft.status).toBe("draft")
    expect(draft.approval.instance.id).toBe(before.approval.instance.id)
    expect(draft.approval.instance.status).toBe("CANCELLED")
    expect(draft.receipt_no ?? draft.reversal_no).toBe(input.documentNo)
    expect(draft.amount).toBe(before.amount)
    await expect(editor.getByLabel(/^金额\s*\*?$/)).toHaveValue(before.amount, {
        timeout: 40_000,
    })
    const changedField =
        input.kind === "customer_receipt" ? "银行流水号" : "原因说明"
    await editor
        .getByLabel(new RegExp(`^${changedField}\\s*\\*?$`))
        .fill(input.changedText)
    const resourcePath = `/admin/${definition.resource}/${input.id}`
    const saved = page.waitForResponse(
        (response) =>
            response.request().method() === "PUT" &&
            new URL(response.url()).pathname === resourcePath,
        { timeout: 90_000 },
    )
    const submitted = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            new URL(response.url()).pathname === `${resourcePath}/submit`,
        { timeout: 90_000 },
    )
    await editor
        .getByRole("button", { name: "保存并提交审批", exact: true })
        .click()
    const savedResponse = await saved
    expect(savedResponse.ok(), await savedResponse.text()).toBeTruthy()
    const savedDraft = (await savedResponse.json()).data as FinancialOriginal
    expect(savedDraft.id).toBe(input.id)
    expect(savedDraft.version).toBeGreaterThan(draft.version)
    const response = await submitted
    expect(response.ok(), await response.text()).toBeTruthy()
    const command = response.request().postDataJSON() as {
        expected_version: number
        idempotency_key: string
        allocations?: FinancialOriginal["pending_allocations"]
    }
    expect(command.expected_version).toBe(savedDraft.version)
    expect(command.idempotency_key).toBeTruthy()
    if (input.kind === "customer_receipt") {
        expect(command.allocations).toEqual(before.pending_allocations)
        expect(draft.pending_allocations).toEqual(before.pending_allocations)
    }
    await expect(editor).toBeHidden({ timeout: 40_000 })
    const current = await readFinancialOriginal(input.kind, input.id)
    expect(current.id).toBe(input.id)
    expect(current.receipt_no ?? current.reversal_no).toBe(input.documentNo)
    expect(current.status).toBe("IN_APPROVAL")
    expect(current.amount).toBe(before.amount)
    expect(
        input.kind === "customer_receipt"
            ? current.bank_reference
            : current.reason_text,
    ).toBe(input.changedText)
    for (const field of [
        "counterparty_party_id",
        "customer_id",
        "received_at",
        "original_customer_receipt_id",
        "original_supplier_payment_id",
    ] as const) {
        expect(draft[field]).toBe(before[field])
        expect(current[field]).toBe(before[field])
    }
    expect(current.approval.instance.id).not.toBe(before.approval.instance.id)
    expect(current.approval.instance.status).toBe("RUNNING")
    expect(current.approval.instance.current_round_no).toBe(1)
    const currentSubjectVersion = await readFinancialApprovalVersion(
        input.kind,
        input.id,
        current.approval.instance.id,
    )
    expect(currentSubjectVersion).toBeGreaterThan(beforeSubjectVersion)
    // 真实提交成功后旧版本重放必须命中相同操作，而不是再次保存/另起审批。
    const token = await apiToken("fukuan")
    const replay = await page.request.post(
        `${API_BASE}${resourcePath}/submit`,
        {
            headers: { Authorization: `Bearer ${token}` },
            data: command,
        },
    )
    expect(replay.ok(), await replay.text()).toBeTruthy()
    const replayed = (await replay.json()).data as FinancialOriginal
    expect(replayed.id).toBe(input.id)
    expect(replayed.version).toBe(current.version)
    expect(replayed.approval.instance.id).toBe(current.approval.instance.id)
}
