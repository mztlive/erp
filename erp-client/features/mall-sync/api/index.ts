/**
 * W17 商城同步 · 真实 HTTP API（P4 F8）。
 * 导出签名保持与 queries.ts / 页面一致；字段适配仅在本文件完成。
 * 后端域：mall_sync + source_registry（/admin/source-systems 样板已有）。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type {
    ConfirmMappingResult,
    MallSnapshotRow,
    MallSyncJobRow,
    MallSyncMetric,
    MallSyncPageView,
    MallSyncViewName,
    MappingTaskView,
    OwnershipStage,
    ReapplyResult,
    ReconciliationBatch,
    ReconciliationDifference,
    RequestSourceFixResult,
    SourceSystemItem,
    SourceSystemListParams,
    SourceSystemPage,
    SourceSystemStatus,
    SourceSystemType,
    TriggerMallSyncResult,
} from "@/features/mall-sync/types"
import {
    JOB_TYPE_LABEL,
    MAPPING_TYPE_LABEL,
    OWNER_ROLE_LABEL,
} from "@/features/mall-sync/types"

// ─── 后端 DTO（snake_case；时间秒级时间戳） ─────────────────────────────────

type BackendJob = {
    id: string
    source_system_id: string
    job_type:
        | "baseline"
        | "incremental"
        | "monthly_reconciliation"
        | "single_order_backfill"
    range_start?: number | null
    range_end?: number | null
    external_order_no?: string | null
    trigger_source: "SCHEDULED" | "MANUAL"
    trigger_reason?: string | null
    triggered_by?: string | null
    source_job_id?: string | null
    started_at: number
    finished_at?: number | null
    status: "running" | "success" | "partial_failure" | "failed"
    page_count: number
    item_count: number
    error_count: number
    version: number
    created_at: number
}

type BackendSnapshot = {
    id: string
    source_system_id: string
    external_order_no: string
    source_updated_at: number
    content_hash?: string | null
    source_status_code: string
    observed_at: number
    mapping_status: "pending" | "applied" | "difference" | "no_change"
    applied_sales_order_revision_id?: string | null
    sync_job_id: string
    version: number
    created_at: number
}

type BackendCursor = {
    id: string
    source_system_id: string
    high_water_updated_at: number
    last_success_job_id?: string | null
    version: number
    created_at: number
}

type BackendMappingTask = {
    id: string
    source_snapshot_id: string
    mapping_type:
        | "customer"
        | "contract"
        | "settlement_entity"
        | "voucher_category"
        | "unique_line_item"
        | "amount_format"
    status: "pending" | "resolved" | "unresolvable" | "closed"
    owner_role?: string | null
    owner_user_id?: string | null
    resolution?: string | null
    resolved_at?: number | null
    owner_routing_state: "MISSING" | "CONFIGURED"
    work_item?: {
        work_item_id: string
        task_version: string
        work_item_type: "BUSINESS_EXCEPTION"
        business_object_type: "MASTER_MAPPING_TASK"
        business_object_id: string
        subject_version: string
        status: "OPEN" | "COMPLETED" | "CLOSED"
        assignment_mode: "DIRECT" | "POOL"
        owner_user_id?: string | null
        allowed_actions: Array<
            "START_PROCESSING" | "RELEASE_TO_TEAM" | "REASSIGN"
        >
    } | null
    external_identity_map_id?: string | null
    source_evidence: Array<{
        field: string
        label: string
        value: string
        sensitive: boolean
    }>
    candidate_targets: Array<{
        object_type: string
        object_id: string
        stable_no: string
        label: string
        current_revision_id: string
        eligibility: "ELIGIBLE" | "INELIGIBLE"
        reason: string
    }>
    current_targets: Array<{
        mapping_target_id: string
        object_type: string
        object_id: string
        relation_role: string
        valid_from: number
        valid_to?: number | null
        status: string
    }>
    impact_summary: string
    resolution_history: Array<{
        action: string
        result: string
        handled_by: string
        handled_at: number
        evidence_reference?: string | null
    }>
    allowed_actions: string[]
    action_blockers: Array<{
        action: string
        code: string
        message: string
    }>
    reapply_operation?: {
        operation_id: string
        status: "QUEUED" | "RUNNING" | "SUCCEEDED" | "FAILED" | "UNKNOWN"
        sales_order_id?: string | null
        sales_order_revision_id?: string | null
        receivable_result_reference?: string | null
        failure_code?: string | null
        failure_message?: string | null
        last_updated_at: number
    } | null
    lock_version: number
    version: number
    created_at: number
}

type BackendReconJob = {
    id: string
    source_system_id: string
    job_no: string
    source_list_as_of: number
    source_count: number
    erp_count: number
    difference_count: number
    status: "running" | "completed" | "has_difference" | "failed"
    started_at: number
    finished_at?: number | null
    version: number
    created_at: number
}

type BackendReconItem = {
    id: string
    reconciliation_job_id: string
    external_order_no: string
    source_status_code: string
    source_updated_at: number
    difference_type:
        | "mall_missing"
        | "erp_missing"
        | "status_difference"
        | "content_fingerprint_difference"
        | "duplicate_identity"
    status: "pending" | "backfilling" | "resolved" | "confirmed_no_difference"
    single_order_sync_job_id?: string | null
    resolution?: string | null
    resolved_by?: string | null
    resolved_at?: number | null
    version: number
    created_at: number
}

type BackendSourceSystem = {
    id: string
    code: string
    name: string
    system_type: "ERP" | "MALL" | "SUPPLIER"
    status: "active" | "disabled"
    mall_sync_stage?: "FIRST_PHASE_MALL_OWNED" | "ARCHIVED" | null
    created_at: number
    version?: number
}

// ─── Query input（契约保持） ─────────────────────────────────────────────────

export type MallSyncQueryInput = {
    view: MallSyncViewName
    sourceUnavailable?: boolean
    q?: string
    jobId?: string
    snapshotId?: string
    mappingTaskId?: string
    workItemId?: string
    differenceId?: string
    queueContextId?: string
    owner?: "mine" | "all"
    mappingType?: string
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function instantToIso(secs: number | null | undefined): string | undefined {
    if (secs == null || !Number.isFinite(secs)) return undefined
    return new Date(secs * 1000).toISOString()
}

function shortHash(hash?: string | null): string {
    if (!hash) return "—"
    return hash.length > 12 ? `${hash.slice(0, 8)}…` : hash
}

function mapJobType(t: BackendJob["job_type"]): MallSyncJobRow["jobType"] {
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

function mapJobStatus(
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

function mapSnapshotStatus(
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

function mapMappingType(
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

function mapMappingStatus(
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

function mapOwnerRole(
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

function mapDiffType(
    t: BackendReconItem["difference_type"],
): ReconciliationDifference["differenceType"] {
    switch (t) {
        case "mall_missing":
            return "MALL_MISSING"
        case "erp_missing":
            return "ERP_MISSING"
        case "status_difference":
            return "STATUS"
        case "content_fingerprint_difference":
            return "FINGERPRINT"
        case "duplicate_identity":
            return "DUPLICATE"
        default:
            return "STATUS"
    }
}

const DIFF_TYPE_LABEL: Record<
    ReconciliationDifference["differenceType"],
    string
> = {
    MALL_MISSING: "商城缺失",
    ERP_MISSING: "ERP 缺失",
    STATUS: "状态差异",
    FINGERPRINT: "内容不一致",
    DUPLICATE: "重复身份",
}

function mapDiffStatus(
    s: BackendReconItem["status"],
): Pick<ReconciliationDifference, "status" | "statusLabel" | "statusTone"> {
    switch (s) {
        case "pending":
            return {
                status: "OPEN",
                statusLabel: "待处理",
                statusTone: "warning",
            }
        case "backfilling":
            return {
                status: "PULLING",
                statusLabel: "补拉中",
                statusTone: "info",
            }
        case "resolved":
            return {
                status: "RESOLVED",
                statusLabel: "已解决",
                statusTone: "success",
            }
        case "confirmed_no_difference":
            return {
                status: "CONFIRMED",
                statusLabel: "确认无误",
                statusTone: "success",
            }
        default:
            return { status: "OPEN", statusLabel: s, statusTone: "neutral" }
    }
}

function mapReconJobStatus(
    s: BackendReconJob["status"],
): Pick<ReconciliationBatch, "status" | "statusLabel"> {
    switch (s) {
        case "running":
            return { status: "RUNNING", statusLabel: "运行中" }
        case "completed":
            return { status: "SUCCEEDED", statusLabel: "完成" }
        case "has_difference":
            return { status: "DIFFERENCE", statusLabel: "有差异" }
        case "failed":
            return { status: "FAILED", statusLabel: "失败" }
        default:
            return { status: "FAILED", statusLabel: s }
    }
}

function toJobRow(job: BackendJob): MallSyncJobRow {
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

function toSnapshotRow(
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

function toMappingTask(
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
                        : "团队待处理",
            assignmentMode: workItem.assignment_mode,
            processingState: "READY" as const,
            ownerUser: workItem.owner_user_id
                ? {
                      id: workItem.owner_user_id,
                      displayName: workItem.owner_user_id,
                  }
                : undefined,
            allowedActions: workItem.allowed_actions,
        },
    }
}

function toDifference(item: BackendReconItem): ReconciliationDifference {
    const dt = mapDiffType(item.difference_type)
    const st = mapDiffStatus(item.status)
    return {
        differenceId: item.id,
        externalOrderNo: item.external_order_no,
        differenceType: dt,
        differenceTypeLabel: DIFF_TYPE_LABEL[dt],
        status: st.status,
        statusLabel: st.statusLabel,
        statusTone: st.statusTone,
        impactSummary: item.resolution ?? item.source_status_code,
    }
}

function mapSourceStatus(s: BackendSourceSystem["status"]): SourceSystemStatus {
    return s === "active" ? "启用" : "停用"
}

function buildMetrics(
    jobs: MallSyncJobRow[],
    mappingTasks: MappingTaskView[],
    recon: ReconciliationBatch | null,
    lagSeconds?: number,
): MallSyncMetric[] {
    const pendingMapping = mappingTasks.filter(
        (t) => t.mappingTaskStatus === "PENDING",
    ).length
    const failedJobs = jobs.filter(
        (j) => j.status === "FAILED" || j.status === "PARTIAL_FAILED",
    ).length
    return [
        {
            key: "lag",
            label: "同步延迟",
            value:
                lagSeconds != null
                    ? `${Math.max(0, Math.round(lagSeconds / 60))} 分`
                    : "—",
            visible: true,
            targetView: "overview",
        },
        {
            key: "failed",
            label: "失败任务",
            count: failedJobs,
            visible: true,
            targetView: "jobs",
        },
        {
            key: "pending",
            label: "待映射",
            count: pendingMapping,
            visible: true,
            targetView: "mapping",
        },
        {
            key: "recon",
            label: "核对差异",
            count: recon?.differenceCount ?? 0,
            visible: true,
            targetView: "reconciliation",
        },
    ]
}

async function resolveMallSourceSystemId(): Promise<{
    id: string
    code: string
    name: string
    environmentLabel: string
    stage: OwnershipStage
} | null> {
    const page = await apiGet<Page<BackendSourceSystem>>(
        "/admin/source-systems",
        {
            page: 1,
            page_size: 50,
            system_type: "MALL",
        },
    )
    const mall =
        page.items.find((s) => s.status === "active") ?? page.items[0] ?? null
    if (!mall) return null
    return {
        id: mall.id,
        code: mall.code,
        name: mall.name,
        environmentLabel: mall.status === "active" ? "启用" : "停用",
        // 阶段未由服务端明确返回时按封存处理，禁止客户端猜测仍可写。
        stage: mall.mall_sync_stage ?? "ARCHIVED",
    }
}

// ─── 读路径 ──────────────────────────────────────────────────────────────────

export async function fetchMallSyncPage(
    input: MallSyncQueryInput,
): Promise<MallSyncPageView> {
    const listParams = { page: 1, page_size: 50 as const }

    const [source, jobsPage, snapshotsPage, mappingPage, reconPage] =
        await Promise.all([
            resolveMallSourceSystemId(),
            apiGet<Page<BackendJob>>("/admin/mall-sales-sync-jobs", listParams),
            apiGet<Page<BackendSnapshot>>(
                "/admin/mall-sales-order-snapshots",
                listParams,
            ),
            apiGet<Page<BackendMappingTask>>(
                "/admin/master-mapping-tasks",
                listParams,
            ),
            apiGet<Page<BackendReconJob>>(
                "/admin/mall-sales-reconciliation-jobs",
                listParams,
            ),
        ])

    let explicitMappingTask: BackendMappingTask | undefined
    if (input.mappingTaskId) {
        explicitMappingTask = await apiGet<BackendMappingTask>(
            `/admin/master-mapping-tasks/${encodeURIComponent(input.mappingTaskId)}`,
            input.workItemId ? { work_item_id: input.workItemId } : undefined,
        )
    }

    let cursor: BackendCursor | null = null
    if (source) {
        try {
            cursor = await apiGet<BackendCursor>(
                "/admin/mall-sales-sync-cursors",
                {
                    source_system_id: source.id,
                },
            )
        } catch {
            cursor = null
        }
    }

    const latestRecon = reconPage.items[0] ?? null
    let differences: ReconciliationDifference[] = []
    if (latestRecon) {
        const itemsPage = await apiGet<Page<BackendReconItem>>(
            `/admin/mall-sales-reconciliation-jobs/${latestRecon.id}/items`,
            listParams,
        )
        differences = itemsPage.items.map(toDifference)
    }

    const jobRows = jobsPage.items.map(toJobRow)
    const jobNoById = new Map(jobRows.map((j) => [j.jobId, j.jobNo]))
    let snapshots = snapshotsPage.items.map((s) => toSnapshotRow(s, jobNoById))
    const snapById = new Map(snapshotsPage.items.map((s) => [s.id, s]))
    const mappingItems = [...mappingPage.items]
    if (explicitMappingTask) {
        const index = mappingItems.findIndex(
            (task) => task.id === explicitMappingTask.id,
        )
        if (index >= 0) mappingItems[index] = explicitMappingTask
        else mappingItems.push(explicitMappingTask)
    }
    let mappingTasks = mappingItems.map((task) => toMappingTask(task, snapById))

    // Client-side q filter only when backend has no free-text search for this surface
    if (input.q?.trim()) {
        const q = input.q.trim().toUpperCase()
        snapshots = snapshots.filter(
            (s) =>
                s.externalOrderNo.toUpperCase().includes(q) ||
                s.syncJobNo.toUpperCase().includes(q),
        )
        mappingTasks = mappingTasks.filter((t) =>
            t.externalOrderNo.toUpperCase().includes(q),
        )
    }
    if (input.mappingType) {
        mappingTasks = mappingTasks.filter(
            (t) => t.mappingType === input.mappingType,
        )
    }

    const recon: ReconciliationBatch | null = latestRecon
        ? {
              jobId: latestRecon.id,
              jobNo: latestRecon.job_no,
              boundaryLabel:
                  instantToIso(latestRecon.source_list_as_of) ??
                  latestRecon.job_no,
              mallCount: latestRecon.source_count,
              erpCount: latestRecon.erp_count,
              differenceCount: latestRecon.difference_count,
              ...mapReconJobStatus(latestRecon.status),
              startedAt: instantToIso(latestRecon.started_at) ?? "",
              finishedAt: instantToIso(latestRecon.finished_at ?? undefined),
              differences,
          }
        : null

    const watermarkIso = instantToIso(cursor?.high_water_updated_at)
    const latestSuccessJob = jobRows.find((j) => j.status === "SUCCEEDED")
    const lagSeconds =
        cursor?.high_water_updated_at != null
            ? Math.max(
                  0,
                  Math.floor(Date.now() / 1000) - cursor.high_water_updated_at,
              )
            : undefined

    const stage: OwnershipStage = source?.stage ?? "ARCHIVED"
    const sourceUnavailable = !source

    const metrics = buildMetrics(jobRows, mappingTasks, recon, lagSeconds)

    const selectedJob =
        jobRows.find((j) => j.jobId === input.jobId) ??
        (input.view === "jobs" ? jobRows[0] : undefined)
    const selectedSnapshot =
        snapshots.find((s) => s.snapshotId === input.snapshotId) ??
        (input.view === "snapshots" ? snapshots[0] : undefined)
    const selectedMappingTask = input.mappingTaskId
        ? mappingTasks.find(
              (task) =>
                  task.mappingTaskId === input.mappingTaskId &&
                  (!input.workItemId ||
                      (task.ownerRoutingState === "CONFIGURED" &&
                          task.workItem.workItemId === input.workItemId)),
          )
        : input.workItemId
          ? mappingTasks.find(
                (task) =>
                    task.ownerRoutingState === "CONFIGURED" &&
                    task.workItem.workItemId === input.workItemId,
            )
          : input.view === "mapping"
            ? mappingTasks[0]
            : undefined

    const selectedDifference = recon?.differences.find(
        (d) => d.differenceId === input.differenceId,
    )

    let emptyReason: MallSyncPageView["emptyReason"]
    if (input.view === "mapping" && mappingTasks.length === 0) {
        emptyReason = "NO_TASKS"
    } else if (!source) {
        emptyReason = "NO_SCOPE"
    }

    const asOf =
        watermarkIso ??
        latestSuccessJob?.finishedAt ??
        latestSuccessJob?.startedAt ??
        instantToIso(jobsPage.items[0]?.created_at) ??
        instantToIso(0)

    return {
        context: {
            sourceSystem: source ?? {
                id: "",
                code: "",
                name: "未配置商城来源",
                environmentLabel: "—",
            },
            ownership: {
                businessType: "VOUCHER",
                stage,
                originSystemSummary: "MALL",
                syncDirection: "MALL_TO_ERP_COMMERCIAL_FACT",
                firstPhasePollingEnabled: Boolean(source),
                mallWriteBoundary:
                    "商城开单商业记录（可继续销售/制卡/绑定/激活/消费）",
                erpWriteBoundary: "ERP 只读接收商业数据；不向商城回写商业修改",
            },
            freshness: {
                currentWatermark: watermarkIso,
                latestSuccessfulJobAt: latestSuccessJob?.finishedAt,
                sourceSafeTime: watermarkIso,
                syncLagSeconds: lagSeconds,
                viewProjectedAt: asOf ?? "",
            },
            metrics,
            sourceUnavailable,
            sourceUnavailableMessage: sourceUnavailable
                ? "未找到启用的商城来源系统；请先在来源系统中登记 MALL 类型来源。"
                : undefined,
            hasSourceScope: Boolean(source),
            scheduledIncrementalNote:
                "系统定时增量按调度契约独立运行；授权管理员可直接提交带理由的人工增量。",
        },
        jobs: jobRows,
        snapshots,
        mappingTasks,
        reconciliation: recon,
        history: [],
        selectedJob,
        selectedSnapshot,
        selectedMappingTask,
        selectedDifference,
        emptyReason,
    }
}

// ─── 写路径 ──────────────────────────────────────────────────────────────────

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
        message: "已按原作业类型/范围创建新重试作业；水位不回退。",
    }
}

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

// ─── 来源系统（D01，样板已对齐） ────────────────────────────────────────────

export const fetchSourceSystems = async (
    params: SourceSystemListParams,
): Promise<SourceSystemPage> => {
    const page = await apiGet<Page<BackendSourceSystem>>(
        "/admin/source-systems",
        {
            page: params.page,
            page_size: params.page_size,
        },
    )
    const items: SourceSystemItem[] = page.items.map((s) => ({
        id: s.id,
        code: s.code,
        name: s.name,
        system_type: s.system_type as SourceSystemType,
        status: mapSourceStatus(s.status),
        created_at: s.created_at,
    }))
    return {
        items,
        total: page.total,
        page: page.page,
        page_size: page.page_size,
    }
}
