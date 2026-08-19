/**
 * W12 供应商往来 · 供应商退款草稿创建与提交审批。
 * 过账只由最终通过动作内部消费，本文件不得调用 /post。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { BackendSupplierRefund } from "@/features/supplier-payables/api/mappers"
import { projectSupplierRefund } from "@/features/supplier-payables/api/mappers"
import { buildSupplierRefundSubmitRequest } from "@/features/supplier-payables/lib/supplier-refund-approval"
import type { SupplierRefundRow } from "@/features/supplier-payables/types"

export const refundDrafts = new Map<string, BackendSupplierRefund>()

/**
 * 丢弃指定幂等键下的退款草稿缓存。
 *
 * 来源单据或原因变化后必须调用，避免把上一张退款单当成当前意图。
 *
 * @param idempotencyKey 即将作废的幂等键。
 */
export const forgetSupplierRefundDraft = (idempotencyKey: string): void => {
    refundDrafts.delete(idempotencyKey)
}

function failedResult(
    code: string,
    message: string,
): { status: "failed"; code: string; message: string } {
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
 * 读取供应商退款详情，含只读审批投影。缺失时返回 null。
 *
 * @param refundId 退款主键。
 */
export async function fetchSupplierRefund(
    refundId: string,
): Promise<SupplierRefundRow | null> {
    try {
        const refund = await apiGet<BackendSupplierRefund>(
            `/admin/supplier-refunds/${encodeURIComponent(refundId)}`,
        )
        return projectSupplierRefund(refund)
    } catch {
        return null
    }
}

/**
 * 按原付款创建供应商退款草稿，只读带回服务端审批绑定。
 *
 * 已存在草稿时只刷新版本与绑定，不得调用过账旁路。
 *
 * @param input 退款草稿创建所需字段。
 */
export async function ensureSupplierRefundDraft(input: {
    sourcePaymentId: string
    supplierId: string
    amount?: string
    reason: string
    idempotencyKey: string
}): Promise<
    | { status: "succeeded"; refund: SupplierRefundRow }
    | { status: "failed"; code: string; message: string }
> {
    const cached = refundDrafts.get(input.idempotencyKey)
    if (cached) {
        const cachedSource = cached.original_payment_id ?? undefined
        if (
            (cachedSource && cachedSource !== input.sourcePaymentId) ||
            cached.reason_text !== input.reason
        ) {
            forgetSupplierRefundDraft(input.idempotencyKey)
            return failedResult(
                "REFUND_INTENT_MISMATCH",
                "当前退款草稿已不是这次提交意图，请重新发起。",
            )
        }
        const latest = await apiGet<BackendSupplierRefund>(
            `/admin/supplier-refunds/${encodeURIComponent(cached.id)}`,
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
            refund: projectSupplierRefund(latest),
        }
    }

    const nowSecs = Math.floor(Date.now() / 1000)
    const noSuffix = input.idempotencyKey.slice(-8)
    const created = await apiPost<BackendSupplierRefund>(
        "/admin/supplier-refunds",
        {
            refund_no: `GTK-${noSuffix}`,
            supplier_id: input.supplierId,
            original_payment_id: input.sourcePaymentId,
            reason_text: input.reason,
            amount: input.amount,
            handled_by: "finance_handler",
            reviewed_by: "finance_reviewer",
            occurred_at: nowSecs,
        },
    )
    refundDrafts.set(input.idempotencyKey, created)
    return {
        status: "succeeded",
        refund: projectSupplierRefund(created),
    }
}

/**
 * 提交供应商退款并启动审批。不得选择定义、下一节点或审批人。
 *
 * @param input 提交所需版本与幂等键。
 */
export async function submitSupplierRefund(input: {
    refundId: string
    expectedVersion: number
    idempotencyKey: string
}): Promise<
    | { status: "succeeded"; refund: SupplierRefundRow }
    | { status: "failed"; code: string; message: string }
> {
    const submitted = await apiPost<BackendSupplierRefund>(
        `/admin/supplier-refunds/${encodeURIComponent(input.refundId)}/submit`,
        buildSupplierRefundSubmitRequest({
            expectedVersion: input.expectedVersion,
            idempotencyKey: input.idempotencyKey,
        }),
    )
    return {
        status: "succeeded",
        refund: projectSupplierRefund(submitted),
    }
}
