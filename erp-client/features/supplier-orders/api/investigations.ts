/**
 * W26 供应商订单 · 结果调查（查询原结果 / 安全重发）。
 * 后端集成在 integration_ops 错误任务上；
 * 履约订单域无独立 QUERY/REPLAY 端点 → 走 investigation 端点。
 */

import { apiPost } from "@/lib/api"
import type {
    FormalActionResponse,
    QueryResultData,
    QueryResultInput,
    ReplayInput,
    ReplayResultData,
} from "@/features/supplier-orders/types"
import { asFulfillment, tsToIso } from "./mapping"
import type { BackendInvestigationResult } from "./wire-types"

/**
 * 查询原结果：后端集成在 integration_ops 错误任务上；
 * 履约订单域无独立 QUERY 端点 → 返回 blocked 并指向 W29。
 */
export async function querySupplierResult(
    input: QueryResultInput,
): Promise<FormalActionResponse<QueryResultData>> {
    return submitInvestigation(input) as Promise<
        FormalActionResponse<QueryResultData>
    >
}

/**
 * 安全重发：后端无独立 REPLAY 端点（在 integration_ops error-task replay）。
 */
export async function replaySupplierOrder(
    input: ReplayInput,
): Promise<FormalActionResponse<ReplayResultData>> {
    return submitInvestigation(input) as Promise<
        FormalActionResponse<ReplayResultData>
    >
}

async function submitInvestigation(
    input: QueryResultInput | ReplayInput,
): Promise<FormalActionResponse<QueryResultData | ReplayResultData>> {
    const result =
        input.commandKind === "TASK"
            ? await apiPost<BackendInvestigationResult>(
                  "/admin/supplier-fulfillment-orders/task-investigations",
                  {
                      work_item_id: input.workItemId,
                      expected_task_version: input.expectedTaskVersion,
                      expected_subject_version: input.expectedSubjectVersion,
                      action: {
                          type: input.action.type,
                          order_id: input.action.orderId,
                          expected_order_lock_version:
                              input.action.expectedOrderLockVersion,
                          target_supplier_action_id:
                              input.action.targetSupplierActionId,
                          operation_id: input.action.operationId,
                      },
                      idempotency_key: input.idempotencyKey,
                  },
              )
            : await apiPost<BackendInvestigationResult>(
                  "/admin/supplier-fulfillment-orders/investigations",
                  {
                      order_id: input.orderId,
                      expected_lock_version: input.expectedLockVersion,
                      action: input.action,
                      operation_id: input.operationId,
                      target_supplier_action_id: input.targetSupplierActionId,
                      idempotency_key: input.idempotencyKey,
                  },
              )
    const evidence = {
        evidenceId: result.evidence.evidence_id,
        targetSupplierActionId: result.evidence.target_supplier_action_id,
        outcome: result.evidence.outcome,
        outcomeLabel:
            result.evidence.outcome === "VERIFIED_TERMINAL"
                ? "处理结果已核实"
                : result.evidence.outcome === "VERIFIED_NO_RESULT"
                  ? "已核实无结果"
                  : "结果仍未知",
        recordedAt: tsToIso(result.evidence.recorded_at),
        canSafeRetry: result.evidence.can_safe_retry,
        externalOrderNo: result.evidence.external_order_no ?? undefined,
        summary: result.evidence.summary,
        verifiedSupplierActionResultId:
            result.evidence.verified_supplier_action_result_id ?? undefined,
        verifiedResolution: result.evidence.verified_resolution ?? undefined,
    }
    const common = {
        status:
            result.result_status === "SUCCEEDED"
                ? ("succeeded" as const)
                : result.result_status === "UNKNOWN"
                  ? ("unknown" as const)
                  : ("blocked" as const),
        message: result.message,
        reference: result.operation_id,
        operationId: result.operation_id,
        data: {
            evidence,
            lockVersion: result.order.version,
            workItemStatus: result.work_item?.status,
            taskVersion:
                result.work_item?.task_version == null
                    ? undefined
                    : String(result.work_item.task_version),
            allowedActions: result.allowed_actions,
            actionBlockers: result.action_blockers.map((blocker) => ({
                action: blocker.action,
                code: blocker.code,
                message: blocker.message,
                destinationWorkspaceId:
                    blocker.destination_workspace_id ?? undefined,
            })),
        },
    }
    if (input.commandKind === "OBJECT" && input.action === "REPLAY") {
        return {
            ...common,
            data: {
                ...common.data,
                externalOrderNo: result.order.external_order_no ?? undefined,
                fulfillmentStatus: asFulfillment(
                    result.order.fulfillment_status,
                ),
            },
        }
    }
    if (input.commandKind === "TASK" && input.action.type === "REPLAY") {
        return {
            ...common,
            data: {
                ...common.data,
                externalOrderNo: result.order.external_order_no ?? undefined,
                fulfillmentStatus: asFulfillment(
                    result.order.fulfillment_status,
                ),
            },
        }
    }
    return common
}
