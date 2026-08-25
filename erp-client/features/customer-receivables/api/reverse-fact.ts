/** Reverse facts (receipt reversal / refund / red invoice) — append-only. */

import { apiPost } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import { classifyFormalCommandError } from "@/lib/formal-command"

import type {
    ReverseFactInput,
    ReverseFactResult,
} from "@/features/customer-receivables/types"
import { buildCustomerRefundSubmitRequest } from "@/features/customer-receivables/lib/customer-refund-approval"
import { buildReceiptReversalSubmitRequest } from "@/features/customer-receivables/lib/receipt-reversal-approval"
import { projectCustomerRefund, projectReceiptReversal } from "./mappers"
import type {
    BackendCustomerRefund,
    BackendInvoice,
    BackendReceiptReversal,
} from "./dto"

export const reverseIdempotency = new Map<string, ReverseFactResult>()
const pendingReverseInputs = new Map<string, ReverseFactInput>()

function freezeReverseInput(input: ReverseFactInput): ReverseFactInput {
    return { ...input }
}

function correctionError(
    error: unknown,
    idempotencyKey: string,
): Extract<ReverseFactResult, { status: "failed" | "unknown" }> {
    const message = getErrorMessage(error, "纠错提交失败，请稍后重试。")
    const code =
        error && typeof error === "object" && "code" in error
            ? String((error as { code?: string }).code ?? "HTTP_ERROR")
            : "HTTP_ERROR"
    if (
        code === "OUTCOME_UNKNOWN" ||
        classifyFormalCommandError(error) === "unknown"
    ) {
        return { status: "unknown", message, idempotencyKey }
    }
    return { status: "failed", code, message }
}

/** 提交已有客户退款草稿并启动审批。 */
export async function submitCustomerRefund(input: {
    refundId: string
    expectedVersion: number
    idempotencyKey: string
}): Promise<
    | {
          status: "succeeded"
          refund: ReturnType<typeof projectCustomerRefund>
      }
    | Extract<ReverseFactResult, { status: "failed" | "unknown" }>
> {
    try {
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
    } catch (error) {
        return correctionError(error, input.idempotencyKey)
    }
}

/** 提交已有回款冲正草稿并启动审批。 */
export async function submitReceiptReversal(input: {
    reversalId: string
    expectedVersion: number
    idempotencyKey: string
}): Promise<
    | {
          status: "succeeded"
          reversal: ReturnType<typeof projectReceiptReversal>
      }
    | Extract<ReverseFactResult, { status: "failed" | "unknown" }>
> {
    try {
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
    } catch (error) {
        return correctionError(error, input.idempotencyKey)
    }
}

export async function reverseFact(
    input: ReverseFactInput,
): Promise<ReverseFactResult> {
    const cached = reverseIdempotency.get(input.idempotencyKey)
    if (cached && cached.status !== "unknown") return cached
    reverseIdempotency.delete(input.idempotencyKey)
    const commandInput =
        pendingReverseInputs.get(input.idempotencyKey) ??
        freezeReverseInput(input)
    pendingReverseInputs.set(commandInput.idempotencyKey, commandInput)

    try {
        if (commandInput.kind === "receipt_reverse") {
            const submitted = projectReceiptReversal(
                await apiPost<BackendReceiptReversal>(
                    "/admin/receipt-reversals/commit",
                    {
                        source_fact_id: commandInput.sourceFactId,
                        amount: commandInput.amount,
                        reason: commandInput.reason,
                        idempotency_key: commandInput.idempotencyKey,
                    },
                ),
            )
            const result: ReverseFactResult = {
                status: "succeeded",
                reverseFactId: submitted.reversalId,
                reverseFactNo: submitted.reversalNo,
                operationId: commandInput.idempotencyKey,
                message: "已提交回款冲正审批，原回款保留。",
                approval: submitted.approval,
                subjectStatus: submitted.status,
            }
            reverseIdempotency.set(commandInput.idempotencyKey, result)
            pendingReverseInputs.delete(commandInput.idempotencyKey)
            return result
        }

        if (commandInput.kind === "refund") {
            const submitted = projectCustomerRefund(
                await apiPost<BackendCustomerRefund>(
                    "/admin/customer-refunds/commit",
                    {
                        source_fact_id: commandInput.sourceFactId,
                        amount: commandInput.amount,
                        reason: commandInput.reason,
                        idempotency_key: commandInput.idempotencyKey,
                    },
                ),
            )
            const result: ReverseFactResult = {
                status: "succeeded",
                reverseFactId: submitted.refundId,
                reverseFactNo: submitted.refundNo,
                operationId: commandInput.idempotencyKey,
                message: "已提交客户退款审批，原回款保留。",
                approval: submitted.approval,
                subjectStatus: submitted.status,
            }
            reverseIdempotency.set(commandInput.idempotencyKey, result)
            pendingReverseInputs.delete(commandInput.idempotencyKey)
            return result
        }

        // red_invoice
        const red = await apiPost<BackendInvoice>(
            `/admin/invoices/${encodeURIComponent(commandInput.sourceFactId)}/red-issue`,
            {
                amount: commandInput.amount || undefined,
                reason: commandInput.reason,
                idempotency_key: commandInput.idempotencyKey,
            },
        )
        const result: ReverseFactResult = {
            status: "succeeded",
            reverseFactId: red.id,
            reverseFactNo: red.invoice_no,
            operationId: commandInput.idempotencyKey,
            message: "已登记独立红票并追加反向分配，原蓝票保留。",
        }
        reverseIdempotency.set(commandInput.idempotencyKey, result)
        pendingReverseInputs.delete(commandInput.idempotencyKey)
        return result
    } catch (err) {
        const result = correctionError(err, commandInput.idempotencyKey)
        if (result.status === "unknown") {
            reverseIdempotency.set(commandInput.idempotencyKey, result)
            return result
        }
        pendingReverseInputs.delete(commandInput.idempotencyKey)
        return result
    }
}
