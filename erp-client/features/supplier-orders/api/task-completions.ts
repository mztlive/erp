/**
 * W26 供应商订单 · 任务完成（按可验证结果确认完成）。
 */

import { apiPost } from "@/lib/api"
import type {
    CompleteSupplierOrderTaskInput,
    CompleteSupplierOrderTaskResult,
    FormalActionResponse,
} from "@/features/supplier-orders/types"
import type { BackendTaskCompletionResult } from "./wire-types"

export async function completeSupplierOrderTask(
    input: CompleteSupplierOrderTaskInput,
): Promise<FormalActionResponse<CompleteSupplierOrderTaskResult>> {
    const result = await apiPost<BackendTaskCompletionResult>(
        "/admin/supplier-fulfillment-orders/task-completions",
        {
            work_item_id: input.workItemId,
            expected_task_version: input.expectedTaskVersion,
            expected_subject_version: input.expectedSubjectVersion,
            decision: {
                type: input.decision.type,
                order_id: input.decision.orderId,
                expected_order_lock_version:
                    input.decision.expectedOrderLockVersion,
                verified_supplier_action_result_id:
                    input.decision.verifiedSupplierActionResultId,
                resolution: input.decision.resolution,
            },
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        status: "succeeded",
        message: "已根据可验证结果完成任务。",
        reference: result.operation_id,
        operationId: result.operation_id,
        data: {
            operationId: result.operation_id,
            workItemId: result.work_item_id,
            workItemStatus: result.work_item_status,
            taskVersion: String(result.task_version),
            lockVersion: result.order_lock_version,
            resolution: result.resolution,
        },
    }
}
