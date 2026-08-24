/** Post allocation (draft session → formal receipt/invoice allocations). */

import { apiGet, apiPost } from "@/lib/api"

import type {
    AllocationSessionView,
    PostAllocationInput,
    PostAllocationResult,
} from "@/features/customer-receivables/types"
import {
    buildCustomerReceiptSubmitRequest,
    mapCustomerReceiptApproval,
} from "@/features/customer-receivables/lib/customer-receipt-approval"
import { stripInvoiceApprovalField } from "@/features/customer-receivables/lib/invoice-no-approval"
import type { BackendCustomerReceipt, BackendInvoice } from "./dto"
import { sessions } from "./session"

export const postIdempotency = new Map<string, PostAllocationResult>()

function failedResult(
    code: string,
    message: string,
): Extract<PostAllocationResult, { status: "failed" }> {
    return { status: "failed", code, message }
}

function isPostedReceiptStatus(status?: string): boolean {
    return status === "posted" || status === "POSTED" || status === "reversed"
}

function isInApprovalReceiptStatus(status?: string): boolean {
    return (
        status === "IN_APPROVAL" ||
        status === "in_approval" ||
        status === "pending_review"
    )
}

/**
 * 把已创建回款草稿写回会话，供绑定卡只读展示。
 *
 * @param session 当前核销会话。
 * @param receipt 服务端回款视图。
 */
function rememberCreatedReceipt(
    session: AllocationSessionView,
    receipt: BackendCustomerReceipt,
): AllocationSessionView {
    const next: AllocationSessionView = {
        ...session,
        existingFactId: receipt.id,
        existingFactNo: receipt.receipt_no,
        existingFactVersion: receipt.version,
        approval: mapCustomerReceiptApproval(receipt.approval),
    }
    sessions.set(session.draftSessionId, next)
    return next
}

/**
 * 创建客户回款草稿并只读展示服务端绑定。提交仍走独立确认。
 *
 * @param session 当前核销会话。
 */
async function createCustomerReceiptDraft(
    session: AllocationSessionView,
    idempotencyKey: string,
): Promise<BackendCustomerReceipt> {
    const amount = session.fact.amount ?? "0"
    const receivedAtLocal = session.fact.receivedAt
    const receivedAtSecs = receivedAtLocal
        ? Math.floor(new Date(receivedAtLocal).getTime() / 1000)
        : Math.floor(Date.now() / 1000)
    const receiptNo = `SK-${new Date().toISOString().slice(0, 10).replaceAll("-", "")}-${idempotencyKey.slice(-6)}`
    return apiPost<BackendCustomerReceipt>("/admin/customer-receipts", {
        receipt_no: receiptNo,
        counterparty_party_id: session.counterpartyPartyId,
        customer_id: session.customerId || undefined,
        received_at: receivedAtSecs,
        amount,
        bank_reference: session.fact.bankReference || undefined,
    })
}

/**
 * 确保回款草稿已在服务端创建并带回只读审批绑定。
 *
 * 已存在草稿时只刷新版本与绑定，不得调用过账旁路。
 *
 * @param input 当前核销会话标识。
 */
export async function ensureCustomerReceiptDraft(input: {
    draftSessionId: string
    editVersion: number
    idempotencyKey: string
}): Promise<
    | {
          status: "succeeded"
          session: AllocationSessionView
      }
    | Extract<PostAllocationResult, { status: "failed" }>
> {
    const session = sessions.get(input.draftSessionId)
    if (!session || session.status !== "draft") {
        return failedResult("SESSION_INVALID", "本次核销已不存在或已提交。")
    }
    if (input.editVersion !== session.editVersion) {
        return failedResult(
            "VERSION_CONFLICT",
            "草稿数据已更新，请保存或刷新后重试。",
        )
    }
    if (session.mode !== "receipt") {
        return failedResult("NOT_RECEIPT", "当前核销不是回款。")
    }

    if (session.existingFactId) {
        const latest = await apiGet<BackendCustomerReceipt>(
            `/admin/customer-receipts/${encodeURIComponent(session.existingFactId)}`,
        )
        if (isPostedReceiptStatus(latest.status)) {
            return failedResult(
                "RECEIPT_ALREADY_POSTED",
                "已过账回款不得再次过账。未分配余额请另登新回款。",
            )
        }
        if (isInApprovalReceiptStatus(latest.status)) {
            return failedResult(
                "RECEIPT_IN_APPROVAL",
                "回款正在审批中，不能重复提交。",
            )
        }
        return {
            status: "succeeded",
            session: rememberCreatedReceipt(session, latest),
        }
    }

    const created = await createCustomerReceiptDraft(
        session,
        input.idempotencyKey,
    )
    return {
        status: "succeeded",
        session: rememberCreatedReceipt(session, created),
    }
}

export async function postAllocation(
    input: PostAllocationInput,
): Promise<PostAllocationResult> {
    const cached = postIdempotency.get(input.idempotencyKey)
    if (cached) return cached

    const s = sessions.get(input.draftSessionId)
    if (!s || s.status !== "draft") {
        const failed: PostAllocationResult = {
            status: "failed",
            code: "SESSION_INVALID",
            message: "本次核销已不存在或已提交。",
        }
        postIdempotency.set(input.idempotencyKey, failed)
        return failed
    }
    if (input.editVersion !== s.editVersion) {
        return {
            status: "failed",
            code: "VERSION_CONFLICT",
            message: "草稿数据已更新，请保存或刷新后重试。",
        }
    }

    const poolTargets = new Set(
        s.pool.map((target) => `${target.targetKind}:${target.targetId}`),
    )
    if (
        s.allocations.some(
            (line) => !poolTargets.has(`${line.targetKind}:${line.targetId}`),
        )
    ) {
        return failedResult(
            "TARGET_OUT_OF_SCOPE",
            "分配目标不属于本次核销范围，请刷新后重新选择。",
        )
    }

    const positiveLines = s.allocations.filter(
        (a) => a.amount && Number(a.amount) > 0,
    )

    try {
        if (s.mode === "receipt") {
            if (positiveLines.length === 0) {
                return failedResult(
                    "NEED_ALLOCATION",
                    "提交审批至少需要一条核销分配。",
                )
            }

            const ensured = await ensureCustomerReceiptDraft({
                draftSessionId: input.draftSessionId,
                editVersion: input.editVersion,
                idempotencyKey: input.idempotencyKey,
            })
            if (ensured.status !== "succeeded") {
                return ensured
            }
            const draft = ensured.session
            const factId = draft.existingFactId
            const expectedVersion = draft.existingFactVersion
            if (!factId || !expectedVersion) {
                return failedResult(
                    "RECEIPT_DRAFT_MISSING",
                    "回款草稿尚未创建，请刷新后重试。",
                )
            }

            const submitted = await apiPost<BackendCustomerReceipt>(
                `/admin/customer-receipts/${encodeURIComponent(factId)}/submit`,
                buildCustomerReceiptSubmitRequest({
                    expectedVersion,
                    idempotencyKey: input.idempotencyKey,
                    allocations: positiveLines.map((line) => ({
                        receivableEntryId: line.targetId,
                        allocatedAmount: line.amount,
                    })),
                }),
            )
            const approval = mapCustomerReceiptApproval(submitted.approval)
            sessions.set(input.draftSessionId, {
                ...draft,
                status: "posted",
                existingFactId: submitted.id,
                existingFactNo: submitted.receipt_no,
                existingFactVersion: submitted.version,
                approval,
            })
            const result: PostAllocationResult = {
                status: "succeeded",
                mode: "receipt",
                factId: submitted.id,
                factNo: submitted.receipt_no,
                allocatedTotal: submitted.allocated_total,
                unallocatedAmount: submitted.unallocated_amount,
                operationId: input.idempotencyKey,
                watermark: new Date().toISOString(),
                returnTo: draft.returnContext?.returnTo,
                approval,
                subjectStatus: submitted.status,
            }
            postIdempotency.set(input.idempotencyKey, result)
            return result
        }

        // invoice mode
        let factId = s.existingFactId
        let factNo = s.existingFactNo ?? (s.fact.invoiceNo ?? "").trim()

        if (!factId) {
            const created = stripInvoiceApprovalField(
                await apiPost<BackendInvoice>("/admin/invoices", {
                    invoice_direction: "sales",
                    invoice_kind: s.fact.invoiceKind ?? "blue",
                    party_id: s.counterpartyPartyId,
                    invoice_code: s.fact.invoiceCode?.trim() || undefined,
                    invoice_no: factNo,
                    invoice_date: s.fact.invoiceDate,
                    gross_amount: s.fact.grossAmount ?? "0",
                    net_amount: s.fact.netAmount || s.fact.grossAmount || "0",
                    tax_amount: s.fact.taxAmount || "0",
                }),
            )
            factId = created.id
            factNo = created.invoice_no
        }

        if (positiveLines.length === 0) {
            sessions.set(input.draftSessionId, { ...s, status: "posted" })
            const result: PostAllocationResult = {
                status: "succeeded",
                mode: "invoice",
                factId: factId!,
                factNo,
                allocatedTotal: "0.00",
                unallocatedAmount: s.fact.grossAmount ?? "0",
                operationId: input.idempotencyKey,
                watermark: new Date().toISOString(),
                returnTo: s.returnContext?.returnTo,
            }
            postIdempotency.set(input.idempotencyKey, result)
            return result
        }

        const gross = s.fact.grossAmount ?? "0"
        const net = s.fact.netAmount || gross
        const tax = s.fact.taxAmount || "0"
        const posted = stripInvoiceApprovalField(
            await apiPost<BackendInvoice>(
                `/admin/invoices/${encodeURIComponent(factId!)}/post`,
                {
                    allocations: positiveLines.map((line) => ({
                        receivable_account_id: line.targetId,
                        allocated_gross_amount: line.amount,
                        allocated_net_amount: net,
                        allocated_tax_amount: tax,
                    })),
                },
            ),
        )
        sessions.set(input.draftSessionId, { ...s, status: "posted" })
        const result: PostAllocationResult = {
            status: "succeeded",
            mode: "invoice",
            factId: posted.id,
            factNo: posted.invoice_no,
            allocatedTotal: posted.allocated_total,
            unallocatedAmount: posted.unallocated_amount,
            operationId: input.idempotencyKey,
            watermark: new Date().toISOString(),
            returnTo: s.returnContext?.returnTo,
        }
        postIdempotency.set(input.idempotencyKey, result)
        return result
    } catch (err) {
        const message =
            err && typeof err === "object" && "message" in err
                ? String((err as { message: unknown }).message)
                : "提交失败"
        const code =
            err && typeof err === "object" && "status" in err
                ? String((err as { status?: number }).status ?? "HTTP_ERROR")
                : "HTTP_ERROR"
        const failed: PostAllocationResult = {
            status: "failed",
            code,
            message,
        }
        // Do not cache non-idempotent validation failures under success key
        if (code === "409" || message.includes("已存在")) {
            postIdempotency.set(input.idempotencyKey, failed)
        }
        return failed
    }
}

export async function resolvePostUnknown(
    idempotencyKey: string,
): Promise<PostAllocationResult | null> {
    return postIdempotency.get(idempotencyKey) ?? null
}
