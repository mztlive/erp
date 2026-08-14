/**
 * W12 供应商往来 · 付款相关请求（登记+核销提交、冲正）。
 * 幂等结果缓存见 api/shared。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { BackendSupplierPayment } from "@/features/supplier-payables/api/mappers"
import {
    errorMessage,
    sessions,
    submitIdempotency,
} from "@/features/supplier-payables/api/shared"
import type {
    FormalSubmitResult,
    PostPaymentInput,
    ReversePaymentInput,
} from "@/features/supplier-payables/types"

export async function submitPayment(
    input: PostPaymentInput,
): Promise<FormalSubmitResult> {
    const cached = submitIdempotency.get(input.idempotencyKey)
    if (cached) return cached

    try {
        let paymentId = input.existingPaymentId
        let paymentNo = ""

        if (!paymentId) {
            const paidAtSecs = input.paidAt
                ? Math.floor(new Date(input.paidAt).getTime() / 1000)
                : Math.floor(Date.now() / 1000)
            const created = await apiPost<BackendSupplierPayment>(
                "/admin/supplier-payments",
                {
                    payment_no: `FK-${input.idempotencyKey.slice(-8)}`,
                    supplier_id: input.supplierId,
                    paid_at: paidAtSecs,
                    amount: input.amount,
                    bank_reference: input.bankReference || undefined,
                },
            )
            paymentId = created.id
            paymentNo = created.payment_no
        }

        const targets = input.targets.filter(
            (t) => t.amount && Number(t.amount) > 0,
        )
        if (targets.length > 0) {
            const posted = await apiPost<BackendSupplierPayment>(
                `/admin/supplier-payments/${encodeURIComponent(paymentId)}/post`,
                {
                    allocations: targets.map((t) => ({
                        payable_entry_id:
                            t.payableEntryId ?? t.payableAccountId,
                        allocated_amount: t.amount,
                    })),
                },
            )
            paymentNo = posted.payment_no
            const result: FormalSubmitResult = {
                status: "succeeded",
                title: "付款已确认",
                description: "付款与核销已提交。",
                reference: posted.payment_no,
                operationId: input.idempotencyKey,
                documentNo: posted.payment_no,
                unallocatedAmount: posted.unallocated_amount,
                allocatedTotal: posted.allocated_total,
                returnTo: sessions.get(input.draftSessionId)?.returnTo,
            }
            submitIdempotency.set(input.idempotencyKey, result)
            return result
        }

        const result: FormalSubmitResult = {
            status: "succeeded",
            title: "付款草稿已创建",
            description: "未分配核销行；付款草稿已登记。",
            reference: paymentNo || paymentId,
            operationId: input.idempotencyKey,
            documentNo: paymentNo || paymentId,
            unallocatedAmount: input.amount,
            allocatedTotal: "0.00",
        }
        submitIdempotency.set(input.idempotencyKey, result)
        return result
    } catch (err) {
        return {
            status: "failed",
            title: "付款失败",
            description: errorMessage(err, "付款提交失败"),
            errorCode: "HTTP_ERROR",
        }
    }
}

export async function reversePayment(
    input: ReversePaymentInput,
): Promise<FormalSubmitResult> {
    const cached = submitIdempotency.get(input.idempotencyKey)
    if (cached) return cached
    try {
        const payment = await apiGet<BackendSupplierPayment>(
            `/admin/supplier-payments/${encodeURIComponent(input.paymentId)}`,
        )
        const nowSecs = Math.floor(Date.now() / 1000)
        const created = await apiPost<{ id: string; reversal_no: string }>(
            "/admin/payment-reversals",
            {
                reversal_no: `PCZ-${input.idempotencyKey.slice(-8)}`,
                original_supplier_payment_id: input.paymentId,
                reason_text: input.reason,
                amount: payment.amount,
                handled_by: "finance_handler",
                reviewed_by: "finance_reviewer",
                occurred_at: nowSecs,
            },
        )
        const posted = await apiPost<{ id: string; reversal_no: string }>(
            `/admin/payment-reversals/${encodeURIComponent(created.id)}/post`,
            {},
        )
        const result: FormalSubmitResult = {
            status: "succeeded",
            title: "付款冲正已完成",
            description: "已追加付款冲正记录，原付款保留。",
            reference: posted.reversal_no,
            operationId: input.idempotencyKey,
            documentNo: posted.reversal_no,
        }
        submitIdempotency.set(input.idempotencyKey, result)
        return result
    } catch (err) {
        return {
            status: "failed",
            title: "冲正失败",
            description: errorMessage(err, "付款冲正失败"),
            errorCode: "HTTP_ERROR",
        }
    }
}
