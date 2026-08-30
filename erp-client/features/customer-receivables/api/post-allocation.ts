/** Post allocation (draft session → formal receipt/invoice allocations). */

import { apiPost } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { compareDecimal } from "@/lib/fixed-decimal"

import type {
    AllocationSessionView,
    PostAllocationInput,
    PostAllocationResult,
} from "@/features/customer-receivables/types"
import { mapCustomerReceiptApproval } from "@/features/customer-receivables/lib/customer-receipt-approval"
import { stripInvoiceApprovalField } from "@/features/customer-receivables/lib/invoice-no-approval"
import type { BackendCustomerReceipt, BackendInvoice } from "./dto"

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
    session: AllocationSessionView | null,
): Promise<PostAllocationResult> {
    const commandInput = freezePostInput(input)
    if (!session || session.status !== "draft") {
        return {
            status: "failed",
            code: "SESSION_INVALID",
            message: "本次核销草稿已不存在或已提交。请重新进入核销页面。",
        }
    }
    if (commandInput.editVersion !== session.editVersion) {
        return {
            status: "failed",
            code: "VERSION_CONFLICT",
            message: "草稿数据已更新，请保存或刷新后重试。",
        }
    }
    let s: AllocationSessionView = {
        ...session,
        fact: { ...commandInput.fact },
        allocations: commandInput.allocations.map((line) => ({ ...line })),
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

    const positiveLines = s.allocations.filter((line) => {
        try {
            return (
                Boolean(line.amount) && compareDecimal(line.amount, "0", 2) > 0
            )
        } catch {
            return false
        }
    })

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
            return unknown
        }
        const failed: PostAllocationResult = {
            status: "failed",
            code,
            message,
        }
        return failed
    }
}

export async function resolvePostUnknown(
    input: PostAllocationInput,
    session: AllocationSessionView | null,
): Promise<PostAllocationResult | null> {
    return postAllocation(input, session)
}
