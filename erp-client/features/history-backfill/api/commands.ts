/**
 * W30 历史消费回填 · 命令提交
 */

import { apiPost } from "@/lib/api"
import type {
    HistoryBackfillCommandInput,
    HistoryBackfillCommandResult,
} from "@/features/history-backfill/types"
import { dateToUnix } from "@/features/history-backfill/api/mapping"
import type {
    BackendCommandResult,
    BackendJob,
} from "@/features/history-backfill/api/wire"

export async function submitHistoryBackfillCommand(
    input: HistoryBackfillCommandInput,
): Promise<HistoryBackfillCommandResult> {
    const { action, operationId, idempotencyKey } = input

    if (action === "CREATE_DRAFT") {
        if (!input.cutoverId || !input.rangeStart || !input.rangeEnd) {
            return {
                status: "BLOCKED",
                title: "缺少创建参数",
                description:
                    "创建回填任务需要 cutoverId、rangeStart、rangeEnd；创建上下文接口未交付时无法自动填参。",
                operationId,
                idempotencyKey,
                blockers: ["CREATE_CONTEXT_MISSING"],
            }
        }
        const rangeStart = dateToUnix(input.rangeStart)
        const rangeEnd = dateToUnix(input.rangeEnd)
        if (!rangeStart || !rangeEnd) {
            return {
                status: "BLOCKED",
                title: "范围非法",
                description: "范围起点/终点无法解析为时间戳。",
                operationId,
                idempotencyKey,
                blockers: ["RANGE_INVALID"],
            }
        }

        const created = await apiPost<BackendJob>(
            "/admin/mall-consumption-backfill-jobs",
            {
                mall_id: "default",
                cutover_id: input.cutoverId,
                range_start: rangeStart,
                range_end: rangeEnd,
                total_count: 1,
                total_amount: "0.00",
            },
        )

        return {
            status: "COMMITTED",
            title: "已创建回填任务草稿",
            description: `任务 ${created.id} 已创建。`,
            jobId: created.id,
            jobNo: created.id,
            operationId,
            idempotencyKey,
            nextStep: "完成来源校验后开始回填",
        }
    }

    if (action === "VALIDATE_SOURCE") {
        // Backend has no validate endpoint — treat as client-side pass marker via detail refresh
        if (!input.jobId) {
            return {
                status: "FAILED",
                title: "缺少任务 ID",
                description: "校验来源必须引用既有任务。",
                operationId,
                idempotencyKey,
            }
        }
        return {
            status: "BLOCKED",
            title: "来源校验接口未交付",
            description: "后端未提供独立 VALIDATE_SOURCE 命令；可直接 START。",
            jobId: input.jobId,
            operationId,
            idempotencyKey,
            blockers: ["VALIDATE_SOURCE_NOT_IMPLEMENTED"],
        }
    }

    if (action === "START" || action === "RESUME") {
        if (!input.jobId) {
            return {
                status: "FAILED",
                title: "缺少任务 ID",
                description: `${action} 必须引用既有任务。`,
                operationId,
                idempotencyKey,
            }
        }
        const version = input.expectedLockVersion ?? 1
        const result = await apiPost<BackendCommandResult>(
            `/admin/mall-consumption-backfill-jobs/${encodeURIComponent(input.jobId)}/commands`,
            {
                command: action === "START" ? "START" : "RESUME",
                version,
                operation_id: operationId,
                idempotency_key: idempotencyKey,
            },
        )
        return {
            status: "COMMITTED",
            title: action === "START" ? "回填已提交后台" : "已续跑原任务",
            description: result.next_step || "进度以任务记录为准。",
            jobId: result.job_id,
            jobNo: result.job_no,
            operationId: result.operation_id,
            idempotencyKey: result.idempotency_key,
            nextStep: result.next_step,
        }
    }

    if (action === "REATTRIBUTE") {
        return {
            status: "BLOCKED",
            title: "重新归集未交付",
            description: "后端尚未提供回填明细重新归集命令。",
            jobId: input.jobId,
            operationId,
            idempotencyKey,
            blockers: ["REATTRIBUTE_NOT_IMPLEMENTED"],
        }
    }

    if (action === "CONFIRM_REPORT") {
        return {
            status: "BLOCKED",
            title: "报告确认未交付",
            description:
                "后端 BackfillJobView 仅有 report_file_id，无报告确认状态机接口。",
            jobId: input.jobId,
            operationId,
            idempotencyKey,
            blockers: ["CONFIRM_REPORT_NOT_IMPLEMENTED"],
        }
    }

    return {
        status: "FAILED",
        title: "未知动作",
        description: `不支持的 action: ${action}`,
        operationId,
        idempotencyKey,
    }
}
