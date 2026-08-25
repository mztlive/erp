/**
 * W12 供应商往来 · 付款冲正草稿创建与提交审批。
 * 过账只由最终通过动作内部消费，本文件不得调用 /post。
 */

import { apiGet, apiPost } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import type { BackendPaymentReversal } from "@/features/supplier-payables/api/mappers"
import { projectPaymentReversal } from "@/features/supplier-payables/api/mappers"
import { isOutcomeUnknown } from "@/features/supplier-payables/api/shared"
import { buildPaymentReversalSubmitRequest } from "@/features/supplier-payables/lib/payment-reversal-approval"
import type { PaymentReversalRow } from "@/features/supplier-payables/types"

type PaymentReversalMutationResult =
    | { status: "succeeded"; reversal: PaymentReversalRow }
    | { status: "failed"; code: string; message: string }
    | { status: "unknown"; message: string; idempotencyKey: string }

function reversalError(
    error: unknown,
    idempotencyKey: string,
): Exclude<PaymentReversalMutationResult, { status: "succeeded" }> {
    const message = getErrorMessage(error, "冲正提交失败，请稍后重试。")
    if (isOutcomeUnknown(error)) {
        return { status: "unknown", message, idempotencyKey }
    }
    const code =
        error && typeof error === "object" && "code" in error
            ? String((error as { code?: unknown }).code ?? "HTTP_ERROR")
            : "HTTP_ERROR"
    return { status: "failed", code, message }
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
 * 按原付款一次创建付款冲正并启动审批。
 *
 * 服务端在一个事务内完成单据、审批绑定、运行实例、任务与审计写入。
 *
 * @param input 冲正提交所需业务意图。
 */
export async function commitPaymentReversal(input: {
    sourcePaymentId: string
    amount?: string
    reason: string
    idempotencyKey: string
}): Promise<PaymentReversalMutationResult> {
    try {
        const submitted = await apiPost<BackendPaymentReversal>(
            "/admin/payment-reversals/commit",
            {
                source_fact_id: input.sourcePaymentId,
                amount: input.amount,
                reason: input.reason,
                idempotency_key: input.idempotencyKey,
            },
        )
        return {
            status: "succeeded",
            reversal: projectPaymentReversal(submitted),
        }
    } catch (error) {
        return reversalError(error, input.idempotencyKey)
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
}): Promise<PaymentReversalMutationResult> {
    try {
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
    } catch (error) {
        return reversalError(error, input.idempotencyKey)
    }
}
