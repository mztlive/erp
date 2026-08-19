/**
 * W12 供应商往来 · 付款冲正草稿创建与提交审批。
 * 过账只由最终通过动作内部消费，本文件不得调用 /post。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { BackendPaymentReversal } from "@/features/supplier-payables/api/mappers"
import { projectPaymentReversal } from "@/features/supplier-payables/api/mappers"
import type { BackendSupplierPayment } from "@/features/supplier-payables/api/mappers"
import { buildPaymentReversalSubmitRequest } from "@/features/supplier-payables/lib/payment-reversal-approval"
import type { PaymentReversalRow } from "@/features/supplier-payables/types"

export const paymentReversalDrafts = new Map<string, BackendPaymentReversal>()

/**
 * 丢弃指定幂等键下的付款冲正草稿缓存。
 *
 * 来源单据或原因变化后必须调用，避免把上一张冲正单当成当前意图。
 *
 * @param idempotencyKey 即将作废的幂等键。
 */
export const forgetPaymentReversalDraft = (idempotencyKey: string): void => {
    paymentReversalDrafts.delete(idempotencyKey)
}

function failedResult(
    code: string,
    message: string,
): { status: "failed"; code: string; message: string } {
    return { status: "failed", code, message }
}

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
 * 读取付款冲正详情，含只读审批投影。缺失时返回 null。
 *
 * @param reversalId 冲正主键。
 */
export async function fetchPaymentReversal(
    reversalId: string,
): Promise<PaymentReversalRow | null> {
    try {
        const reversal = await apiGet<BackendPaymentReversal>(
            `/admin/payment-reversals/${encodeURIComponent(reversalId)}`,
        )
        return projectPaymentReversal(reversal)
    } catch {
        return null
    }
}

/**
 * 按原付款创建付款冲正草稿，只读带回服务端审批绑定。
 *
 * 已存在草稿时只刷新版本与绑定，不得调用过账旁路。
 *
 * @param input 冲正草稿创建所需字段。
 */
export async function ensurePaymentReversalDraft(input: {
    sourcePaymentId: string
    amount?: string
    reason: string
    idempotencyKey: string
}): Promise<
    | { status: "succeeded"; reversal: PaymentReversalRow }
    | { status: "failed"; code: string; message: string }
> {
    const cached = paymentReversalDrafts.get(input.idempotencyKey)
    if (cached) {
        if (
            cached.original_supplier_payment_id !== input.sourcePaymentId ||
            cached.reason_text !== input.reason
        ) {
            forgetPaymentReversalDraft(input.idempotencyKey)
            return failedResult(
                "REVERSAL_INTENT_MISMATCH",
                "当前冲正草稿已不是这次提交意图，请重新发起。",
            )
        }
        const latest = await apiGet<BackendPaymentReversal>(
            `/admin/payment-reversals/${encodeURIComponent(cached.id)}`,
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
        paymentReversalDrafts.set(input.idempotencyKey, latest)
        return {
            status: "succeeded",
            reversal: projectPaymentReversal(latest),
        }
    }

    const payment = await apiGet<BackendSupplierPayment>(
        `/admin/supplier-payments/${encodeURIComponent(input.sourcePaymentId)}`,
    )
    const nowSecs = Math.floor(Date.now() / 1000)
    const noSuffix = input.idempotencyKey.slice(-8)
    const created = await apiPost<BackendPaymentReversal>(
        "/admin/payment-reversals",
        {
            reversal_no: `PCZ-${noSuffix}`,
            original_supplier_payment_id: input.sourcePaymentId,
            reason_text: input.reason,
            amount: input.amount ?? payment.amount,
            handled_by: "finance_handler",
            reviewed_by: "finance_reviewer",
            occurred_at: nowSecs,
        },
    )
    paymentReversalDrafts.set(input.idempotencyKey, created)
    return {
        status: "succeeded",
        reversal: projectPaymentReversal(created),
    }
}

/**
 * 提交付款冲正并启动审批。不得选择定义、下一节点或审批人，也不得调用过账旁路。
 *
 * @param input 提交所需版本与幂等键。
 */
export async function submitPaymentReversal(input: {
    reversalId: string
    expectedVersion: number
    idempotencyKey: string
}): Promise<
    | { status: "succeeded"; reversal: PaymentReversalRow }
    | { status: "failed"; code: string; message: string }
> {
    const submitted = await apiPost<BackendPaymentReversal>(
        `/admin/payment-reversals/${encodeURIComponent(input.reversalId)}/submit`,
        buildPaymentReversalSubmitRequest({
            expectedVersion: input.expectedVersion,
            idempotencyKey: input.idempotencyKey,
        }),
    )
    return {
        status: "succeeded",
        reversal: projectPaymentReversal(submitted),
    }
}
