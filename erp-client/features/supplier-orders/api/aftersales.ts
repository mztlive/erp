/**
 * W26 供应商订单 · 售后动作（取消 / 退款提交）。
 */

import { apiPost } from "@/lib/api"
import type {
    AfterSalesActionInput,
    AfterSalesActionResult,
    FormalActionResponse,
} from "@/features/supplier-orders/types"
import { asCancel, asRefund } from "./mapping"
import type { BackendSubmitResult } from "./wire-types"

export async function submitAfterSalesAction(
    input: AfterSalesActionInput,
): Promise<FormalActionResponse<AfterSalesActionResult>> {
    const path =
        input.action === "CANCEL"
            ? `/admin/supplier-fulfillment-orders/${encodeURIComponent(input.orderId)}/cancel`
            : `/admin/supplier-fulfillment-orders/${encodeURIComponent(input.orderId)}/refund`

    const result = await apiPost<BackendSubmitResult>(path, {
        expected_lock_version: input.expectedLockVersion,
        operation_id: input.operationId,
        idempotency_key: input.idempotencyKey,
        after_sales_request_id: input.afterSalesRequestId,
        lines: [],
        reason_code: input.reasonCode,
        comment: input.comment,
    })

    const order = result.order
    return {
        status: "succeeded",
        message:
            input.action === "CANCEL"
                ? "取消动作已提交供应商"
                : "退款动作已提交供应商",
        reference: result.action?.id,
        operationId: input.operationId,
        data: {
            lockVersion: order.version,
            cancelStatus: asCancel(order.cancel_status),
            refundStatus: asRefund(order.refund_status),
            actionRecordId: result.action?.id ?? input.operationId,
            note: "动作已登记",
        },
    }
}
