/** Reverse facts (receipt reversal / refund / red invoice) — append-only. */

import { apiGet, apiPost } from "@/lib/api"

import type {
    ReverseFactInput,
    ReverseFactResult,
} from "@/features/customer-receivables/types"
import type { BackendCustomerReceipt, BackendInvoice } from "./dto"

export const reverseIdempotency = new Map<string, ReverseFactResult>()

export async function reverseFact(
    input: ReverseFactInput,
): Promise<ReverseFactResult> {
    const cached = reverseIdempotency.get(input.idempotencyKey)
    if (cached) return cached

    const nowSecs = Math.floor(Date.now() / 1000)
    const noSuffix = input.idempotencyKey.slice(-8)

    try {
        if (input.kind === "receipt_reverse") {
            const receipt = await apiGet<BackendCustomerReceipt>(
                `/admin/customer-receipts/${encodeURIComponent(input.sourceFactId)}`,
            )
            const created = await apiPost<{ id: string; reversal_no: string }>(
                "/admin/receipt-reversals",
                {
                    reversal_no: `CZ-${noSuffix}`,
                    original_customer_receipt_id: input.sourceFactId,
                    reason_text: input.reason,
                    amount: input.amount ?? receipt.amount,
                    handled_by: "finance_handler",
                    reviewed_by: "finance_reviewer",
                    occurred_at: nowSecs,
                },
            )
            const posted = await apiPost<{ id: string; reversal_no: string }>(
                `/admin/receipt-reversals/${encodeURIComponent(created.id)}/post`,
                {},
            )
            const result: ReverseFactResult = {
                status: "succeeded",
                reverseFactId: posted.id,
                reverseFactNo: posted.reversal_no,
                operationId: input.idempotencyKey,
                message: "已追加回款冲正记录，原回款保留。",
            }
            reverseIdempotency.set(input.idempotencyKey, result)
            return result
        }

        if (input.kind === "refund") {
            const receipt = await apiGet<BackendCustomerReceipt>(
                `/admin/customer-receipts/${encodeURIComponent(input.sourceFactId)}`,
            )
            const customerId = receipt.customer_id
            if (!customerId) {
                const failed: ReverseFactResult = {
                    status: "failed",
                    code: "CUSTOMER_REQUIRED",
                    message:
                        "回款未关联经营客户，无法登记退款（后端要求 customer_id）。",
                }
                reverseIdempotency.set(input.idempotencyKey, failed)
                return failed
            }
            const created = await apiPost<{ id: string; refund_no: string }>(
                "/admin/customer-refunds",
                {
                    refund_no: `TK-${noSuffix}`,
                    customer_id: customerId,
                    original_receipt_id: input.sourceFactId,
                    reason_text: input.reason,
                    amount: input.amount ?? receipt.amount,
                    handled_by: "finance_handler",
                    reviewed_by: "finance_reviewer",
                    occurred_at: nowSecs,
                },
            )
            const posted = await apiPost<{ id: string; refund_no: string }>(
                `/admin/customer-refunds/${encodeURIComponent(created.id)}/post`,
                {},
            )
            const result: ReverseFactResult = {
                status: "succeeded",
                reverseFactId: posted.id,
                reverseFactNo: posted.refund_no,
                operationId: input.idempotencyKey,
                message: "已追加退款记录，原回款保留。",
            }
            reverseIdempotency.set(input.idempotencyKey, result)
            return result
        }

        // red_invoice
        const inv = await apiGet<BackendInvoice>(
            `/admin/invoices/${encodeURIComponent(input.sourceFactId)}`,
        )
        const applyLines = (inv.allocations ?? []).filter(
            (a) => a.allocation_action === "apply",
        )
        if (applyLines.length === 0) {
            const failed: ReverseFactResult = {
                status: "failed",
                code: "NO_ALLOCATIONS",
                message: "原蓝票无有效分配，无法开具红票。",
            }
            reverseIdempotency.set(input.idempotencyKey, failed)
            return failed
        }
        const red = await apiPost<BackendInvoice>(
            `/admin/invoices/${encodeURIComponent(input.sourceFactId)}/red-issue`,
            {
                invoice_no: `R${inv.invoice_no}`,
                invoice_date: new Date().toISOString().slice(0, 10),
                gross_amount: input.amount ?? inv.gross_amount,
                net_amount: inv.net_amount,
                tax_amount: inv.tax_amount,
                allocations: applyLines.map((a) => ({
                    reverses_allocation_id: a.id,
                    allocated_gross_amount: a.allocated_gross_amount,
                    allocated_net_amount: a.allocated_net_amount,
                    allocated_tax_amount: a.allocated_tax_amount,
                })),
            },
        )
        const result: ReverseFactResult = {
            status: "succeeded",
            reverseFactId: red.id,
            reverseFactNo: red.invoice_no,
            operationId: input.idempotencyKey,
            message: "已登记独立红票并追加反向分配，原蓝票保留。",
        }
        reverseIdempotency.set(input.idempotencyKey, result)
        return result
    } catch (err) {
        const message =
            err && typeof err === "object" && "message" in err
                ? String((err as { message: unknown }).message)
                : "纠错提交失败"
        const failed: ReverseFactResult = {
            status: "failed",
            code: "HTTP_ERROR",
            message,
        }
        return failed
    }
}
