/**
 * W23 销售单执行投影 · 写路径 HTTP 适配（单条投递命令 / 批量命令）。
 * 调用点必须是 TanStack Query 的 mutationFn（AGENTS.md 第 2 节）。
 */

import { apiPost } from "@/lib/api"
import type {
    BulkItemOutcome,
    BulkProjectionJob,
    ProjectionDeliveryCommandResult,
} from "@/features/execution-projections/types"
import type { BackendDeliveryActionResult } from "@/features/execution-projections/api/mapping"
import { secsToIso } from "@/features/execution-projections/api/mapping"
import { fetchExecutionProjectionDetail } from "@/features/execution-projections/api/reads"

/** 批量操作上限（前端选择条需同步提示，超限禁用批量按钮） */
export const BULK_SELECTION_LIMIT = 20

// ─── In-memory bulk jobs (no backend bulk endpoint) ───────────────────────────

const bulkJobs = new Map<string, BulkProjectionJob>()

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

export async function submitBulkProjectionCommand(
    input: BulkCommandInput,
): Promise<BulkProjectionJob> {
    if (!input.projectionIds.length) {
        return {
            jobId: `bulk_empty_${input.requestId}`,
            action: input.action,
            status: "failed",
            total: 0,
            completed: 0,
            succeeded: 0,
            skipped: 0,
            failed: 0,
            stillUnknown: 0,
            selectionSnapshotId: `snap-${input.requestId}`,
            items: [],
            startedAt: new Date().toISOString(),
            nextAction: "请先逐项显式勾选失败/可处理项",
        }
    }

    if (input.projectionIds.length > BULK_SELECTION_LIMIT) {
        return {
            jobId: `bulk_reject_${input.requestId}`,
            action: input.action,
            status: "failed",
            total: input.projectionIds.length,
            completed: 0,
            succeeded: 0,
            skipped: 0,
            failed: input.projectionIds.length,
            stillUnknown: 0,
            selectionSnapshotId: `snap-${input.requestId}`,
            items: input.projectionIds.map((id) => ({
                projectionId: id,
                salesOrderNo: id,
                deliveryId: "",
                outcome: "failed" as const,
                reason: `批量最多 ${BULK_SELECTION_LIMIT} 条，超出部分请分批`,
            })),
            startedAt: new Date().toISOString(),
            nextAction: `批量最多 ${BULK_SELECTION_LIMIT} 条，超出部分请分批`,
        }
    }

    const jobId = `bulk_${input.action}_${input.requestId}`
    const items: BulkItemOutcome[] = []
    let succeeded = 0
    let failed = 0
    let stillUnknown = 0

    for (const projectionId of input.projectionIds) {
        try {
            const detail = await fetchExecutionProjectionDetail({
                projectionId,
            })
            const result = await submitProjectionDeliveryCommand({
                action:
                    input.action === "BULK_QUERY" ? "QUERY_RESULT" : "RETRY",
                projectionId,
                projectionRevisionId:
                    detail?.selectedRevision.projectionRevisionId ?? "",
                deliveryId:
                    detail?.deliveries[0]?.deliveryId ?? `dlv_${projectionId}`,
                expectedObjectVersion: detail?.objectVersion ?? "1",
                requestId: `${input.requestId}:${projectionId}`,
            })
            if (result.stillUnknown) {
                stillUnknown += 1
                items.push({
                    projectionId,
                    salesOrderNo: result.salesOrderNo,
                    deliveryId: result.deliveryId,
                    outcome: "still_unknown",
                    reason: result.resultLabel,
                })
            } else if (result.result === "FAILED") {
                failed += 1
                items.push({
                    projectionId,
                    salesOrderNo: result.salesOrderNo,
                    deliveryId: result.deliveryId,
                    outcome: "failed",
                    reason: result.resultLabel,
                })
            } else {
                succeeded += 1
                items.push({
                    projectionId,
                    salesOrderNo: result.salesOrderNo,
                    deliveryId: result.deliveryId,
                    outcome: "succeeded",
                    reason: result.resultLabel,
                })
            }
        } catch {
            failed += 1
            items.push({
                projectionId,
                salesOrderNo: projectionId,
                deliveryId: "",
                outcome: "failed",
                reason: "请求失败",
            })
        }
    }

    const job: BulkProjectionJob = {
        jobId,
        action: input.action,
        status:
            failed === 0 && stillUnknown === 0
                ? "succeeded"
                : succeeded > 0
                  ? "partial"
                  : "failed",
        total: input.projectionIds.length,
        completed: items.length,
        succeeded,
        skipped: 0,
        failed,
        stillUnknown,
        selectionSnapshotId: `snap-${input.requestId}`,
        items,
        startedAt: new Date().toISOString(),
        nextAction:
            stillUnknown > 0
                ? "存在结果未知项：不得标成功，请逐项查询"
                : failed > 0
                  ? "部分失败，请查看明细"
                  : "批量完成",
    }
    bulkJobs.set(jobId, job)
    return job
}
