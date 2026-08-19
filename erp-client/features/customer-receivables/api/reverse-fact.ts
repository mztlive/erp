/** Reverse facts (receipt reversal / refund / red invoice) — append-only. */

import { apiGet, apiPost } from "@/lib/api"

import type {
    ReverseFactInput,
    ReverseFactResult,
} from "@/features/customer-receivables/types"
import { buildCustomerRefundSubmitRequest } from "@/features/customer-receivables/lib/customer-refund-approval"
import { projectCustomerRefund } from "./mappers"
import type {
    BackendCustomerReceipt,
    BackendCustomerRefund,
    BackendInvoice,
} from "./dto"

export const reverseIdempotency = new Map<string, ReverseFactResult>()

export const refundDrafts = new Map<string, BackendCustomerRefund>()

function failedResult(
    code: string,
    message: string,
): Extract<ReverseFactResult, { status: "failed" }> {
    return { status: "failed", code, message }
}

function isPostedRefundStatus(status?: string): boolean {
    return status === "posted" || status === "POSTED" || status === "reversed"
}

function isInApprovalRefundStatus(status?: string): boolean {
    return (
        status === "IN_APPROVAL" ||
        status === "in_approval" ||
        status === "pending_review"
    )
}

/**
 * 按原回款创建客户退款草稿，只读带回服务端审批绑定。
 *
 * 已存在草稿时只刷新版本与绑定，不得调用过账旁路。
 *
 * @param input 退款草稿创建所需字段。
 */
export async function ensureCustomerRefundDraft(input: {
    sourceFactId: string
    amount?: string
    reason: string
    idempotencyKey: string
}): Promise<
    | {
          status: "succeeded"
          refund: ReturnType<typeof projectCustomerRefund>
      }
    | Extract<ReverseFactResult, { status: "failed" }>
> {
    const cached = refundDrafts.get(input.idempotencyKey)
    if (cached) {
        const latest = await apiGet<BackendCustomerRefund>(
            `/admin/customer-refunds/${encodeURIComponent(cached.id)}`,
        )
        if (isPostedRefundStatus(latest.status)) {
            return failedResult(
                "REFUND_ALREADY_POSTED",
                "已过账退款不得再次提交。",
            )
        }
        if (isInApprovalRefundStatus(latest.status)) {
            return failedResult(
                "REFUND_IN_APPROVAL",
                "退款正在审批中，不能重复提交。",
            )
        }
        refundDrafts.set(input.idempotencyKey, latest)
        return {
            status: "succeeded",
            refund: projectCustomerRefund(latest),
        }
    }

    const receipt = await apiGet<BackendCustomerReceipt>(
        `/admin/customer-receipts/${encodeURIComponent(input.sourceFactId)}`,
    )
    const customerId = receipt.customer_id
    if (!customerId) {
        return failedResult(
            "CUSTOMER_REQUIRED",
            "回款未关联经营客户，无法登记退款。",
        )
    }
    const nowSecs = Math.floor(Date.now() / 1000)
    const noSuffix = input.idempotencyKey.slice(-8)
    const created = await apiPost<BackendCustomerRefund>(
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
    refundDrafts.set(input.idempotencyKey, created)
    return {
        status: "succeeded",
        refund: projectCustomerRefund(created),
    }
}

/**
 * 提交客户退款并启动审批。不得选择定义、下一节点或审批人。
 *
 * @param input 提交所需版本与幂等键。
 */
export async function submitCustomerRefund(input: {
    refundId: string
    expectedVersion: number
    idempotencyKey: string
}): Promise<
    | {
          status: "succeeded"
          refund: ReturnType<typeof projectCustomerRefund>
      }
    | Extract<ReverseFactResult, { status: "failed" }>
> {
    const submitted = await apiPost<BackendCustomerRefund>(
        `/admin/customer-refunds/${encodeURIComponent(input.refundId)}/submit`,
        buildCustomerRefundSubmitRequest({
            expectedVersion: input.expectedVersion,
            idempotencyKey: input.idempotencyKey,
        }),
    )
    return {
        status: "succeeded",
        refund: projectCustomerRefund(submitted),
    }
}

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
            const ensured = await ensureCustomerRefundDraft({
                sourceFactId: input.sourceFactId,
                amount: input.amount,
                reason: input.reason,
                idempotencyKey: input.idempotencyKey,
            })
            if (ensured.status !== "succeeded") {
                reverseIdempotency.set(input.idempotencyKey, ensured)
                return ensured
            }
            const submitted = await submitCustomerRefund({
                refundId: ensured.refund.refundId,
                expectedVersion: ensured.refund.baselineVersion,
                idempotencyKey: input.idempotencyKey,
            })
            if (submitted.status !== "succeeded") {
                return submitted
            }
            const result: ReverseFactResult = {
                status: "succeeded",
                reverseFactId: submitted.refund.refundId,
                reverseFactNo: submitted.refund.refundNo,
                operationId: input.idempotencyKey,
                message: "已提交客户退款审批，原回款保留。",
                approval: submitted.refund.approval,
                subjectStatus: submitted.refund.status,
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
