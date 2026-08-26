/** Post allocation (draft session → formal receipt/invoice allocations). */

import { apiPost } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import { classifyFormalCommandError } from "@/lib/formal-command"

import type {
    AllocationSessionView,
    PostAllocationInput,
    PostAllocationResult,
} from "@/features/customer-receivables/types"
import { mapCustomerReceiptApproval } from "@/features/customer-receivables/lib/customer-receipt-approval"
import { stripInvoiceApprovalField } from "@/features/customer-receivables/lib/invoice-no-approval"
import type { BackendCustomerReceipt, BackendInvoice } from "./dto"
import { sessions } from "./session"

export const postIdempotency = new Map<string, PostAllocationResult>()
type PendingPostCommand = {
    input: PostAllocationInput
    session: AllocationSessionView
}

const pendingPostCommands = new Map<string, PendingPostCommand>()

function freezePostInput(input: PostAllocationInput): PostAllocationInput {
    return {
        ...input,
        fact: { ...input.fact },
        allocations: input.allocations.map((line) => ({ ...line })),
    }
}

function failedResult(
    code: string,
    message: string,
): Extract<PostAllocationResult, { status: "failed" }> {
    return { status: "failed", code, message }
}

export async function postAllocation(
    input: PostAllocationInput,
): Promise<PostAllocationResult> {
    const cached = postIdempotency.get(input.idempotencyKey)
    if (cached && cached.status !== "unknown") return cached
    postIdempotency.delete(input.idempotencyKey)

    const pending = pendingPostCommands.get(input.idempotencyKey)
    const commandInput = pending?.input ?? freezePostInput(input)
    let s: AllocationSessionView

    if (pending) {
        s = pending.session
    } else {
        const stored = sessions.get(commandInput.draftSessionId)
        if (!stored || stored.status !== "draft") {
            const failed: PostAllocationResult = {
                status: "failed",
                code: "SESSION_INVALID",
                message: "本次核销已不存在或已提交。",
            }
            postIdempotency.set(commandInput.idempotencyKey, failed)
            return failed
        }
        if (commandInput.editVersion !== stored.editVersion) {
            return {
                status: "failed",
                code: "VERSION_CONFLICT",
                message: "草稿数据已更新，请保存或刷新后重试。",
            }
        }
        s = {
            ...stored,
            fact: { ...commandInput.fact },
            allocations: commandInput.allocations.map((line) => ({ ...line })),
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
            if (!commandInput.fact.receivedAt) {
                commandInput.fact.receivedAt = new Date().toISOString()
                s = {
                    ...s,
                    fact: {
                        ...s.fact,
                        receivedAt: commandInput.fact.receivedAt,
                    },
                }
            }
            pendingPostCommands.set(commandInput.idempotencyKey, {
                input: commandInput,
                session: s,
            })

            const amount = s.fact.amount ?? "0"
            const receivedAt = new Date(commandInput.fact.receivedAt)
            const receivedAtSecs = Math.floor(receivedAt.getTime() / 1000)
            const submitted = await apiPost<BackendCustomerReceipt>(
                "/admin/customer-receipts/commit",
                {
                    receipt_id: s.existingFactId ?? null,
                    expected_version: s.existingFactId
                        ? (s.existingFactVersion ?? null)
                        : null,
                    receipt: s.existingFactId
                        ? null
                        : {
                              receipt_no: `SK-${receivedAt
                                  .toISOString()
                                  .slice(0, 10)
                                  .replaceAll(
                                      "-",
                                      "",
                                  )}-${commandInput.idempotencyKey.slice(-6)}`,
                              counterparty_party_id: s.counterpartyPartyId,
                              customer_id: s.customerId || undefined,
                              received_at: receivedAtSecs,
                              amount,
                              bank_reference: s.fact.bankReference || undefined,
                          },
                    allocations: positiveLines.map((line) => ({
                        receivable_entry_id: line.targetId,
                        allocated_amount: line.amount,
                    })),
                    idempotency_key: commandInput.idempotencyKey,
                },
            )
            const approval = mapCustomerReceiptApproval(submitted.approval)
            sessions.set(commandInput.draftSessionId, {
                ...s,
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
                operationId: commandInput.idempotencyKey,
                watermark: new Date().toISOString(),
                returnTo: s.returnContext?.returnTo,
                approval,
                subjectStatus: submitted.status,
            }
            postIdempotency.set(commandInput.idempotencyKey, result)
            pendingPostCommands.delete(commandInput.idempotencyKey)
            return result
        }

        if (positiveLines.length === 0) {
            return failedResult(
                "NEED_ALLOCATION",
                "登记销项发票至少需要一条正式分配。",
            )
        }
        if (!commandInput.workItemId || !commandInput.expectedTaskVersion) {
            return failedResult(
                "WORK_ITEM_REQUIRED",
                "销项开票必须由当前负责人从工作台开票任务进入。",
            )
        }
        pendingPostCommands.set(commandInput.idempotencyKey, {
            input: commandInput,
            session: s,
        })

        const gross = s.fact.grossAmount ?? "0"
        const net = s.fact.netAmount || gross
        const tax = s.fact.taxAmount || "0"
        const posted = stripInvoiceApprovalField(
            await apiPost<BackendInvoice>("/admin/invoices/commit", {
                work_item_id: commandInput.workItemId,
                expected_task_version: commandInput.expectedTaskVersion,
                invoice_id: s.existingFactId ?? null,
                expected_version: s.existingFactId
                    ? (s.existingFactVersion ?? null)
                    : null,
                invoice: s.existingFactId
                    ? null
                    : {
                          invoice_direction: "sales",
                          invoice_kind: s.fact.invoiceKind ?? "blue",
                          party_id: s.counterpartyPartyId,
                          invoice_code: s.fact.invoiceCode?.trim() || undefined,
                          invoice_no: (s.fact.invoiceNo ?? "").trim(),
                          invoice_date: s.fact.invoiceDate,
                          gross_amount: gross,
                          net_amount: net,
                          tax_amount: tax,
                      },
                allocations: positiveLines.map((line) => ({
                    receivable_account_id: line.targetId,
                    allocated_gross_amount: line.amount,
                    allocated_net_amount: net,
                    allocated_tax_amount: tax,
                })),
                idempotency_key: commandInput.idempotencyKey,
            }),
        )
        sessions.set(commandInput.draftSessionId, { ...s, status: "posted" })
        const result: PostAllocationResult = {
            status: "succeeded",
            mode: "invoice",
            factId: posted.id,
            factNo: posted.invoice_no,
            allocatedTotal: posted.allocated_total,
            unallocatedAmount: posted.unallocated_amount,
            operationId: commandInput.idempotencyKey,
            watermark: new Date().toISOString(),
            returnTo: s.returnContext?.returnTo,
        }
        postIdempotency.set(commandInput.idempotencyKey, result)
        pendingPostCommands.delete(commandInput.idempotencyKey)
        return result
    } catch (err) {
        const message = getErrorMessage(err, "提交失败，请稍后重试。")
        const errorCode =
            err && typeof err === "object" && "code" in err
                ? String((err as { code?: string }).code ?? "HTTP_ERROR")
                : "HTTP_ERROR"
        const code =
            err && typeof err === "object" && "status" in err
                ? String((err as { status?: number }).status ?? "HTTP_ERROR")
                : errorCode
        if (
            errorCode === "OUTCOME_UNKNOWN" ||
            classifyFormalCommandError(err) === "unknown"
        ) {
            const unknown: PostAllocationResult = {
                status: "unknown",
                message,
                idempotencyKey: commandInput.idempotencyKey,
                operationId: commandInput.idempotencyKey,
            }
            postIdempotency.set(commandInput.idempotencyKey, unknown)
            return unknown
        }
        const failed: PostAllocationResult = {
            status: "failed",
            code,
            message,
        }
        // Do not cache non-idempotent validation failures under success key
        if (code === "409" || errorCode === "CONFLICT") {
            postIdempotency.set(commandInput.idempotencyKey, failed)
        }
        pendingPostCommands.delete(commandInput.idempotencyKey)
        return failed
    }
}

export async function resolvePostUnknown(
    idempotencyKey: string,
): Promise<PostAllocationResult | null> {
    const cached = postIdempotency.get(idempotencyKey)
    if (cached?.status !== "unknown") return cached ?? null
    const pending = pendingPostCommands.get(idempotencyKey)
    if (!pending) return cached
    postIdempotency.delete(idempotencyKey)
    return postAllocation(pending.input)
}
