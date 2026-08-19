/** Reverse facts (receipt reversal / refund / red invoice) — append-only. */

import { apiGet, apiPost } from "@/lib/api"

import type {
    ReverseFactInput,
    ReverseFactResult,
} from "@/features/customer-receivables/types"
import { buildCustomerRefundSubmitRequest } from "@/features/customer-receivables/lib/customer-refund-approval"
import { buildReceiptReversalSubmitRequest } from "@/features/customer-receivables/lib/receipt-reversal-approval"
import { projectCustomerRefund, projectReceiptReversal } from "./mappers"
import type {
    BackendCustomerReceipt,
    BackendCustomerRefund,
    BackendInvoice,
    BackendReceiptReversal,
} from "./dto"

export const reverseIdempotency = new Map<string, ReverseFactResult>()

export const refundDrafts = new Map<string, BackendCustomerRefund>()

export const reversalDrafts = new Map<string, BackendReceiptReversal>()

/**
 * 丢弃指定幂等键下的退款草稿缓存。
 *
 * 来源单据或原因变化后必须调用，避免把上一张退款单当成当前意图。
 *
 * @param idempotencyKey 即将作废的幂等键。
 */
export const forgetRefundDraft = (idempotencyKey: string): void => {
    refundDrafts.delete(idempotencyKey)
}

/**
 * 丢弃指定幂等键下的回款冲正草稿缓存。
 *
 * 来源单据或原因变化后必须调用，避免把上一张冲正单当成当前意图。
 *
 * @param idempotencyKey 即将作废的幂等键。
 */
export const forgetReversalDraft = (idempotencyKey: string): void => {
    reversalDrafts.delete(idempotencyKey)
}

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
        const cachedSource = cached.original_receipt_id ?? undefined
        if (
            (cachedSource && cachedSource !== input.sourceFactId) ||
            cached.reason_text !== input.reason
        ) {
            forgetRefundDraft(input.idempotencyKey)
            return failedResult(
                "REFUND_INTENT_MISMATCH",
                "当前退款草稿已不是这次提交意图，请重新发起。",
            )
        }
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
function isPostedReversalStatus(status?: string): boolean {
    return status === "posted" || status === "POSTED" || status === "reversed"
}

function isInApprovalReversalStatus(status?: string): boolean {
    return (
        status === "IN_APPROVAL" ||
        status === "in_approval" ||
        status === "pending_review"
    )
}

/**
 * 按原回款创建回款冲正草稿，只读带回服务端审批绑定。
 *
 * 已存在草稿时只刷新版本与绑定，不得调用过账旁路。
 *
 * @param input 冲正草稿创建所需字段。
 */
export async function ensureReceiptReversalDraft(input: {
    sourceFactId: string
    amount?: string
    reason: string
    idempotencyKey: string
}): Promise<
    | {
          status: "succeeded"
          reversal: ReturnType<typeof projectReceiptReversal>
      }
    | Extract<ReverseFactResult, { status: "failed" }>
> {
    const cached = reversalDrafts.get(input.idempotencyKey)
    if (cached) {
        if (
            cached.original_customer_receipt_id !== input.sourceFactId ||
            cached.reason_text !== input.reason
        ) {
            forgetReversalDraft(input.idempotencyKey)
            return failedResult(
                "REVERSAL_INTENT_MISMATCH",
                "当前冲正草稿已不是这次提交意图，请重新发起。",
            )
        }
        const latest = await apiGet<BackendReceiptReversal>(
            `/admin/receipt-reversals/${encodeURIComponent(cached.id)}`,
        )
        if (isPostedReversalStatus(latest.status)) {
            return failedResult(
                "REVERSAL_ALREADY_POSTED",
                "已过账冲正不得再次提交。",
            )
        }
        if (isInApprovalReversalStatus(latest.status)) {
            return failedResult(
                "REVERSAL_IN_APPROVAL",
                "冲正正在审批中，不能重复提交。",
            )
        }
        reversalDrafts.set(input.idempotencyKey, latest)
        return {
            status: "succeeded",
            reversal: projectReceiptReversal(latest),
        }
    }

    const receipt = await apiGet<BackendCustomerReceipt>(
        `/admin/customer-receipts/${encodeURIComponent(input.sourceFactId)}`,
    )
    const nowSecs = Math.floor(Date.now() / 1000)
    const noSuffix = input.idempotencyKey.slice(-8)
    const created = await apiPost<BackendReceiptReversal>(
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
    reversalDrafts.set(input.idempotencyKey, created)
    return {
        status: "succeeded",
        reversal: projectReceiptReversal(created),
    }
}

/**
 * 提交回款冲正并启动审批。不得选择定义、下一节点或审批人，也不得调用过账旁路。
 *
 * @param input 提交所需版本与幂等键。
 */
export async function submitReceiptReversal(input: {
    reversalId: string
    expectedVersion: number
    idempotencyKey: string
}): Promise<
    | {
          status: "succeeded"
          reversal: ReturnType<typeof projectReceiptReversal>
      }
    | Extract<ReverseFactResult, { status: "failed" }>
> {
    const submitted = await apiPost<BackendReceiptReversal>(
        `/admin/receipt-reversals/${encodeURIComponent(input.reversalId)}/submit`,
        buildReceiptReversalSubmitRequest({
            expectedVersion: input.expectedVersion,
            idempotencyKey: input.idempotencyKey,
        }),
    )
    return {
        status: "succeeded",
        reversal: projectReceiptReversal(submitted),
    }
}

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

    try {
        if (input.kind === "receipt_reverse") {
            const ensured = await ensureReceiptReversalDraft({
                sourceFactId: input.sourceFactId,
                amount: input.amount,
                reason: input.reason,
                idempotencyKey: input.idempotencyKey,
            })
            if (ensured.status !== "succeeded") {
                reverseIdempotency.set(input.idempotencyKey, ensured)
                return ensured
            }
            const submitted = await submitReceiptReversal({
                reversalId: ensured.reversal.reversalId,
                expectedVersion: ensured.reversal.baselineVersion,
                idempotencyKey: input.idempotencyKey,
            })
            if (submitted.status !== "succeeded") {
                return submitted
            }
            const result: ReverseFactResult = {
                status: "succeeded",
                reverseFactId: submitted.reversal.reversalId,
                reverseFactNo: submitted.reversal.reversalNo,
                operationId: input.idempotencyKey,
                message: "已提交回款冲正审批，原回款保留。",
                approval: submitted.reversal.approval,
                subjectStatus: submitted.reversal.status,
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
