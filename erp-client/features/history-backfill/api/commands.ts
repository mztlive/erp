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
                    "请填写切换编号、范围起点和范围终点后再创建回填任务。",
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
            title: "暂不支持单独校验来源",
            description: "当前可直接开始回填，系统会在开始时核对来源资料。",
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
            title: "暂不支持重新归集",
            description: "当前无法重新归集回填明细，请联系管理员处理。",
            jobId: input.jobId,
            operationId,
            idempotencyKey,
            blockers: ["REATTRIBUTE_NOT_IMPLEMENTED"],
        }
    }

    if (action === "CONFIRM_REPORT") {
        return {
            status: "BLOCKED",
            title: "暂不支持在本页确认报告",
            description: "请先下载报告核对；需要确认时请联系管理员处理。",
            jobId: input.jobId,
            operationId,
            idempotencyKey,
            blockers: ["CONFIRM_REPORT_NOT_IMPLEMENTED"],
        }
    }

    return {
        status: "FAILED",
        title: "操作未完成",
        description: "当前操作暂不受支持，请刷新页面后重试。",
        operationId,
        idempotencyKey,
    }
}
