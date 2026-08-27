/**
 * W12 供应商往来 · 付款相关请求（任务内登记并过账、冲正）。
 * 幂等结果缓存见 api/shared；普通付款不启动独立审批。
 */

import { apiGetBlob, apiPostForm } from "@/lib/api"
import type { BackendSupplierPayment } from "@/features/supplier-payables/api/mappers"
import {
    errorMessage,
    isOutcomeUnknown,
    sessions,
    submitIdempotency,
    submitUnknownResolvers,
} from "@/features/supplier-payables/api/shared"
import { commitPaymentReversal } from "@/features/supplier-payables/api/reversals"
import { BANK_RECEIPT_PENDING_REFERENCE } from "@/features/supplier-payables/lib/allocation-model"
import type {
    FormalSubmitResult,
    PostPaymentInput,
    ReversePaymentInput,
} from "@/features/supplier-payables/types"

function failedPayment(code: string, message: string): FormalSubmitResult {
    return {
        status: "failed",
        title: "付款失败",
        description: message,
        errorCode: code,
    }
}

/**
 * 在当前付款任务内登记付款并过账核销。命令与银行回单一次提交。
 *
 * @param input 提交所需字段。
 */
export async function submitPayment(
    input: PostPaymentInput,
): Promise<FormalSubmitResult> {
    const cached = submitIdempotency.get(input.idempotencyKey)
    if (cached && cached.status !== "unknown") return cached
    submitIdempotency.delete(input.idempotencyKey)

    const commandInput: PostPaymentInput = {
        ...input,
        paidAt: input.paidAt || new Date().toISOString(),
        targets: input.targets.map((target) => ({ ...target })),
    }

    const targets = commandInput.targets.filter(
        (t) => t.amount && Number(t.amount) > 0,
    )
    if (targets.length === 0) {
        return failedPayment(
            "NEED_ALLOCATION",
            "登记付款至少需要一条核销分配。",
        )
    }
    const bankReceiptAssetId = commandInput.bankReceiptFile
        ? BANK_RECEIPT_PENDING_REFERENCE
        : commandInput.bankReceiptAssetId.trim()
    if (!bankReceiptAssetId) {
        return failedPayment("BANK_RECEIPT_REQUIRED", "请上传银行回单图片。")
    }

    try {
        const paidAtSecs = Math.floor(
            new Date(commandInput.paidAt).getTime() / 1000,
        )
        const command = {
            work_item_id: commandInput.workItemId,
            expected_task_version: commandInput.expectedTaskVersion,
            expected_payee_bank_account_id:
                commandInput.expectedPayeeBankAccountId,
            expected_payee_bank_account_version:
                commandInput.expectedPayeeBankAccountVersion,
            payment: {
                payment_no: `FK-${commandInput.idempotencyKey.slice(-8)}`,
                supplier_id: commandInput.supplierId,
                paid_at: paidAtSecs,
                amount: commandInput.amount,
                bank_reference: commandInput.bankReference || undefined,
                bank_receipt_asset_id: bankReceiptAssetId,
            },
            allocations: targets.map((target) => ({
                payable_entry_id:
                    target.payableEntryId ?? target.payableAccountId,
                allocated_amount: target.amount,
            })),
            idempotency_key: commandInput.idempotencyKey,
        }
        const form = new FormData()
        form.append("command", JSON.stringify(command))
        if (commandInput.bankReceiptFile) {
            form.append(
                BANK_RECEIPT_PENDING_REFERENCE,
                commandInput.bankReceiptFile,
                commandInput.bankReceiptFile.name,
            )
        }
        const submitted = await apiPostForm<BackendSupplierPayment>(
            "/admin/supplier-payments/commit",
            form,
        )
        const draft = sessions.get(commandInput.draftSessionId)
        const result: FormalSubmitResult = {
            status: "succeeded",
            title: "付款已登记并过账",
            description: "已形成过账付款记录，并按本次分配核销应付。",
            reference: submitted.payment_no,
            operationId: commandInput.idempotencyKey,
            documentNo: submitted.payment_no,
            existingDocumentId: submitted.id,
            unallocatedAmount: submitted.unallocated_amount,
            allocatedTotal: submitted.allocated_total,
            returnTo: draft?.returnTo,
            subjectStatus: submitted.status,
        }
        submitIdempotency.set(commandInput.idempotencyKey, result)
        submitUnknownResolvers.delete(commandInput.idempotencyKey)
        return result
    } catch (err) {
        if (isOutcomeUnknown(err)) {
            const result: FormalSubmitResult = {
                status: "unknown",
                title: "付款结果待确认",
                description: errorMessage(
                    err,
                    "付款结果暂无法确认，请按操作号查询最终结果。",
                ),
                reference: commandInput.idempotencyKey,
                operationId: commandInput.idempotencyKey,
            }
            submitIdempotency.set(commandInput.idempotencyKey, result)
            submitUnknownResolvers.set(
                commandInput.idempotencyKey,
                async () => {
                    submitIdempotency.delete(commandInput.idempotencyKey)
                    return submitPayment(commandInput)
                },
            )
            return result
        }
        return {
            status: "failed",
            title: "付款失败",
            description: errorMessage(err, "付款提交失败"),
            errorCode: "HTTP_ERROR",
        }
    }
}

/** 读取付款单归属的银行回单图片；后端按付款关系校验并记录审计。 */
export function fetchSupplierPaymentBankReceiptBlob(
    paymentId: string,
): Promise<Blob> {
    return apiGetBlob(
        `/admin/supplier-payments/${encodeURIComponent(paymentId)}/bank-receipt`,
        { timeoutMs: 30_000, cache: "no-store" },
    )
}

/**
 * 一次创建付款冲正并启动审批。过账只由最终通过动作内部消费。
 *
 * @param input 冲正草稿创建所需字段。
 */
export async function reversePayment(
    input: ReversePaymentInput,
): Promise<FormalSubmitResult> {
    const cached = submitIdempotency.get(input.idempotencyKey)
    if (cached && cached.status !== "unknown") return cached
    submitIdempotency.delete(input.idempotencyKey)
    try {
        const committed = await commitPaymentReversal({
            sourcePaymentId: input.paymentId,
            reason: input.reason,
            idempotencyKey: input.idempotencyKey,
        })
        if (committed.status !== "succeeded") {
            if (committed.status === "unknown") {
                const result: FormalSubmitResult = {
                    status: "unknown",
                    title: "冲正结果待确认",
                    description: committed.message,
                    reference: input.idempotencyKey,
                    operationId: input.idempotencyKey,
                }
                submitIdempotency.set(input.idempotencyKey, result)
                submitUnknownResolvers.set(input.idempotencyKey, async () => {
                    submitIdempotency.delete(input.idempotencyKey)
                    return reversePayment(input)
                })
                return result
            }
            return failedPayment(committed.code, committed.message)
        }
        const result: FormalSubmitResult = {
            status: "succeeded",
            title: "付款冲正已提交审批",
            description: "已原子登记付款冲正并启动审批，原付款保留。",
            reference: committed.reversal.reversalNo,
            operationId: input.idempotencyKey,
            documentNo: committed.reversal.reversalNo,
            approval: committed.reversal.approval,
            subjectStatus: committed.reversal.status,
        }
        submitIdempotency.set(input.idempotencyKey, result)
        submitUnknownResolvers.delete(input.idempotencyKey)
        return result
    } catch (err) {
        if (isOutcomeUnknown(err)) {
            const result: FormalSubmitResult = {
                status: "unknown",
                title: "冲正结果待确认",
                description: errorMessage(
                    err,
                    "冲正结果暂无法确认，请按操作号查询最终结果。",
                ),
                reference: input.idempotencyKey,
                operationId: input.idempotencyKey,
            }
            submitIdempotency.set(input.idempotencyKey, result)
            submitUnknownResolvers.set(input.idempotencyKey, async () => {
                submitIdempotency.delete(input.idempotencyKey)
                return reversePayment(input)
            })
            return result
        }
        return {
            status: "failed",
            title: "冲正失败",
            description: errorMessage(err, "付款冲正失败"),
            errorCode: "HTTP_ERROR",
        }
    }
}
