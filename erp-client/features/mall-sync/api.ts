/**
 * W17 商城同步 · 真实 HTTP API（P4 F8）。
 * 导出签名保持与 queries.ts / 页面一致；字段适配仅在本文件完成。
 * 后端域：mall_sync + source_registry（/admin/source-systems 样板已有）。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type {
  ConfirmMappingResult,
  DeferMappingResult,
  DemoRole,
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
  SourceSystemItem,
  SourceSystemListParams,
  SourceSystemPage,
  SourceSystemStatus,
  SourceSystemType,
  TriggerMallSyncResult,
} from "@/features/mall-sync/types"
import {
  DEMO_ROLE_LABEL,
  DIRECTION_LABEL,
  JOB_TYPE_LABEL,
  MAPPING_TYPE_LABEL,
  OWNER_ROLE_LABEL,
  STAGE_LABEL,
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
  owner_role: string
  owner_user_id?: string | null
  resolution?: string | null
  resolved_at?: number | null
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
  created_at: number
  version?: number
}

type WorkItemView = {
  id: string
  work_item_type: string
  business_object_type: string
  business_object_id: string
  subject_version?: string | null
  status: string
  owner_role?: string | null
  owner_user_id?: string | null
  completion_action: string
  version: number
}

// ─── Query input（契约保持） ─────────────────────────────────────────────────

export type MallSyncQueryInput = {
  view: MallSyncViewName
  demoRole: DemoRole
  demoStage?: OwnershipStage
  policy?: "missing" | "configured"
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

/** 演示开关：后端无对应策略 API，保留空实现以稳定 queries 导出签名 */
export function setMallSyncDemoStage(stage: OwnershipStage) {
  void stage
}

export function setMallSyncPolicyConfigured(configured: boolean) {
  void configured
}

export function setMallSyncSourceUnavailable(unavailable: boolean) {
  void unavailable
}

export function getMallSyncStageLabels() {
  return { STAGE_LABEL, DIRECTION_LABEL }
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

function mapJobType(
  t: BackendJob["job_type"]
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

function mapJobStatus(
  s: BackendJob["status"]
): Pick<
  MallSyncJobRow,
  "status" | "statusLabel" | "statusTone"
> {
  switch (s) {
    case "running":
      return { status: "RUNNING", statusLabel: "运行中", statusTone: "info" }
    case "success":
      return { status: "SUCCEEDED", statusLabel: "成功", statusTone: "success" }
    case "partial_failure":
      return {
        status: "PARTIAL_FAILED",
        statusLabel: "部分失败",
        statusTone: "warning",
      }
    case "failed":
      return { status: "FAILED", statusLabel: "失败", statusTone: "destructive" }
    default:
      return { status: "FAILED", statusLabel: s, statusTone: "neutral" }
  }
}

function mapSnapshotStatus(
  s: BackendSnapshot["mapping_status"]
): Pick<MallSnapshotRow, "mappingStatus" | "mappingStatusLabel"> {
  switch (s) {
    case "pending":
      return { mappingStatus: "PENDING_MAPPING", mappingStatusLabel: "待映射" }
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
  t: BackendMappingTask["mapping_type"]
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
  s: BackendMappingTask["status"]
): Pick<
  MappingTaskView,
  "mappingTaskStatus" | "mappingTaskStatusLabel"
> {
  switch (s) {
    case "pending":
      return { mappingTaskStatus: "PENDING", mappingTaskStatusLabel: "待处理" }
    case "resolved":
      return { mappingTaskStatus: "RESOLVED", mappingTaskStatusLabel: "已解决" }
    case "unresolvable":
      return {
        mappingTaskStatus: "UNRESOLVABLE",
        mappingTaskStatusLabel: "无法处理",
      }
    case "closed":
      return { mappingTaskStatus: "CLOSED", mappingTaskStatusLabel: "关闭" }
    default:
      return { mappingTaskStatus: "PENDING", mappingTaskStatusLabel: s }
  }
}

function mapOwnerRole(
  role: string
): "SALES" | "OPERATIONS" | "FINANCE" | undefined {
  const r = role.trim().toUpperCase()
  if (r === "SALES" || r === "销售") return "SALES"
  if (r === "OPERATIONS" || r === "运营" || r === "OPS") return "OPERATIONS"
  if (r === "FINANCE" || r === "财务") return "FINANCE"
  return undefined
}

function mapDiffType(
  t: BackendReconItem["difference_type"]
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
  FINGERPRINT: "内容指纹差异",
  DUPLICATE: "重复身份",
}

function mapDiffStatus(
  s: BackendReconItem["status"]
): Pick<
  ReconciliationDifference,
  "status" | "statusLabel" | "statusTone"
> {
  switch (s) {
    case "pending":
      return { status: "OPEN", statusLabel: "待处理", statusTone: "warning" }
    case "backfilling":
      return { status: "PULLING", statusLabel: "补拉中", statusTone: "info" }
    case "resolved":
      return { status: "RESOLVED", statusLabel: "已解决", statusTone: "success" }
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
  s: BackendReconJob["status"]
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
    triggeredBy: "系统",
    watermarkAdvanced: st.status === "SUCCEEDED",
    allowedActions: failed ? ["RETRY_FAILED_JOB"] : [],
    actionBlockers: [],
  }
}

function toSnapshotRow(
  snap: BackendSnapshot,
  jobNoById: Map<string, string>
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
    syncJobNo: jobNoById.get(snap.sync_job_id) ?? snap.sync_job_id.slice(0, 12),
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
  snapById: Map<string, BackendSnapshot>
): MappingTaskView {
  const mappingType = mapMappingType(task.mapping_type)
  const st = mapMappingStatus(task.status)
  const ownerRole = mapOwnerRole(task.owner_role)
  const snap = snapById.get(task.source_snapshot_id)
  const externalOrderNo = snap?.external_order_no ?? "—"

const typedBase = {
    mappingTaskId: task.id,
    sourceSnapshotId: task.source_snapshot_id,
    externalOrderNo,
    mappingType,
    mappingTypeLabel: MAPPING_TYPE_LABEL[mappingType],
    ...st,
    sourceEvidence: [
      {
        field: "external_order_no",
        label: "来源单号",
        value: externalOrderNo,
      },
      {
        field: "mapping_type",
        label: "映射类型",
        value: MAPPING_TYPE_LABEL[mappingType],
      },
    ],
    candidateTargets: [] as Array<{
      objectType: string
      objectId: string
      stableNo: string
      label: string
      currentRevisionId: string
      eligibility: "ELIGIBLE" | "INELIGIBLE"
      reason: string
    }>,
    currentTargets: [] as Array<{
      objectType: string
      objectId: string
      stableNo: string
      label: string
      relationRole: string
      validFrom: string
      validTo?: string
      status: string
    }>,
    impactSummary: task.resolution ?? "待确认 ERP 目标",
    resolutionHistory: task.resolution
      ? [
          {
            action: "RESOLVE",
            result: task.resolution,
            handledBy: task.owner_role,
            handledAt: instantToIso(task.resolved_at ?? undefined) ?? "",
          },
        ]
      : [],
    allowedActions:
      st.mappingTaskStatus === "PENDING"
        ? ["CONFIRM_TARGET", "DEFER_MAPPING_TASK"]
        : st.mappingTaskStatus === "RESOLVED"
          ? ["REAPPLY"]
          : [],
    actionBlockers: [
      {
        action: "CONFIRM_TARGET",
        code: "CANDIDATES_NOT_PROVIDED",
        message:
          "后端映射任务未返回候选目标列表；确认时须由调用方提供 targetObjectId（backend_gap）。",
      },
    ],
    lockVersion: task.version,
    hasConflict: st.mappingTaskStatus === "PENDING",
  }

  if (!ownerRole) {
    return {
      ...typedBase,
      ownerRoutingState: "MISSING" as const,
    }
  }

  return {
    ...typedBase,
    ownerRoutingState: "CONFIGURED" as const,
    ownerRole,
    ownerRoleLabel: OWNER_ROLE_LABEL[ownerRole],
    ownerUserId: task.owner_user_id ?? undefined,
    workItem: {
      workItemId: `mapping:${task.id}`,
      workItemType: "BUSINESS_EXCEPTION" as const,
      businessObjectType: "MASTER_MAPPING_TASK" as const,
      businessObjectId: task.id,
      subjectVersion: String(task.version),
      subjectHash: `v${task.version}`,
      status:
        st.mappingTaskStatus === "RESOLVED"
          ? ("COMPLETED" as const)
          : ("PENDING" as const),
      statusLabel:
        st.mappingTaskStatus === "RESOLVED" ? "已完成" : "待处理",
      completionAction: "CONFIRM_MAPPING",
      claimedBy: task.owner_user_id ?? undefined,
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
  lagSeconds?: number
): MallSyncMetric[] {
  const pendingMapping = mappingTasks.filter(
    (t) => t.mappingTaskStatus === "PENDING"
  ).length
  const failedJobs = jobs.filter(
    (j) => j.status === "FAILED" || j.status === "PARTIAL_FAILED"
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
} | null> {
  const page = await apiGet<Page<BackendSourceSystem>>("/admin/source-systems", {
    page: 1,
    page_size: 50,
    system_type: "MALL",
  })
  const mall =
    page.items.find((s) => s.status === "active") ?? page.items[0] ?? null
  if (!mall) return null
  return {
    id: mall.id,
    code: mall.code,
    name: mall.name,
    environmentLabel: mall.status === "active" ? "启用" : "停用",
  }
}

// ─── 读路径 ──────────────────────────────────────────────────────────────────

export async function fetchMallSyncPage(
  input: MallSyncQueryInput
): Promise<MallSyncPageView> {
  const listParams = { page: 1, page_size: 50 as const }

  const [source, jobsPage, snapshotsPage, mappingPage, reconPage] =
    await Promise.all([
      resolveMallSourceSystemId(),
      apiGet<Page<BackendJob>>("/admin/mall-sales-sync-jobs", listParams),
      apiGet<Page<BackendSnapshot>>(
        "/admin/mall-sales-order-snapshots",
        listParams
      ),
      apiGet<Page<BackendMappingTask>>(
        "/admin/master-mapping-tasks",
        listParams
      ),
      apiGet<Page<BackendReconJob>>(
        "/admin/mall-sales-reconciliation-jobs",
        listParams
      ),
    ])

  let cursor: BackendCursor | null = null
  if (source) {
    try {
      cursor = await apiGet<BackendCursor>("/admin/mall-sales-sync-cursors", {
        source_system_id: source.id,
      })
    } catch {
      cursor = null
    }
  }

  const latestRecon = reconPage.items[0] ?? null
  let differences: ReconciliationDifference[] = []
  if (latestRecon) {
    const itemsPage = await apiGet<Page<BackendReconItem>>(
      `/admin/mall-sales-reconciliation-jobs/${latestRecon.id}/items`,
      listParams
    )
    differences = itemsPage.items.map(toDifference)
  }

  const jobRows = jobsPage.items.map(toJobRow)
  const jobNoById = new Map(jobRows.map((j) => [j.jobId, j.jobNo]))
  let snapshots = snapshotsPage.items.map((s) => toSnapshotRow(s, jobNoById))
  const snapById = new Map(snapshotsPage.items.map((s) => [s.id, s]))
  let mappingTasks = mappingPage.items.map((t) => toMappingTask(t, snapById))

  // Client-side q filter only when backend has no free-text search for this surface
  if (input.q?.trim()) {
    const q = input.q.trim().toUpperCase()
    snapshots = snapshots.filter(
      (s) =>
        s.externalOrderNo.toUpperCase().includes(q) ||
        s.syncJobNo.toUpperCase().includes(q)
    )
    mappingTasks = mappingTasks.filter((t) =>
      t.externalOrderNo.toUpperCase().includes(q)
    )
  }
  if (input.mappingType) {
    mappingTasks = mappingTasks.filter((t) => t.mappingType === input.mappingType)
  }

  const recon: ReconciliationBatch | null = latestRecon
    ? {
        jobId: latestRecon.id,
        jobNo: latestRecon.job_no,
        boundaryLabel:
          instantToIso(latestRecon.source_list_as_of) ?? latestRecon.job_no,
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
          Math.floor(Date.now() / 1000) - cursor.high_water_updated_at
        )
      : undefined

  const stage: OwnershipStage = "FIRST_PHASE_MALL_OWNED"
  const role = input.demoRole
  const sourceUnavailable = !source

  const metrics = buildMetrics(jobRows, mappingTasks, recon, lagSeconds)

  const selectedJob =
    jobRows.find((j) => j.jobId === input.jobId) ??
    (input.view === "jobs" ? jobRows[0] : undefined)
  const selectedSnapshot =
    snapshots.find((s) => s.snapshotId === input.snapshotId) ??
    (input.view === "snapshots" ? snapshots[0] : undefined)
  let selectedMappingTask =
    mappingTasks.find((t) => t.mappingTaskId === input.mappingTaskId) ??
    mappingTasks.find(
      (t) =>
        t.ownerRoutingState === "CONFIGURED" &&
        t.workItem.workItemId === input.workItemId
    ) ??
    (input.view === "mapping" ? mappingTasks[0] : undefined)

  if (input.workItemId && !input.mappingTaskId) {
    const byWi = mappingTasks.find(
      (t) =>
        t.ownerRoutingState === "CONFIGURED" &&
        t.workItem.workItemId === input.workItemId
    )
    if (byWi) selectedMappingTask = byWi
  }

  const selectedDifference = recon?.differences.find(
    (d) => d.differenceId === input.differenceId
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
      manualGovernancePolicy: {
        state: "MISSING",
        blockerCode: "MANUAL_GOVERNANCE_POLICY_MISSING",
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
      viewerRole: role,
      viewerRoleLabel: DEMO_ROLE_LABEL[role],
      hasSourceScope: Boolean(source),
      scheduledIncrementalNote:
        "系统定时增量按调度契约独立运行；人工立即增量需治理策略与权限。",
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
  demoRole: DemoRole
  policyConfigured?: boolean
  stage?: OwnershipStage
}): Promise<TriggerMallSyncResult> {
  if (input.demoRole !== "admin") {
    return {
      status: "failed",
      code: "FORBIDDEN",
      message: "仅系统管理员可触发立即增量",
    }
  }
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
  const job = await apiPost<BackendJob>("/admin/mall-sales-sync-jobs", {
    source_system_id: source.id,
    job_type: "incremental",
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
  demoRole: DemoRole
  policyConfigured?: boolean
  stage?: OwnershipStage
}): Promise<TriggerMallSyncResult> {
  if (input.demoRole !== "admin") {
    return {
      status: "failed",
      code: "FORBIDDEN",
      message: "仅系统管理员可按单补拉",
    }
  }
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
  // backend CreateMallSalesSyncJobRequest 无 external_order_no；仅创建 single_order_backfill 作业
  const job = await apiPost<BackendJob>("/admin/mall-sales-sync-jobs", {
    source_system_id: source.id,
    job_type: "single_order_backfill",
  })
  return {
    status: "succeeded",
    jobId: job.id,
    jobNo: job.id.slice(0, 12).toUpperCase(),
    message: `已创建按单补拉作业（来源单号 ${input.externalOrderNo.trim()} 由执行器消费；创建契约未携带单号字段）。`,
  }
}

export async function retryFailedJob(input: {
  jobId: string
  reason: string
  demoRole: DemoRole
  stage?: OwnershipStage
}): Promise<TriggerMallSyncResult> {
  if (input.demoRole !== "admin") {
    return { status: "failed", code: "FORBIDDEN", message: "仅管理员可重试" }
  }
  const original = await apiGet<BackendJob>(
    `/admin/mall-sales-sync-jobs/${input.jobId}`
  )
  if (original.status !== "failed" && original.status !== "partial_failure") {
    return {
      status: "failed",
      code: "NOT_RETRYABLE",
      message: "仅失败/部分失败任务可重试",
    }
  }
  const job = await apiPost<BackendJob>("/admin/mall-sales-sync-jobs", {
    source_system_id: original.source_system_id,
    job_type: original.job_type,
    range_start: original.range_start ?? undefined,
    range_end: original.range_end ?? undefined,
  })
  return {
    status: "succeeded",
    jobId: job.id,
    jobNo: job.id.slice(0, 12).toUpperCase(),
    message: "已按原作业类型/范围创建新重试作业；水位不回退。",
  }
}

export async function claimMappingWorkItem(input: {
  workItemId: string
  subjectVersion: string
}): Promise<{ workItemId: string; subjectVersion: string }> {
  // workItemId may be synthetic `mapping:{taskId}` when backend has no linked work_item
  if (input.workItemId.startsWith("mapping:")) {
    return {
      workItemId: input.workItemId,
      subjectVersion: input.subjectVersion,
    }
  }
  const version = Number(input.subjectVersion)
  const result = await apiPost<WorkItemView>(
    `/admin/work-items/${input.workItemId}/claim`,
    { version: Number.isFinite(version) && version > 0 ? version : 1 }
  )
  return {
    workItemId: result.id,
    subjectVersion: result.subject_version ?? String(result.version),
  }
}

export async function confirmMapping(input: {
  mappingTaskId: string
  workItemId: string
  expectedSubjectVersion: string
  expectedLockVersion: number
  targetObjectId: string
  targetLabel: string
  evidenceNote: string
  demoRole: DemoRole
  stage?: OwnershipStage
}): Promise<ConfirmMappingResult> {
  if (input.demoRole === "admin") {
    return {
      status: "failed",
      code: "ADMIN_CANNOT_CONFIRM_MAPPING",
      message: "系统管理员不能替业务角色确认映射",
    }
  }
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

  const resolution = `target=${input.targetObjectId};label=${input.targetLabel};note=${input.evidenceNote.trim()}`
  await apiPost<BackendMappingTask>(
    `/admin/master-mapping-tasks/${input.mappingTaskId}/resolve`,
    { kind: "resolved", resolution }
  )

  if (!input.workItemId.startsWith("mapping:")) {
    const version = Number(input.expectedSubjectVersion)
    await apiPost(`/admin/work-items/${input.workItemId}/complete`, {
      version: Number.isFinite(version) && version > 0 ? version : 1,
    })
  }

  const recordedAt = new Date().toISOString()
  return {
    status: "succeeded",
    mappingTaskId: input.mappingTaskId,
    mappingTaskStatus: "RESOLVED",
    externalIdentityMapId: `eim_${input.mappingTaskId}`,
    mappingTargetId: `mtgt_${input.targetObjectId}`,
    recordedAt,
    message:
      "映射已解决。尚未形成销售版本；请使用原数据重新归集（若后端支持）。",
  }
}

export async function deferMapping(input: {
  mappingTaskId: string
  workItemId: string
  expectedSubjectVersion: string
  reasonCode: string
  note?: string
  queueContextId: string
  demoRole: DemoRole
}): Promise<DeferMappingResult> {
  if (input.demoRole === "admin") {
    return {
      status: "failed",
      code: "ADMIN_DEFER_N_A",
      message: "管理员请使用指派；跳过由业务责任人操作",
    }
  }
  if (!input.reasonCode) {
    return {
      status: "failed",
      code: "REASON_REQUIRED",
      message: "跳过须选择结构化原因",
    }
  }
  if (!input.workItemId.startsWith("mapping:")) {
    const version = Number(input.expectedSubjectVersion)
    await apiPost(`/admin/work-items/${input.workItemId}/defer`, {
      version: Number.isFinite(version) && version > 0 ? version : 1,
      comment: `${input.reasonCode}${input.note ? `: ${input.note}` : ""}`,
    })
  }
  return {
    status: "succeeded",
    mappingTaskId: input.mappingTaskId,
    mappingTaskStatus: "PENDING",
    leaseDisposition: "RELEASED",
    message: "已记录跳过原因。映射任务仍为待处理。",
  }
}

export async function reapplyMallSnapshot(input: {
  mappingTaskId: string
  sourceSnapshotId: string
  demoRole: DemoRole
  stage?: OwnershipStage
}): Promise<ReapplyResult> {
  void input
  // backend_gap: 无 reapply 专用 HTTP
  return {
    status: "failed",
    code: "BACKEND_GAP_REAPPLY",
    message: "后端尚未提供商城快照重新归集接口（mall_sync reapply）。",
  }
}

export async function resolveUnknownReapply(input: {
  mappingTaskId: string
  operationId: string
  settle?: boolean
}): Promise<ReapplyResult> {
  void input
  return {
    status: "failed",
    code: "BACKEND_GAP_REAPPLY",
    message: "后端尚未提供重新归集查询/结算接口。",
  }
}

export async function assignMappingTask(input: {
  mappingTaskId: string
  targetOwnerRole: "SALES" | "OPERATIONS" | "FINANCE"
  reason: string
  demoRole: DemoRole
}): Promise<{ status: "succeeded" | "failed"; message: string; code?: string }> {
  if (input.demoRole !== "admin") {
    return {
      status: "failed",
      code: "FORBIDDEN",
      message: "仅管理员可指派映射任务",
    }
  }
  void input.mappingTaskId
  void input.reason
  // backend_gap: master_mapping_task 无 assign 端点；仅有 create 时指定 owner_role
  return {
    status: "failed",
    code: "BACKEND_GAP_ASSIGN",
    message: `后端映射任务无指派接口，无法切换到 ${OWNER_ROLE_LABEL[input.targetOwnerRole]}。`,
  }
}

// ─── 来源系统（D01，样板已对齐） ────────────────────────────────────────────

export const fetchSourceSystems = async (
  params: SourceSystemListParams
): Promise<SourceSystemPage> => {
  const page = await apiGet<Page<BackendSourceSystem>>("/admin/source-systems", {
    page: params.page,
    page_size: params.page_size,
  })
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
