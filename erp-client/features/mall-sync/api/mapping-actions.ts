/**
 * 写路径：映射确认 / 来源修复 / 重新归集。
 */

import { apiGet, apiPost } from "@/lib/api"
import type {
    ConfirmMappingResult,
    ReapplyResult,
    RequestSourceFixResult,
} from "@/features/mall-sync/types"

export async function confirmMapping(input: {
    mappingTaskId: string
    sourceSnapshotId: string
    externalIdentityMapId?: string
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    expectedMappingTaskVersion: number
    mappingOperationId: string
    targetObjectType: string
    targetObjectId: string
    relationRole: string
    evidenceNote: string
    executionStage: "FIRST_PHASE_MALL_OWNED"
    idempotencyKey: string
}): Promise<ConfirmMappingResult> {
    if (!input.evidenceNote.trim() || input.evidenceNote.trim().length < 4) {
        return {
            status: "failed",
            code: "EVIDENCE_REQUIRED",
            message: "请填写确认依据（至少 4 个字）",
        }
    }
    if (!input.targetObjectId) {
        return {
            status: "failed",
            code: "TARGET_REQUIRED",
            message: "请选择 ERP 候选目标（不自动合并）",
        }
    }

    const result = await apiPost<{
        work_item_id: string
        work_item_status: "COMPLETED"
        business_result: {
            mapping_task_id: string
            mapping_task_status: "RESOLVED"
            external_identity_map_id: string
            mapping_target_id: string
            recorded_at: string
        }
    }>(
        `/admin/master-mapping-tasks/${encodeURIComponent(input.mappingTaskId)}/confirm`,
        {
            work_item_id: input.workItemId,
            expected_task_version: input.expectedTaskVersion,
            expected_subject_version: input.expectedSubjectVersion,
            decision: {
                mapping_task_id: input.mappingTaskId,
                source_snapshot_id: input.sourceSnapshotId,
                external_identity_map_id: input.externalIdentityMapId,
                expected_mapping_task_version: input.expectedMappingTaskVersion,
                mapping_operation_id: input.mappingOperationId,
                execution_stage: input.executionStage,
                resolution: {
                    type: "CONFIRM_TARGET",
                    object_type: input.targetObjectType,
                    object_id: input.targetObjectId,
                    relation_role: input.relationRole,
                },
                evidence_note: input.evidenceNote.trim(),
            },
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        status: "succeeded",
        mappingTaskId: result.business_result.mapping_task_id,
        mappingTaskStatus: result.business_result.mapping_task_status,
        externalIdentityMapId: result.business_result.external_identity_map_id,
        mappingTargetId: result.business_result.mapping_target_id,
        recordedAt: result.business_result.recorded_at,
        message: "映射已确认，当前处理任务已完成。",
    }
}

export async function requestSourceFix(input: {
    mappingTaskId: string
    sourceSnapshotId: string
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    expectedMappingTaskVersion: number
    requestOperationId: string
    reasonCode: string
    reasonText: string
    requestedEvidence: string[]
    idempotencyKey: string
}): Promise<RequestSourceFixResult> {
    const result = await apiPost<{
        work_item_id: string
        work_item_status: "OPEN"
        task_version: string | number
        mapping_task_id: string
        mapping_task_status: "PENDING"
        mapping_evidence_entry_id: string
        recorded_at: string
    }>(
        `/admin/master-mapping-tasks/${encodeURIComponent(input.mappingTaskId)}/request-source-fix`,
        {
            work_item_id: input.workItemId,
            expected_task_version: input.expectedTaskVersion,
            expected_subject_version: input.expectedSubjectVersion,
            action: {
                type: "REQUEST_SOURCE_FIX",
                mapping_task_id: input.mappingTaskId,
                source_snapshot_id: input.sourceSnapshotId,
                expected_mapping_task_version: input.expectedMappingTaskVersion,
                request_operation_id: input.requestOperationId,
                reason_code: input.reasonCode,
                reason_text: input.reasonText,
                requested_evidence: input.requestedEvidence,
            },
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        status: "succeeded",
        mappingTaskId: result.mapping_task_id,
        mappingTaskStatus: result.mapping_task_status,
        workItemStatus: result.work_item_status,
        taskVersion: String(result.task_version),
        mappingEvidenceEntryId: result.mapping_evidence_entry_id,
        recordedAt: result.recorded_at,
        message: "来源修复说明已记录，当前处理任务保持待处理。",
    }
}

export async function reapplyMallSnapshot(input: {
    mappingTaskId: string
    sourceSnapshotId: string
    expectedMappingVersion: number
    operationId: string
    executionStage: "FIRST_PHASE_MALL_OWNED"
    idempotencyKey: string
}): Promise<ReapplyResult> {
    const result = await apiPost<{
        action_id: string
        status: "ACCEPTED" | "SUCCEEDED" | "FAILED" | "UNKNOWN"
        background_job_id?: string | null
        reapply_operation_status?:
            | "QUEUED"
            | "RUNNING"
            | "SUCCEEDED"
            | "FAILED"
            | "UNKNOWN"
        sales_order_id?: string | null
        sales_order_revision_id?: string | null
        receivable_result_reference?: string | null
    }>(
        `/admin/master-mapping-tasks/${encodeURIComponent(input.mappingTaskId)}/reapply`,
        {
            mapping_task_id: input.mappingTaskId,
            source_snapshot_id: input.sourceSnapshotId,
            expected_mapping_version: input.expectedMappingVersion,
            operation_id: input.operationId,
            execution_stage: input.executionStage,
            idempotency_key: input.idempotencyKey,
        },
    )
    if (result.status === "UNKNOWN" || result.status === "ACCEPTED") {
        return {
            status: "unknown",
            reapplyOperationStatus: "UNKNOWN",
            operationId: result.action_id,
            message: "重新归集尚无确定结果，请留在当前项查询处理结果。",
            idempotencyKey: input.idempotencyKey,
        }
    }
    if (
        result.status === "SUCCEEDED" &&
        result.sales_order_id &&
        result.sales_order_revision_id
    ) {
        return {
            status: "succeeded",
            operationId: result.action_id,
            reapplyOperationStatus: "SUCCEEDED",
            salesOrderId: result.sales_order_id,
            salesOrderNo: result.sales_order_id,
            salesOrderRevisionId: result.sales_order_revision_id,
            receivableResultReference:
                result.receivable_result_reference ?? undefined,
            message: "原数据已重新归集并形成销售版本。",
        }
    }
    return {
        status: "failed",
        code: "REAPPLY_FAILED",
        operationId: result.action_id,
        reapplyOperationStatus: "FAILED",
        message:
            "重新归集已取得明确失败结果；映射结论保持已解决，请查看操作详情。",
    }
}

export async function resolveUnknownReapply(input: {
    mappingTaskId: string
    operationId: string
    settle?: boolean
}): Promise<ReapplyResult> {
    const operation = await apiGet<{
        operation_id: string
        status: "QUEUED" | "RUNNING" | "SUCCEEDED" | "FAILED" | "UNKNOWN"
        sales_order_id?: string | null
        sales_order_revision_id?: string | null
        receivable_result_reference?: string | null
        failure_code?: string | null
        failure_message?: string | null
    }>(
        `/admin/master-mapping-tasks/${encodeURIComponent(input.mappingTaskId)}/reapply-operations/${encodeURIComponent(input.operationId)}`,
    )
    if (
        operation.status === "SUCCEEDED" &&
        operation.sales_order_id &&
        operation.sales_order_revision_id
    ) {
        return {
            status: "succeeded",
            operationId: operation.operation_id,
            reapplyOperationStatus: "SUCCEEDED",
            salesOrderId: operation.sales_order_id,
            salesOrderNo: operation.sales_order_id,
            salesOrderRevisionId: operation.sales_order_revision_id,
            receivableResultReference:
                operation.receivable_result_reference ?? undefined,
            message: "重新归集已形成可验证销售版本与应收结果。",
        }
    }
    if (operation.status === "FAILED") {
        return {
            status: "failed",
            code: operation.failure_code ?? "REAPPLY_FAILED",
            operationId: operation.operation_id,
            reapplyOperationStatus: "FAILED",
            message:
                operation.failure_message ??
                "重新归集已明确失败，映射结论保持已解决。",
        }
    }
    return {
        status: "unknown",
        reapplyOperationStatus: "UNKNOWN",
        operationId: operation.operation_id,
        message:
            operation.status === "UNKNOWN"
                ? "重新归集结果仍未知，请稍后按同一 operation ID 查询。"
                : "重新归集仍在排队或运行，请稍后查询。",
        idempotencyKey: operation.operation_id,
    }
}
