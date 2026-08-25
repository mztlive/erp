/**
 * 写路径：人工增量 / 按单补拉 / 失败任务重试。
 */

import { apiPost } from "@/lib/api"
import type { BackendJob } from "@/features/mall-sync/api/backend-dtos"
import type {
    OwnershipStage,
    TriggerMallSyncResult,
} from "@/features/mall-sync/types"

export async function triggerManualIncremental(input: {
    sourceSystemId: string
    reason: string
    stage: OwnershipStage
    idempotencyKey?: string
}): Promise<TriggerMallSyncResult> {
    if (!input.reason.trim() || input.reason.trim().length < 4) {
        return {
            status: "failed",
            code: "REASON_REQUIRED",
            message: "单人理由模式下请填写至少 4 个字的触发理由",
        }
    }
    if (!input.sourceSystemId.trim()) {
        return {
            status: "failed",
            code: "SOURCE_MISSING",
            message: "未配置可用商城来源系统",
        }
    }
    if (input.stage !== "FIRST_PHASE_MALL_OWNED") {
        return {
            status: "failed",
            code: "MALL_SYNC_ARCHIVED",
            message: "W17 已封存为只读历史，不能触发人工增量",
        }
    }
    const job = await apiPost<BackendJob>("/admin/mall-sales-sync-jobs", {
        mode: "INCREMENTAL",
        source_system_id: input.sourceSystemId,
        execution_stage: input.stage,
        trigger_source: "MANUAL",
        reason: input.reason.trim(),
        idempotency_key: input.idempotencyKey ?? crypto.randomUUID(),
    })
    return {
        status: "succeeded",
        jobId: job.id,
        jobNo: job.id.slice(0, 12).toUpperCase(),
        message: `已创建增量任务。理由：${input.reason.trim().slice(0, 40)}`,
    }
}

export async function triggerSingleOrderPull(input: {
    sourceSystemId: string
    externalOrderNo: string
    reason: string
    stage: OwnershipStage
    idempotencyKey?: string
}): Promise<TriggerMallSyncResult> {
    if (!input.externalOrderNo.trim()) {
        return {
            status: "failed",
            code: "ORDER_NO_REQUIRED",
            message: "请填写有效来源单号",
        }
    }
    if (!input.sourceSystemId.trim()) {
        return {
            status: "failed",
            code: "SOURCE_MISSING",
            message: "未配置可用商城来源系统",
        }
    }
    if (input.stage !== "FIRST_PHASE_MALL_OWNED") {
        return {
            status: "failed",
            code: "MALL_SYNC_ARCHIVED",
            message: "W17 已封存为只读历史，不能按单号补拉",
        }
    }
    const job = await apiPost<BackendJob>("/admin/mall-sales-sync-jobs", {
        mode: "SINGLE_ORDER",
        source_system_id: input.sourceSystemId,
        execution_stage: input.stage,
        trigger_source: "MANUAL",
        external_order_no: input.externalOrderNo.trim(),
        reason: input.reason.trim(),
        idempotency_key: input.idempotencyKey ?? crypto.randomUUID(),
    })
    return {
        status: "succeeded",
        jobId: job.id,
        jobNo: job.id.slice(0, 12).toUpperCase(),
        message: `已沿原来源单号 ${input.externalOrderNo.trim()} 创建按单补拉作业。`,
    }
}

export async function retryFailedJob(input: {
    jobId: string
    reason: string
    stage?: OwnershipStage
    idempotencyKey?: string
}): Promise<TriggerMallSyncResult> {
    const job = await apiPost<BackendJob>(
        `/admin/mall-sales-sync-jobs/${input.jobId}/retry`,
        {
            reason: input.reason.trim(),
            idempotency_key: input.idempotencyKey ?? crypto.randomUUID(),
        },
    )
    return {
        status: "succeeded",
        jobId: job.id,
        jobNo: job.id.slice(0, 12).toUpperCase(),
        message: "已按原作业类型和范围创建重试作业；历史处理进度保持不变。",
    }
}
