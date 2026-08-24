/**
 * 写路径：人工增量 / 按单补拉 / 失败任务重试。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { BackendJob } from "@/features/mall-sync/api/backend-dtos"
import { resolveMallSourceSystemId } from "@/features/mall-sync/api/fetch-page"
import type { OwnershipStage, TriggerMallSyncResult } from "@/features/mall-sync/types"

export async function triggerManualIncremental(input: {
    reason: string
    stage?: OwnershipStage
    idempotencyKey?: string
}): Promise<TriggerMallSyncResult> {
    if (!input.reason.trim() || input.reason.trim().length < 4) {
        return {
            status: "failed",
            code: "REASON_REQUIRED",
            message: "单人理由模式下请填写至少 4 个字的触发理由",
        }
    }
    const source = await resolveMallSourceSystemId()
    if (!source) {
        return {
            status: "failed",
            code: "SOURCE_MISSING",
            message: "未配置可用商城来源系统",
        }
    }
    if (source.stage !== "FIRST_PHASE_MALL_OWNED") {
        return {
            status: "failed",
            code: "MALL_SYNC_ARCHIVED",
            message: "W17 已封存为只读历史，不能触发人工增量",
        }
    }
    const job = await apiPost<BackendJob>("/admin/mall-sales-sync-jobs", {
        mode: "INCREMENTAL",
        source_system_id: source.id,
        execution_stage: source.stage,
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
    externalOrderNo: string
    reason: string
    stage?: OwnershipStage
    idempotencyKey?: string
}): Promise<TriggerMallSyncResult> {
    if (!input.externalOrderNo.trim()) {
        return {
            status: "failed",
            code: "ORDER_NO_REQUIRED",
            message: "请填写有效来源单号",
        }
    }
    const source = await resolveMallSourceSystemId()
    if (!source) {
        return {
            status: "failed",
            code: "SOURCE_MISSING",
            message: "未配置可用商城来源系统",
        }
    }
    if (source.stage !== "FIRST_PHASE_MALL_OWNED") {
        return {
            status: "failed",
            code: "MALL_SYNC_ARCHIVED",
            message: "W17 已封存为只读历史，不能按单号补拉",
        }
    }
    const job = await apiPost<BackendJob>("/admin/mall-sales-sync-jobs", {
        mode: "SINGLE_ORDER",
        source_system_id: source.id,
        execution_stage: source.stage,
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
    const original = await apiGet<BackendJob>(
        `/admin/mall-sales-sync-jobs/${input.jobId}`,
    )
    if (original.status !== "failed" && original.status !== "partial_failure") {
        return {
            status: "failed",
            code: "NOT_RETRYABLE",
            message: "仅失败/部分失败任务可重试",
        }
    }
    const source = await resolveMallSourceSystemId()
    if (
        !source ||
        source.id !== original.source_system_id ||
        source.stage !== "FIRST_PHASE_MALL_OWNED"
    ) {
        return {
            status: "failed",
            code: "MALL_SYNC_ARCHIVED",
            message: "来源商城不在一期可写阶段，不能重试普通同步作业",
        }
    }
    const job = await apiPost<BackendJob>("/admin/mall-sales-sync-jobs", {
        mode: "RETRY_FAILED_JOB",
        source_system_id: original.source_system_id,
        execution_stage: source.stage,
        failed_job_id: original.id,
        reason: input.reason.trim(),
        idempotency_key: input.idempotencyKey ?? crypto.randomUUID(),
    })
    return {
        status: "succeeded",
        jobId: job.id,
        jobNo: job.id.slice(0, 12).toUpperCase(),
        message:
            "已按原作业类型和范围创建重试作业；历史处理进度保持不变。",
    }
}
