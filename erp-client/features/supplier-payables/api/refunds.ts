/**
 * W12 供应商往来 · 供应商退款草稿创建与提交审批。
 * 过账只由最终通过动作内部消费，本文件不得调用 /post。
 */

import { apiGet, apiPost } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import type { BackendSupplierRefund } from "@/features/supplier-payables/api/mappers"
import { projectSupplierRefund } from "@/features/supplier-payables/api/mappers"
import { isOutcomeUnknown } from "@/features/supplier-payables/api/shared"
import { buildSupplierRefundSubmitRequest } from "@/features/supplier-payables/lib/supplier-refund-approval"
import type { SupplierRefundRow } from "@/features/supplier-payables/types"

type SupplierRefundMutationResult =
    | { status: "succeeded"; refund: SupplierRefundRow }
    | { status: "failed"; code: string; message: string }
    | { status: "unknown"; message: string; idempotencyKey: string }

function refundError(
    error: unknown,
    idempotencyKey: string,
): Exclude<SupplierRefundMutationResult, { status: "succeeded" }> {
    const message = getErrorMessage(error, "退款提交失败，请稍后重试。")
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
 * 按原付款一次创建供应商退款并启动审批。
 *
 * 服务端在一个事务内完成单据、审批绑定、运行实例、任务与审计写入。
 *
 * @param input 退款提交所需业务意图。
 */
export async function commitSupplierRefund(input: {
    sourcePaymentId: string
    amount?: string
    reason: string
    idempotencyKey: string
}): Promise<SupplierRefundMutationResult> {
    try {
        const submitted = await apiPost<BackendSupplierRefund>(
            "/admin/supplier-refunds/commit",
            {
                source_fact_id: input.sourcePaymentId,
                amount: input.amount,
                reason: input.reason,
                idempotency_key: input.idempotencyKey,
            },
        )
        return {
            status: "succeeded",
            refund: projectSupplierRefund(submitted),
        }
    } catch (error) {
        return refundError(error, input.idempotencyKey)
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
}): Promise<SupplierRefundMutationResult> {
    try {
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
    } catch (error) {
        return refundError(error, input.idempotencyKey)
    }
}
