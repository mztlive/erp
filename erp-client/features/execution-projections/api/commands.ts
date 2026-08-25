/**
 * W23 销售单执行投影 · 写路径 HTTP 适配（单条投递命令 / 批量命令）。
 * 调用点必须是 TanStack Query 的 mutationFn（AGENTS.md 第 2 节）。
 */

import { apiPost } from "@/lib/api"
import type {
    BulkProjectionJob,
    ProjectionDeliveryCommandResult,
} from "@/features/execution-projections/types"
import type { BackendDeliveryActionResult } from "@/features/execution-projections/api/mapping"
import { secsToIso } from "@/features/execution-projections/api/mapping"

/** 批量操作上限（前端选择条需同步提示，超限禁用批量按钮） */
export const BULK_SELECTION_LIMIT = 20

export type DeliveryCommandInput = {
    projectionId: string
    projectionRevisionId: string
    deliveryId: string
    action: "QUERY_RESULT" | "RETRY" | "ESCALATE"
    expectedObjectVersion: string
    requestId: string
}

export async function submitProjectionDeliveryCommand(
    input: DeliveryCommandInput,
): Promise<ProjectionDeliveryCommandResult> {
    const result = await apiPost<BackendDeliveryActionResult>(
        `/admin/sales-order-projection-deliveries/${encodeURIComponent(input.deliveryId)}/actions`,
        {
            projection_id: input.projectionId,
            projection_revision_id: input.projectionRevisionId,
            delivery_id: input.deliveryId,
            action: input.action,
            expected_object_version: Number(input.expectedObjectVersion),
            request_id: input.requestId,
        },
    )
    const resultLabel: Record<BackendDeliveryActionResult["result"], string> = {
        ACKED: "已确认",
        FAILED: "失败",
        STILL_UNKNOWN: "结果未知",
        RETRY_SCHEDULED: "已安排重试",
        ESCALATED: "已转人工",
    }
    return {
        operationId: result.operation_id,
        deliveryId: result.delivery_id,
        projectionId: input.projectionId,
        salesOrderNo: input.projectionId,
        result: result.result,
        resultLabel: resultLabel[result.result],
        workItemId: result.work_item_id ?? undefined,
        errorTaskId: result.error_task_id ?? undefined,
        occurredAt: secsToIso(result.occurred_at),
        nextAction: result.next_action ?? "无需进一步操作",
        stillUnknown: result.result === "STILL_UNKNOWN",
        objectVersion: input.expectedObjectVersion,
    }
}

export type BulkCommandInput = {
    action: "BULK_QUERY" | "BULK_RETRY"
    /** 仅显式选择的稳定 ID；拒绝“当前筛选全部” */
    projectionIds: string[]
    requestId: string
}

type BackendBulkProjectionCommandResult = {
    job_id: string
    action: "BULK_QUERY" | "BULK_RETRY"
    status: "SUCCEEDED" | "PARTIAL" | "FAILED"
    total: number
    completed: number
    succeeded: number
    skipped: number
    failed: number
    still_unknown: number
    selection_snapshot_id: string
    items: Array<{
        projection_id: string
        sales_order_no: string
        delivery_id: string
        outcome: "SUCCEEDED" | "SKIPPED" | "FAILED" | "STILL_UNKNOWN"
        reason: string
    }>
    started_at: number
    finished_at: number
    next_action: string
}

export async function submitBulkProjectionCommand(
    input: BulkCommandInput,
): Promise<BulkProjectionJob> {
    const result = await apiPost<BackendBulkProjectionCommandResult>(
        "/admin/sales-order-projection-deliveries/bulk-actions",
        {
            action: input.action,
            projection_ids: input.projectionIds,
            request_id: input.requestId,
        },
    )
    const outcomes = {
        SUCCEEDED: "succeeded",
        SKIPPED: "skipped",
        FAILED: "failed",
        STILL_UNKNOWN: "still_unknown",
    } as const
    const statuses = {
        SUCCEEDED: "succeeded",
        PARTIAL: "partial",
        FAILED: "failed",
    } as const
    return {
        jobId: result.job_id,
        action: result.action,
        status: statuses[result.status],
        total: result.total,
        completed: result.completed,
        succeeded: result.succeeded,
        skipped: result.skipped,
        failed: result.failed,
        stillUnknown: result.still_unknown,
        selectionSnapshotId: result.selection_snapshot_id,
        items: result.items.map((item) => ({
            projectionId: item.projection_id,
            salesOrderNo: item.sales_order_no,
            deliveryId: item.delivery_id,
            outcome: outcomes[item.outcome],
            reason: item.reason,
        })),
        startedAt: secsToIso(result.started_at),
        finishedAt: secsToIso(result.finished_at),
        nextAction: result.next_action,
    }
}
