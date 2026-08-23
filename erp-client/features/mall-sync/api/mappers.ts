/**
 * 后端 DTO → 客户端视图映射。纯函数，无 IO。
 */

import type {
    BackendJob,
    BackendMappingTask,
    BackendSnapshot,
    BackendSourceSystem,
} from "@/features/mall-sync/api/backend-dtos"
import type {
    MallSnapshotRow,
    MallSyncJobRow,
    MappingTaskView,
    SourceSystemStatus,
} from "@/features/mall-sync/types"
import {
    JOB_TYPE_LABEL,
    MAPPING_TYPE_LABEL,
    OWNER_ROLE_LABEL,
} from "@/features/mall-sync/types"

export function instantToIso(
    secs: number | null | undefined,
): string | undefined {
    if (secs == null || !Number.isFinite(secs)) return undefined
    return new Date(secs * 1000).toISOString()
}

export function shortHash(hash?: string | null): string {
    if (!hash) return "—"
    return hash.length > 12 ? `${hash.slice(0, 8)}…` : hash
}

export function mapJobType(
    t: BackendJob["job_type"],
): MallSyncJobRow["jobType"] {
    switch (t) {
        case "baseline":
            return "BASELINE"
        case "incremental":
            return "INCREMENTAL"
        case "single_order_backfill":
            return "SINGLE_ORDER"
        case "monthly_reconciliation":
            return "RECONCILIATION"
        default:
            return "INCREMENTAL"
    }
}

export function mapJobStatus(
    s: BackendJob["status"],
): Pick<MallSyncJobRow, "status" | "statusLabel" | "statusTone"> {
    switch (s) {
        case "running":
            return {
                status: "RUNNING",
                statusLabel: "运行中",
                statusTone: "info",
            }
        case "success":
            return {
                status: "SUCCEEDED",
                statusLabel: "成功",
                statusTone: "success",
            }
        case "partial_failure":
            return {
                status: "PARTIAL_FAILED",
                statusLabel: "部分失败",
                statusTone: "warning",
            }
        case "failed":
            return {
                status: "FAILED",
                statusLabel: "失败",
                statusTone: "destructive",
            }
        default:
            return { status: "FAILED", statusLabel: s, statusTone: "neutral" }
    }
}

export function mapSnapshotStatus(
    s: BackendSnapshot["mapping_status"],
): Pick<MallSnapshotRow, "mappingStatus" | "mappingStatusLabel"> {
    switch (s) {
        case "pending":
            return {
                mappingStatus: "PENDING_MAPPING",
                mappingStatusLabel: "待映射",
            }
        case "applied":
            return { mappingStatus: "APPLIED", mappingStatusLabel: "已应用" }
        case "difference":
            return { mappingStatus: "DIFF", mappingStatusLabel: "差异" }
        case "no_change":
            return { mappingStatus: "UNCHANGED", mappingStatusLabel: "无变化" }
        default:
            return { mappingStatus: "PENDING_MAPPING", mappingStatusLabel: s }
    }
}

export function mapMappingType(
    t: BackendMappingTask["mapping_type"],
): keyof typeof MAPPING_TYPE_LABEL {
    switch (t) {
        case "customer":
            return "CUSTOMER"
        case "contract":
            return "CONTRACT"
        case "settlement_entity":
            return "SETTLEMENT_PARTY"
        case "voucher_category":
            return "VOUCHER_CATEGORY"
        case "unique_line_item":
            return "UNIQUE_LINE"
        case "amount_format":
            return "AMOUNT_FORMAT"
        default:
            return "CUSTOMER"
    }
}

export function mapMappingStatus(
    s: BackendMappingTask["status"],
): Pick<MappingTaskView, "mappingTaskStatus" | "mappingTaskStatusLabel"> {
    switch (s) {
        case "pending":
            return {
                mappingTaskStatus: "PENDING",
                mappingTaskStatusLabel: "待处理",
            }
        case "resolved":
            return {
                mappingTaskStatus: "RESOLVED",
                mappingTaskStatusLabel: "已解决",
            }
        case "unresolvable":
            return {
                mappingTaskStatus: "UNRESOLVABLE",
                mappingTaskStatusLabel: "无法处理",
            }
        case "closed":
            return {
                mappingTaskStatus: "CLOSED",
                mappingTaskStatusLabel: "关闭",
            }
        default:
            return { mappingTaskStatus: "PENDING", mappingTaskStatusLabel: s }
    }
}

export function mapOwnerRole(
    role: string,
): "SALES" | "OPERATIONS" | "FINANCE" | undefined {
    const r = role.trim().toUpperCase()
    if (r === "SALES" || r === "ROLE-SALES" || r === "销售") return "SALES"
    if (
        r === "OPERATIONS" ||
        r === "ROLE-OPERATIONS" ||
        r === "运营" ||
        r === "OPS"
    )
        return "OPERATIONS"
    if (r === "FINANCE" || r === "ROLE-FINANCE" || r === "财务")
        return "FINANCE"
    return undefined
}

export function toJobRow(job: BackendJob): MallSyncJobRow {
    const jobType = mapJobType(job.job_type)
    const st = mapJobStatus(job.status)
    const failed = st.status === "FAILED" || st.status === "PARTIAL_FAILED"
    return {
        jobId: job.id,
        jobNo: job.id.slice(0, 12).toUpperCase(),
        jobType,
        jobTypeLabel: JOB_TYPE_LABEL[jobType],
        rangeStart: instantToIso(job.range_start ?? undefined),
        rangeEnd: instantToIso(job.range_end ?? undefined),
        ...st,
        pageCount: job.page_count,
        itemCount: job.item_count,
        errorCount: job.error_count,
        startedAt: instantToIso(job.started_at) ?? "",
        finishedAt: instantToIso(job.finished_at ?? undefined),
        triggeredBy:
            job.trigger_source === "SCHEDULED"
                ? "系统调度"
                : (job.triggered_by ?? "授权用户"),
        watermarkAdvanced: st.status === "SUCCEEDED",
        allowedActions: failed ? ["RETRY_FAILED_JOB"] : [],
        actionBlockers: [],
    }
}

export function toSnapshotRow(
    snap: BackendSnapshot,
    jobNoById: Map<string, string>,
): MallSnapshotRow {
    const ms = mapSnapshotStatus(snap.mapping_status)
    return {
        snapshotId: snap.id,
        externalOrderNo: snap.external_order_no,
        sourceUpdatedAt: instantToIso(snap.source_updated_at) ?? "",
        observedAt: instantToIso(snap.observed_at) ?? "",
        sourceStatusCode: snap.source_status_code,
        sourceStatusLabel: snap.source_status_code,
        contentHashShort: shortHash(snap.content_hash),
        ...ms,
        appliedSalesOrderId: snap.applied_sales_order_revision_id ?? undefined,
        syncJobId: snap.sync_job_id,
        syncJobNo:
            jobNoById.get(snap.sync_job_id) ?? snap.sync_job_id.slice(0, 12),
        conflictFlags: ms.mappingStatus === "DIFF" ? ["MAPPING_DIFF"] : [],
        whitelistFields: [
            {
                field: "external_order_no",
                label: "来源单号",
                value: snap.external_order_no,
            },
            {
                field: "source_status_code",
                label: "商城状态码",
                value: snap.source_status_code,
            },
        ],
    }
}

export function toMappingTask(
    task: BackendMappingTask,
    snapById: Map<string, BackendSnapshot>,
): MappingTaskView {
    const mappingType = mapMappingType(task.mapping_type)
    const st = mapMappingStatus(task.status)
    const ownerRole = task.owner_role
        ? mapOwnerRole(task.owner_role)
        : undefined
    const snap = snapById.get(task.source_snapshot_id)
    const externalOrderNo =
        task.source_evidence.find(
            (evidence) => evidence.field === "external_order_no",
        )?.value ??
        snap?.external_order_no ??
        "—"
    const reapply = task.reapply_operation
        ? {
              operationId: task.reapply_operation.operation_id,
              status: task.reapply_operation.status,
              statusLabel:
                  {
                      QUEUED: "排队中",
                      RUNNING: "运行中",
                      SUCCEEDED: "成功",
                      FAILED: "失败",
                      UNKNOWN: "结果未知",
                  }[task.reapply_operation.status] ??
                  task.reapply_operation.status,
              lastUpdatedAt:
                  instantToIso(task.reapply_operation.last_updated_at) ?? "",
              salesOrderId: task.reapply_operation.sales_order_id ?? undefined,
              salesOrderRevisionId:
                  task.reapply_operation.sales_order_revision_id ?? undefined,
              receivableResultReference:
                  task.reapply_operation.receivable_result_reference ??
                  undefined,
          }
        : undefined

    const typedBase = {
        mappingTaskId: task.id,
        sourceSnapshotId: task.source_snapshot_id,
        externalOrderNo,
        externalIdentityMapId: task.external_identity_map_id ?? undefined,
        mappingType,
        mappingTypeLabel: MAPPING_TYPE_LABEL[mappingType],
        ...st,
        reapplyOperation: reapply,
        sourceEvidence: task.source_evidence.map((evidence) => ({
            field: evidence.field,
            label: evidence.label,
            value: evidence.value,
            sensitive: evidence.sensitive,
        })),
        candidateTargets: task.candidate_targets.map((candidate) => ({
            objectType: candidate.object_type,
            objectId: candidate.object_id,
            stableNo: candidate.stable_no,
            label: candidate.label,
            currentRevisionId: candidate.current_revision_id,
            eligibility: candidate.eligibility,
            reason: candidate.reason,
        })),
        currentTargets: task.current_targets.map((target) => ({
            objectType: target.object_type,
            objectId: target.object_id,
            stableNo: target.object_id,
            label: target.object_id,
            relationRole: target.relation_role,
            validFrom: instantToIso(target.valid_from) ?? "",
            validTo: instantToIso(target.valid_to ?? undefined),
            status: target.status,
        })),
        impactSummary: task.impact_summary,
        resolutionHistory: task.resolution_history.map((history) => ({
            action: history.action,
            result: history.result,
            handledBy: history.handled_by,
            handledAt: instantToIso(history.handled_at) ?? "",
            evidenceReference: history.evidence_reference ?? undefined,
        })),
        allowedActions: task.allowed_actions,
        actionBlockers: task.action_blockers,
        lockVersion: task.lock_version,
        hasConflict: task.action_blockers.some((blocker) =>
            blocker.code.includes("CONFLICT"),
        ),
    }

    if (
        task.owner_routing_state !== "CONFIGURED" ||
        !ownerRole ||
        !task.work_item ||
        task.work_item.work_item_type !== "BUSINESS_EXCEPTION" ||
        task.work_item.business_object_type.toUpperCase() !==
            "MASTER_MAPPING_TASK" ||
        task.work_item.business_object_id !== task.id
    ) {
        return {
            ...typedBase,
            ownerRoutingState: "MISSING" as const,
            allowedActions: [],
            candidateTargets: [],
        }
    }

    const workItem = task.work_item
    return {
        ...typedBase,
        ownerRoutingState: "CONFIGURED" as const,
        ownerRole,
        ownerRoleLabel: OWNER_ROLE_LABEL[ownerRole],
        ownerUserId: workItem.owner_user_id ?? undefined,
        workItem: {
            workItemId: workItem.work_item_id,
            workItemType: "BUSINESS_EXCEPTION" as const,
            businessObjectType: "MASTER_MAPPING_TASK" as const,
            businessObjectId: task.id,
            subjectVersion: workItem.subject_version,
            taskVersion: workItem.task_version,
            status: workItem.status,
            statusLabel:
                workItem.status === "COMPLETED"
                    ? "已完成"
                    : workItem.status === "CLOSED"
                      ? "已关闭"
                      : workItem.owner_user_id
                        ? "处理中"
                        : "责任人信息不可用",
            processingState: "READY" as const,
            ownerUser: workItem.owner_user_id
                ? {
                      id: workItem.owner_user_id,
                      displayName: workItem.owner_user_id,
                  }
                : undefined,
        },
    }
}

export function mapSourceStatus(
    s: BackendSourceSystem["status"],
): SourceSystemStatus {
    return s === "active" ? "启用" : "停用"
}
