/** Post allocation (draft session → formal receipt/invoice allocations). */

import { apiPost } from "@/lib/api"

import type {
    PostAllocationInput,
    PostAllocationResult,
} from "@/features/customer-receivables/types"
import type { BackendCustomerReceipt, BackendInvoice } from "./dto"
import { sessions } from "./session"

export const postIdempotency = new Map<string, PostAllocationResult>()

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

    const positiveLines = s.allocations.filter(
        (a) => a.amount && Number(a.amount) > 0,
    )

    try {
        if (s.mode === "receipt") {
            let factId = s.existingFactId
            let factNo = s.existingFactNo ?? ""

            if (!factId) {
                const amount = s.fact.amount ?? "0"
                const receivedAtLocal = s.fact.receivedAt
                const receivedAtSecs = receivedAtLocal
                    ? Math.floor(new Date(receivedAtLocal).getTime() / 1000)
                    : Math.floor(Date.now() / 1000)
                const receiptNo = `SK-${new Date().toISOString().slice(0, 10).replaceAll("-", "")}-${input.idempotencyKey.slice(-6)}`
                const created = await apiPost<BackendCustomerReceipt>(
                    "/admin/customer-receipts",
                    {
                        receipt_no: receiptNo,
                        counterparty_party_id: s.counterpartyPartyId,
                        customer_id: s.customerId || undefined,
                        received_at: receivedAtSecs,
                        amount,
                        bank_reference: s.fact.bankReference || undefined,
                    },
                )
                factId = created.id
                factNo = created.receipt_no
            }

            if (positiveLines.length === 0) {
                // Backend requires ≥1 allocation line on post — create without post if no lines
                sessions.set(input.draftSessionId, { ...s, status: "posted" })
                const result: PostAllocationResult = {
                    status: "succeeded",
                    mode: "receipt",
                    factId: factId!,
                    factNo,
                    allocatedTotal: "0.00",
                    unallocatedAmount: s.fact.amount ?? "0",
                    operationId: input.idempotencyKey,
                    watermark: new Date().toISOString(),
                    returnTo: s.returnContext?.returnTo,
                }
                postIdempotency.set(input.idempotencyKey, result)
                return result
            }

            const posted = await apiPost<BackendCustomerReceipt>(
                `/admin/customer-receipts/${encodeURIComponent(factId!)}/post`,
                {
                    allocations: positiveLines.map((line) => ({
                        receivable_entry_id: line.targetId,
                        allocated_amount: line.amount,
                    })),
                },
            )
            sessions.set(input.draftSessionId, { ...s, status: "posted" })
            const result: PostAllocationResult = {
                status: "succeeded",
                mode: "receipt",
                factId: posted.id,
                factNo: posted.receipt_no,
                allocatedTotal: posted.allocated_total,
                unallocatedAmount: posted.unallocated_amount,
                operationId: input.idempotencyKey,
                watermark: new Date().toISOString(),
                returnTo: s.returnContext?.returnTo,
            }
            postIdempotency.set(input.idempotencyKey, result)
            return result
        }

        // invoice mode
        let factId = s.existingFactId
        let factNo = s.existingFactNo ?? (s.fact.invoiceNo ?? "").trim()

        if (!factId) {
            const created = await apiPost<BackendInvoice>("/admin/invoices", {
                invoice_direction: "sales",
                invoice_kind: s.fact.invoiceKind ?? "blue",
                party_id: s.counterpartyPartyId,
                invoice_code: s.fact.invoiceCode?.trim() || undefined,
                invoice_no: factNo,
                invoice_date: s.fact.invoiceDate,
                gross_amount: s.fact.grossAmount ?? "0",
                net_amount: s.fact.netAmount || s.fact.grossAmount || "0",
                tax_amount: s.fact.taxAmount || "0",
            })
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
        const posted = await apiPost<BackendInvoice>(
            `/admin/invoices/${encodeURIComponent(factId!)}/post`,
            {
                allocations: positiveLines.map((line) => ({
                    receivable_account_id: line.targetId,
                    allocated_gross_amount: line.amount,
                    allocated_net_amount: net,
                    allocated_tax_amount: tax,
                })),
            },
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
