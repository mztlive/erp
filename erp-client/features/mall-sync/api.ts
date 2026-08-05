/**
 * W17 session-mock API：queryFn / mutationFn 纯函数。
 * - 人工治理策略未配置时拒绝立即增量 / 按单补拉
 * - 管理员不得确认业务映射
 * - 映射与重新归集状态独立；结果未知不自动推进
 * - mock 永不返回玩法/卡号/卡密/绑定手机/连接/密钥
 */

import { mockDelay } from "@/lib/mock-delay"
import type {
  ConfirmMappingResult,
  DeferMappingResult,
  DemoRole,
  MallSyncJobRow,
  MallSyncMetric,
  MallSyncPageView,
  MallSyncViewName,
  MappingTaskView,
  OwnershipStage,
  ReapplyResult,
  TriggerMallSyncResult,
} from "@/features/mall-sync/types"
import {
  DEMO_ROLE_LABEL,
  DIRECTION_LABEL,
  JOB_TYPE_LABEL,
  OWNER_ROLE_LABEL,
  STAGE_LABEL,
} from "@/features/mall-sync/types"
import {
  MALL_HISTORY,
  MALL_MAPPING_TASKS,
  MALL_RECONCILIATION,
  MALL_SNAPSHOTS,
  MALL_SOURCE,
  MALL_SYNC_JOBS,
} from "@/mock/mall-sync"
import {
  applyWorkItemActionSession,
  claimWorkItemSession,
  completeWorkItemSession,
  getCompletedQueueTaskIds,
  getHeldQueueTaskIds,
  getSessionLease,
  markQueueTaskCompleted,
  markQueueTaskHeld,
  WorkItemMockError,
} from "@/mock/session-state"

const WORKSPACE_ID = "W17"

/** 会话内演示控制（不写 localStorage） */
let sessionStage: OwnershipStage = "FIRST_PHASE_MALL_OWNED"
let sessionPolicyConfigured = false
let sessionSourceUnavailable = false
let sessionManualJobs: MallSyncJobRow[] = []
const resolvedMappings = new Map<
  string,
  {
    mappingTaskStatus: "RESOLVED"
    targetId: string
    targetLabel: string
    externalIdentityMapId: string
    recordedAt: string
    evidenceNote: string
  }
>()
const reapplyState = new Map<
  string,
  {
    operationId: string
    status: "QUEUED" | "RUNNING" | "SUCCEEDED" | "FAILED" | "UNKNOWN"
    lastUpdatedAt: string
    salesOrderId?: string
    salesOrderNo?: string
    salesOrderRevisionId?: string
    receivableResultReference?: string
  }
>()
const deferredNotes = new Map<string, { reasonCode: string; note?: string; at: string }>()

// seed reapply UNKNOWN from mock
for (const task of MALL_MAPPING_TASKS) {
  if (task.reapplyOperation) {
    reapplyState.set(task.mappingTaskId, { ...task.reapplyOperation })
  }
}

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

export function setMallSyncDemoStage(stage: OwnershipStage) {
  sessionStage = stage
}

export function setMallSyncPolicyConfigured(configured: boolean) {
  sessionPolicyConfigured = configured
}

export function setMallSyncSourceUnavailable(unavailable: boolean) {
  sessionSourceUnavailable = unavailable
}

function roleOwnsMapping(
  role: DemoRole,
  ownerRole?: "SALES" | "OPERATIONS" | "FINANCE"
): boolean {
  if (!ownerRole) return false
  if (role === "sales") return ownerRole === "SALES"
  if (role === "operations") return ownerRole === "OPERATIONS"
  if (role === "finance") return ownerRole === "FINANCE"
  return false
}

function filterMappingForRole(
  task: MappingTaskView,
  role: DemoRole
): MappingTaskView | null {
  const completed = getCompletedQueueTaskIds(WORKSPACE_ID)
  if (
    task.ownerRoutingState === "CONFIGURED" &&
    completed.has(task.workItem.workItemId)
  ) {
    // still show resolved tasks for reapply demos
    if (task.mappingTaskStatus !== "RESOLVED" && !resolvedMappings.has(task.mappingTaskId)) {
      return null
    }
  }

  // 业务角色只看自己责任类型；管理员看全部（含 MISSING）
  if (role !== "admin") {
    if (task.ownerRoutingState === "MISSING") return null
    if (!roleOwnsMapping(role, task.ownerRole)) return null
  }

  return projectMappingTask(task, role)
}

function projectMappingTask(
  seed: MappingTaskView,
  role: DemoRole
): MappingTaskView {
  const resolved = resolvedMappings.get(seed.mappingTaskId)
  const reapply = reapplyState.get(seed.mappingTaskId) ?? seed.reapplyOperation
  const held = getHeldQueueTaskIds(WORKSPACE_ID)
  const publicLease =
    seed.ownerRoutingState === "CONFIGURED"
      ? getSessionLease(seed.workItem.workItemId)
      : null

  let mappingTaskStatus = seed.mappingTaskStatus
  let mappingTaskStatusLabel = seed.mappingTaskStatusLabel
  let resolutionHistory = [...seed.resolutionHistory]
  let currentTargets = [...seed.currentTargets]
  let allowedActions = [...seed.allowedActions]
  let actionBlockers = [...seed.actionBlockers]

  if (resolved) {
    mappingTaskStatus = "RESOLVED"
    mappingTaskStatusLabel = "已解决"
    resolutionHistory = [
      ...resolutionHistory,
      {
        action: "CONFIRM_TARGET",
        result: `已确认 ${resolved.targetLabel}`,
        handledBy: DEMO_ROLE_LABEL[role],
        handledAt: resolved.recordedAt,
        evidenceReference: resolved.externalIdentityMapId,
      },
    ]
    currentTargets = [
      {
        objectType: "MAPPED_TARGET",
        objectId: resolved.targetId,
        stableNo: resolved.targetId.toUpperCase(),
        label: resolved.targetLabel,
        relationRole: "PRIMARY",
        validFrom: resolved.recordedAt.slice(0, 10),
        status: "ACTIVE",
      },
    ]
    // 映射解决后只允许重新归集相关动作
    allowedActions = ["REAPPLY", "QUERY_REAPPLY"]
    actionBlockers = []
  }

  const deferred = deferredNotes.get(seed.mappingTaskId)
  if (deferred && mappingTaskStatus === "PENDING") {
    resolutionHistory = [
      ...resolutionHistory,
      {
        action: "DEFER_MAPPING_TASK",
        result: `先跳过：${deferred.reasonCode}${deferred.note ? ` · ${deferred.note}` : ""}（任务仍待处理）`,
        handledBy: DEMO_ROLE_LABEL[role],
        handledAt: deferred.at,
      },
    ]
  }

  // 管理员：技术动作，禁止 CONFIRM_TARGET
  if (role === "admin") {
    allowedActions = allowedActions.filter(
      (a) => a !== "CONFIRM_TARGET" && a !== "REAPPLY"
    )
    if (!allowedActions.includes("ASSIGN") && mappingTaskStatus === "PENDING") {
      allowedActions = [...allowedActions, "ASSIGN", "RETRY_TECH"]
    }
    if (
      mappingTaskStatus === "PENDING" &&
      !actionBlockers.some((b) => b.action === "CONFIRM_TARGET")
    ) {
      actionBlockers = [
        ...actionBlockers,
        {
          action: "CONFIRM_TARGET",
          code: "ADMIN_CANNOT_CONFIRM_MAPPING",
          message: "系统管理员只能补拉/重试/指派/排障，不能替业务确认映射",
        },
      ]
    }
  } else if (seed.ownerRoutingState === "CONFIGURED") {
    // 业务角色：仅责任匹配时可确认
    if (!roleOwnsMapping(role, seed.ownerRole)) {
      allowedActions = allowedActions.filter((a) => a !== "CONFIRM_TARGET")
    }
  }

  // MISSING：强制无确认
  if (seed.ownerRoutingState === "MISSING") {
    allowedActions = []
    if (!actionBlockers.some((b) => b.code === "OWNER_ROUTING_MISSING")) {
      actionBlockers = [
        ...actionBlockers,
        {
          action: "CONFIRM_TARGET",
          code: "OWNER_ROUTING_MISSING",
          message: "责任路由未配置，不可执行确认",
        },
      ]
    }
  }

  // 重新归集状态覆盖
  const reapplyOperation = reapply
    ? {
        operationId: reapply.operationId,
        status: reapply.status,
        statusLabel:
          reapply.status === "UNKNOWN"
            ? "结果未知"
            : reapply.status === "SUCCEEDED"
              ? "成功"
              : reapply.status === "FAILED"
                ? "失败"
                : reapply.status === "RUNNING"
                  ? "运行中"
                  : "排队中",
        lastUpdatedAt: reapply.lastUpdatedAt,
        salesOrderId: reapply.salesOrderId,
        salesOrderNo: reapply.salesOrderNo,
        salesOrderRevisionId: reapply.salesOrderRevisionId,
        receivableResultReference: reapply.receivableResultReference,
      }
    : undefined

  if (reapplyOperation?.status === "UNKNOWN") {
    actionBlockers = [
      ...actionBlockers.filter((b) => b.code !== "REAPPLY_UNKNOWN"),
      {
        action: "ADVANCE_QUEUE",
        code: "REAPPLY_UNKNOWN",
        message: "重新归集结果未知，停留当前项，不自动完成/下一项",
      },
    ]
  }

  const workItemPatch =
    seed.ownerRoutingState === "CONFIGURED"
      ? {
          ...seed.workItem,
          status:
            held.has(seed.workItem.workItemId) && seed.workItem.status !== "COMPLETED"
              ? ("PENDING" as const)
              : seed.workItem.status,
          statusLabel: held.has(seed.workItem.workItemId)
            ? "已跳过（仍待处理）"
            : seed.workItem.statusLabel,
          claimedBy: publicLease?.ownerUserId ?? seed.workItem.claimedBy,
          subjectVersion:
            publicLease?.subjectVersion ?? seed.workItem.subjectVersion,
        }
      : undefined

  if (seed.ownerRoutingState === "MISSING") {
    return {
      ...seed,
      mappingTaskStatus,
      mappingTaskStatusLabel,
      resolutionHistory,
      currentTargets,
      allowedActions,
      actionBlockers,
      reapplyOperation,
      ownerRoutingState: "MISSING",
    }
  }

  return {
    ...seed,
    mappingTaskStatus,
    mappingTaskStatusLabel,
    resolutionHistory,
    currentTargets,
    allowedActions,
    actionBlockers,
    reapplyOperation,
    ownerRoutingState: "CONFIGURED",
    ownerRole: seed.ownerRole,
    ownerRoleLabel: seed.ownerRoleLabel ?? OWNER_ROLE_LABEL[seed.ownerRole],
    ownerUserId: seed.ownerUserId,
    workItem: workItemPatch!,
  }
}

function buildOwnership(stage: OwnershipStage) {
  if (stage === "SECOND_PHASE_ERP_OWNED") {
    return {
      businessType: "VOUCHER" as const,
      stage,
      originSystemSummary: "ERP" as const,
      mallOwnedOrderCount: 0,
      erpOwnedOrderCount: 2_048,
      syncDirection: "SEALED_HISTORY" as const,
      firstPhasePollingEnabled: false,
      sealedAt: "2026-07-15T18:00:00+08:00",
      finalWatermark: "2026-07-15T18:00:00+08:00",
      mallWriteBoundary: "商城开单已封存；商城执行信息见执行信息页",
      erpWriteBoundary: "ERP 全面服务；商城同步仅历史只读，当前治理见执行信息 / 接口错误中心",
    }
  }
  return {
    businessType: "VOUCHER" as const,
    stage,
    originSystemSummary: "MALL" as const,
    mallOwnedOrderCount: 1_206,
    erpOwnedOrderCount: 842,
    syncDirection: "MALL_TO_ERP_COMMERCIAL_FACT" as const,
    firstPhasePollingEnabled: true,
    mallWriteBoundary: "商城开单商业记录（可继续销售/制卡/绑定/激活/消费）",
    erpWriteBoundary: "ERP 只读接收商业数据；不向商城回写商业修改",
  }
}

function metricsForRole(
  role: DemoRole,
  stage: OwnershipStage,
  mappingTasks: MappingTaskView[]
): MallSyncMetric[] {
  if (stage === "SECOND_PHASE_ERP_OWNED") {
    return [
      {
        key: "sealed",
        label: "封存状态",
        value: "已封存",
        detail: "历史只读",
        visible: true,
        targetView: "history",
      },
    ]
  }

  const pendingMapping = mappingTasks.filter(
    (t) => t.mappingTaskStatus === "PENDING"
  ).length
  const pendingReapply = mappingTasks.filter(
    (t) =>
      t.mappingTaskStatus === "RESOLVED" &&
      t.reapplyOperation &&
      t.reapplyOperation.status !== "SUCCEEDED"
  ).length
  const failedJobs = [...MALL_SYNC_JOBS, ...sessionManualJobs].filter(
    (j) => j.status === "FAILED" || j.status === "PARTIAL_FAILED"
  ).length

  const adminMetrics: MallSyncMetric[] = [
    {
      key: "lag",
      label: "同步延迟",
      value: sessionSourceUnavailable ? "同步进度未推进" : "4 分",
      detail: sessionSourceUnavailable ? "来源不可用" : "最近成功 09:12",
      visible: true,
      targetView: "overview",
    },
    {
      key: "failed",
      label: "失败任务",
      count: failedJobs,
      detail: "未恢复",
      visible: true,
      targetView: "jobs",
      targetFilter: { status: "failed" },
    },
    {
      key: "pending",
      label: "待映射",
      count: pendingMapping,
      detail: role === "admin" ? "全范围" : "我的责任",
      visible: true,
      targetView: "mapping",
    },
    {
      key: "recon",
      label: "核对差异",
      count: MALL_RECONCILIATION.differenceCount,
      detail: "完整版本标识",
      visible: true,
      targetView: "reconciliation",
    },
    {
      key: "reapply",
      label: "待重新归集",
      count: pendingReapply,
      detail: "映射已解决",
      visible: true,
      targetView: "mapping",
      targetFilter: { mappingStatus: "resolved" },
    },
  ]

  if (role === "admin") return adminMetrics
  // 业务：不展示失败任务技术数，保留待映射/重新归集
  return adminMetrics.filter((m) => m.key === "pending" || m.key === "reapply" || m.key === "recon")
}

export async function fetchMallSyncPage(
  input: MallSyncQueryInput
): Promise<MallSyncPageView> {
  await mockDelay()

  if (input.demoStage) sessionStage = input.demoStage
  if (input.policy === "configured") sessionPolicyConfigured = true
  if (input.policy === "missing") sessionPolicyConfigured = false
  if (input.sourceUnavailable != null) {
    sessionSourceUnavailable = input.sourceUnavailable
  }

  const stage = sessionStage
  const role = input.demoRole
  const policyConfigured = sessionPolicyConfigured
  const ownership = buildOwnership(stage)

  // 封存后强制 history 语义（URL 仍可恢复，但 emptyReason 提示）
  const sealed = stage === "SECOND_PHASE_ERP_OWNED"
  const effectiveView =
    sealed && input.view !== "history" && input.view !== "snapshots"
      ? input.view
      : input.view

  const allJobs = [...sessionManualJobs, ...MALL_SYNC_JOBS]
  let jobs = allJobs.map((job) => {
    const blockers = [...job.actionBlockers]
    let actions = [...job.allowedActions]
    if (stage !== "FIRST_PHASE_MALL_OWNED") {
      actions = actions.filter((a) => a !== "RETRY_FAILED_JOB")
      blockers.push({
        action: "RETRY_FAILED_JOB",
        code: "STAGE_NOT_FIRST_PHASE",
        message: "按单补拉与普通失败重试仅在第一阶段可用；已封存后无第一期写动作",
      })
    }
    if (role !== "admin") {
      actions = actions.filter((a) => a !== "RETRY_FAILED_JOB")
    }
    return { ...job, allowedActions: actions, actionBlockers: blockers }
  })
  if (input.q) {
    const q = input.q.trim().toUpperCase()
    jobs = jobs.filter((j) => j.jobNo.toUpperCase().includes(q))
  }

  // 快照：业务按映射任务范围过滤；管理员全量（演示）
  let snapshots = [...MALL_SNAPSHOTS]
  if (role !== "admin") {
    const allowedSnapIds = new Set(
      MALL_MAPPING_TASKS.filter(
        (t) =>
          t.ownerRoutingState === "CONFIGURED" &&
          roleOwnsMapping(role, t.ownerRole)
      ).map((t) => t.sourceSnapshotId)
    )
    snapshots = snapshots.filter((s) => allowedSnapIds.has(s.snapshotId))
  }

  if (input.q) {
    const q = input.q.trim().toUpperCase()
    snapshots = snapshots.filter(
      (s) =>
        s.externalOrderNo.toUpperCase().includes(q) ||
        s.syncJobNo.toUpperCase().includes(q) ||
        (s.appliedSalesOrderNo?.toUpperCase().includes(q) ?? false)
    )
  }

  const mappingTasks = MALL_MAPPING_TASKS.map((t) => filterMappingForRole(t, role)).filter(
    (t): t is MappingTaskView => t != null
  )

  let filteredMapping = mappingTasks
  if (input.mappingType) {
    filteredMapping = filteredMapping.filter(
      (t) => t.mappingType === input.mappingType
    )
  }
  if (input.q) {
    const q = input.q.trim().toUpperCase()
    filteredMapping = filteredMapping.filter((t) =>
      t.externalOrderNo.toUpperCase().includes(q)
    )
  }

  const context = {
    sourceSystem: { ...MALL_SOURCE },
    manualGovernancePolicy: policyConfigured
      ? {
          state: "CONFIGURED" as const,
          policyVersion: "pol_v1",
          executionMode: "SINGLE_OPERATOR_REASON" as const,
        }
      : {
          state: "MISSING" as const,
          blockerCode: "MANUAL_GOVERNANCE_POLICY_MISSING" as const,
        },
    ownership,
    freshness: {
      currentWatermark: sessionSourceUnavailable
        ? "2026-08-01T08:00:00+08:00"
        : "2026-08-01T09:00:00+08:00",
      latestSuccessfulJobAt: "2026-08-01T08:02:41+08:00",
      sourceSafeTime: sessionSourceUnavailable
        ? "2026-08-01T08:00:00+08:00"
        : "2026-08-01T09:00:00+08:00",
      syncLagSeconds: sessionSourceUnavailable ? undefined : 240,
      viewProjectedAt: new Date().toISOString(),
    },
    metrics: metricsForRole(role, stage, mappingTasks),
    sourceUnavailable: sessionSourceUnavailable,
    sourceUnavailableMessage: sessionSourceUnavailable
      ? "商城继续运行，ERP 同步进度未推进。最近成功同步时间 08-01 08:00；恢复后按原同步进度补齐。"
      : undefined,
    viewerRole: role,
    viewerRoleLabel: DEMO_ROLE_LABEL[role],
    hasSourceScope: true,
    scheduledIncrementalNote:
      "系统定时增量按调度契约独立运行，不依赖人工治理策略；策略缺失不会停摆定时同步。",
  }

  // 选中对象恢复
  const selectedJob =
    jobs.find((j) => j.jobId === input.jobId) ??
    (effectiveView === "jobs" ? jobs[0] : undefined)

  const selectedSnapshot =
    snapshots.find((s) => s.snapshotId === input.snapshotId) ??
    (effectiveView === "snapshots" ? snapshots[0] : undefined)

  let selectedMappingTask =
    filteredMapping.find((t) => t.mappingTaskId === input.mappingTaskId) ??
    filteredMapping.find(
      (t) =>
        t.ownerRoutingState === "CONFIGURED" &&
        t.workItem.workItemId === input.workItemId
    ) ??
    (effectiveView === "mapping" ? filteredMapping[0] : undefined)

  // work-item deep link from queue
  if (input.workItemId && !input.mappingTaskId) {
    const byWi = filteredMapping.find(
      (t) =>
        t.ownerRoutingState === "CONFIGURED" &&
        t.workItem.workItemId === input.workItemId
    )
    if (byWi) selectedMappingTask = byWi
  }

  const recon =
    role === "admin" || role === "finance" || role === "sales"
      ? MALL_RECONCILIATION
      : role === "operations"
        ? {
            ...MALL_RECONCILIATION,
            differences: MALL_RECONCILIATION.differences.filter((d) =>
              filteredMapping.some((m) => m.externalOrderNo === d.externalOrderNo)
            ),
          }
        : null

  const selectedDifference = recon?.differences.find(
    (d) => d.differenceId === input.differenceId
  )

  let emptyReason: MallSyncPageView["emptyReason"]
  if (sealed && effectiveView !== "history") {
    emptyReason = "SEALED_HISTORY"
  } else if (
    effectiveView === "mapping" &&
    filteredMapping.length === 0
  ) {
    emptyReason = mappingTasks.length === 0 ? "NO_TASKS" : "FILTER_NO_RESULT"
  }

  return {
    context,
    jobs: role === "admin" ? jobs : jobs.filter((j) => j.status !== "RUNNING"),
    snapshots,
    mappingTasks: filteredMapping,
    reconciliation: recon,
    history: MALL_HISTORY,
    selectedJob,
    selectedSnapshot,
    selectedMappingTask,
    selectedDifference,
    emptyReason,
  }
}

function assertManualGovernance(policyConfigured: boolean): string | null {
  if (!policyConfigured) {
    return "MANUAL_GOVERNANCE_POLICY_MISSING"
  }
  return null
}

export async function triggerManualIncremental(input: {
  reason: string
  demoRole: DemoRole
  policyConfigured?: boolean
  stage?: OwnershipStage
}): Promise<TriggerMallSyncResult> {
  await mockDelay(120)
  const stage = input.stage ?? sessionStage
  const policy =
    input.policyConfigured ?? sessionPolicyConfigured

  if (input.demoRole !== "admin") {
    return {
      status: "failed",
      code: "FORBIDDEN",
      message: "仅系统管理员可触发立即增量",
    }
  }
  if (stage !== "FIRST_PHASE_MALL_OWNED") {
    return {
      status: "failed",
      code: "STAGE_NOT_FIRST_PHASE",
      message: "立即增量仅在第一阶段可用；已封存后无第一期写动作",
    }
  }
  const missing = assertManualGovernance(policy)
  if (missing) {
    return {
      status: "failed",
      code: missing,
      message:
        "人工治理策略未配置（MANUAL_GOVERNANCE_POLICY_MISSING）。立即增量已禁用；定时增量不受影响。",
    }
  }
  if (!input.reason.trim() || input.reason.trim().length < 4) {
    return {
      status: "failed",
      code: "REASON_REQUIRED",
      message: "单人理由模式下请填写至少 4 个字的触发理由",
    }
  }

  const jobNo = `SYNC-MAN-${Date.now().toString().slice(-6)}`
  const jobId = `job_man_${Date.now()}`
  sessionManualJobs = [
    {
      jobId,
      jobNo,
      jobType: "INCREMENTAL",
      jobTypeLabel: JOB_TYPE_LABEL.INCREMENTAL,
      rangeStart: "（由系统按同步进度计算）",
      rangeEnd: "（系统当前时间）",
      status: "RUNNING",
      statusLabel: "运行中",
      statusTone: "info",
      pageCount: 0,
      itemCount: 0,
      errorCount: 0,
      startedAt: new Date().toISOString(),
      triggeredBy: `管理员 · 人工 · ${input.reason.trim().slice(0, 20)}`,
      watermarkAdvanced: false,
      allowedActions: [],
      actionBlockers: [],
    },
    ...sessionManualJobs,
  ]

  return {
    status: "succeeded",
    jobId,
    jobNo,
    message: `已创建增量任务 ${jobNo}。范围由系统按同步点生成，页面不可改写同步进度。`,
  }
}

export async function triggerSingleOrderPull(input: {
  externalOrderNo: string
  reason: string
  demoRole: DemoRole
  policyConfigured?: boolean
  stage?: OwnershipStage
}): Promise<TriggerMallSyncResult> {
  await mockDelay(120)
  const stage = input.stage ?? sessionStage
  const policy = input.policyConfigured ?? sessionPolicyConfigured

  if (input.demoRole !== "admin") {
    return {
      status: "failed",
      code: "FORBIDDEN",
      message: "仅系统管理员可按单补拉",
    }
  }
  if (stage !== "FIRST_PHASE_MALL_OWNED") {
    return {
      status: "failed",
      code: "STAGE_NOT_FIRST_PHASE",
      message: "按单补拉仅在第一阶段（商城开单）可用；已封存后无第一期写动作",
    }
  }
  const missing = assertManualGovernance(policy)
  if (missing) {
    return {
      status: "failed",
      code: missing,
      message:
        "人工治理策略未配置（MANUAL_GOVERNANCE_POLICY_MISSING）。按单补拉已禁用。",
    }
  }
  if (!input.externalOrderNo.trim()) {
    return {
      status: "failed",
      code: "ORDER_NO_REQUIRED",
      message: "请填写有效来源单号",
    }
  }

  const jobNo = `SYNC-SO-${Date.now().toString().slice(-6)}`
  const jobId = `job_so_${Date.now()}`
  sessionManualJobs = [
    {
      jobId,
      jobNo,
      jobType: "SINGLE_ORDER",
      jobTypeLabel: JOB_TYPE_LABEL.SINGLE_ORDER,
      status: "RUNNING",
      statusLabel: "运行中",
      statusTone: "info",
      pageCount: 1,
      itemCount: 0,
      errorCount: 0,
      startedAt: new Date().toISOString(),
      triggeredBy: `管理员 · 补拉 ${input.externalOrderNo}`,
      watermarkAdvanced: false,
      allowedActions: [],
      actionBlockers: [],
      impactSummary: `沿原来源身份补拉 ${input.externalOrderNo}`,
    },
    ...sessionManualJobs,
  ]

  return {
    status: "succeeded",
    jobId,
    jobNo,
    message: `已创建按单补拉 ${jobNo}，使用原来源身份，不新建销售单。`,
  }
}

export async function retryFailedJob(input: {
  jobId: string
  reason: string
  demoRole: DemoRole
  stage?: OwnershipStage
}): Promise<TriggerMallSyncResult> {
  await mockDelay(100)
  const stage = input.stage ?? sessionStage
  if (input.demoRole !== "admin") {
    return { status: "failed", code: "FORBIDDEN", message: "仅管理员可重试" }
  }
  if (stage !== "FIRST_PHASE_MALL_OWNED") {
    return {
      status: "failed",
      code: "STAGE_NOT_FIRST_PHASE",
      message: "普通失败重试仅第一阶段可用；已封存后无第一期写动作",
    }
  }
  const job = [...sessionManualJobs, ...MALL_SYNC_JOBS].find(
    (j) => j.jobId === input.jobId
  )
  if (!job) {
    return { status: "failed", code: "NOT_FOUND", message: "任务不存在" }
  }
  if (job.status !== "FAILED" && job.status !== "PARTIAL_FAILED") {
    return {
      status: "failed",
      code: "NOT_RETRYABLE",
      message: "仅失败/部分失败任务可重试；禁止手工标记成功或推进同步进度",
    }
  }

  return {
    status: "succeeded",
    jobId: `retry_${job.jobId}`,
    jobNo: `${job.jobNo}-R1`,
    message: `已关联原任务发起重试。原范围与同步规则不变；不回退已捕获的同步进度。`,
  }
}

export async function claimMappingWorkItem(input: {
  workItemId: string
  subjectVersion: string
}): Promise<{ workItemId: string; subjectVersion: string }> {
  await mockDelay(60)
  const lease = claimWorkItemSession({
    workItemId: input.workItemId,
    subjectVersion: input.subjectVersion,
    ownerUserId: "user_mall_map",
  })
  return {
    workItemId: input.workItemId,
    subjectVersion: lease.subjectVersion ?? input.subjectVersion,
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
  await mockDelay(140)

  if (input.demoRole === "admin") {
    return {
      status: "failed",
      code: "ADMIN_CANNOT_CONFIRM_MAPPING",
      message: "系统管理员不能替业务角色确认映射",
    }
  }

  if (input.stage === "SECOND_PHASE_ERP_OWNED") {
    return {
      status: "failed",
      code: "STAGE_SEALED",
      message: "第一期已封存，映射确认不可用；请进入历史只读查看。",
    }
  }

  const seed = MALL_MAPPING_TASKS.find(
    (t) => t.mappingTaskId === input.mappingTaskId
  )
  if (!seed) {
    return { status: "failed", code: "NOT_FOUND", message: "映射任务不存在" }
  }
  if (seed.ownerRoutingState === "MISSING") {
    return {
      status: "failed",
      code: "OWNER_ROUTING_MISSING",
      message: "责任路由未配置，确认动作整体拒绝",
    }
  }
  if (!roleOwnsMapping(input.demoRole, seed.ownerRole)) {
    return {
      status: "failed",
      code: "OWNER_ROLE_MISMATCH",
      message: `当前角色无法确认 ${OWNER_ROLE_LABEL[seed.ownerRole]} 责任的映射`,
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

  try {
    completeWorkItemSession({
      workItemId: input.workItemId,
      expectedSubjectVersion: input.expectedSubjectVersion,
      decision: {
        kind: "CONFIRM_MAPPING",
        summary: `确认 ${input.targetLabel}`,
      },
    })
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      return { status: "failed", code: error.code, message: error.message }
    }
    throw error
  }

  const recordedAt = new Date().toISOString()
  const externalIdentityMapId = `eim_${input.mappingTaskId}`
  const mappingTargetId = `mtgt_${input.targetObjectId}`
  const result: ConfirmMappingResult = {
    status: "succeeded",
    mappingTaskId: input.mappingTaskId,
    mappingTaskStatus: "RESOLVED",
    externalIdentityMapId,
    mappingTargetId,
    recordedAt,
    message:
      "映射已解决并完成待办任务。尚未形成销售版本；请使用原数据重新归集。",
  }

  resolvedMappings.set(input.mappingTaskId, {
    mappingTaskStatus: "RESOLVED",
    targetId: input.targetObjectId,
    targetLabel: input.targetLabel,
    externalIdentityMapId,
    recordedAt,
    evidenceNote: input.evidenceNote,
  })
  markQueueTaskCompleted(WORKSPACE_ID, input.workItemId)

  return result
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
  await mockDelay(90)
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
  try {
    applyWorkItemActionSession({
      workItemId: input.workItemId,
      expectedSubjectVersion: input.expectedSubjectVersion,
      action: {
        kind: "DEFER",
        note: `${input.reasonCode}${input.note ? `: ${input.note}` : ""}`,
      },
    })
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      return { status: "failed", code: error.code, message: error.message }
    }
    throw error
  }

  deferredNotes.set(input.mappingTaskId, {
    reasonCode: input.reasonCode,
    note: input.note,
    at: new Date().toISOString(),
  })
  markQueueTaskHeld(WORKSPACE_ID, input.workItemId)

  return {
    status: "succeeded",
    mappingTaskId: input.mappingTaskId,
    mappingTaskStatus: "PENDING",
    leaseDisposition: "RELEASED",
    message:
      "已记录跳过原因。映射任务仍为待处理，不会暂停或完成任务。",
  }
}

export async function reapplyMallSnapshot(input: {
  mappingTaskId: string
  sourceSnapshotId: string
  demoRole: DemoRole
  stage?: OwnershipStage
}): Promise<ReapplyResult> {
  await mockDelay(150)
  if (input.demoRole === "admin") {
    return {
      status: "failed",
      code: "FORBIDDEN",
      message: "重新归集由业务责任人在映射解决后发起",
    }
  }
  if (input.stage === "SECOND_PHASE_ERP_OWNED") {
    return {
      status: "failed",
      code: "STAGE_SEALED",
      message: "第一期已封存，重新归集不可用；请进入历史只读查看。",
    }
  }

  const resolved =
    resolvedMappings.get(input.mappingTaskId) ||
    MALL_MAPPING_TASKS.find(
      (t) =>
        t.mappingTaskId === input.mappingTaskId &&
        t.mappingTaskStatus === "RESOLVED"
    )
  if (!resolved) {
    return {
      status: "failed",
      code: "MAPPING_NOT_RESOLVED",
      message: "仅映射任务已解决时可重新归集",
    }
  }

  const operationId = `reapply_${input.mappingTaskId}_${Date.now().toString().slice(-4)}`

  const salesOrderId = `so_reapply_${input.mappingTaskId}`
  const salesOrderNo = `SO-R-${input.mappingTaskId.slice(-4).toUpperCase()}`
  const salesOrderRevisionId = `${salesOrderId}_v1`
  const receivableResultReference = `AR-REF-${salesOrderNo}`

  reapplyState.set(input.mappingTaskId, {
    operationId,
    status: "SUCCEEDED",
    lastUpdatedAt: new Date().toISOString(),
    salesOrderId,
    salesOrderNo,
    salesOrderRevisionId,
    receivableResultReference,
  })

  return {
    status: "succeeded",
    operationId,
    reapplyOperationStatus: "SUCCEEDED",
    salesOrderId,
    salesOrderNo,
    salesOrderRevisionId,
    receivableResultReference,
    message: `已用原数据与原来源身份形成 ${salesOrderNo}，未创建重复销售单。`,
  }
}

export async function resolveUnknownReapply(input: {
  mappingTaskId: string
  operationId: string
  settle?: boolean
}): Promise<ReapplyResult> {
  await mockDelay(80)
  const current = reapplyState.get(input.mappingTaskId)
  if (current?.status === "SUCCEEDED" && current.salesOrderId) {
    return {
      status: "succeeded",
      operationId: current.operationId,
      reapplyOperationStatus: "SUCCEEDED",
      salesOrderId: current.salesOrderId,
      salesOrderNo: current.salesOrderNo!,
      salesOrderRevisionId: current.salesOrderRevisionId!,
      receivableResultReference: current.receivableResultReference,
      message: "重新归集已确认成功",
    }
  }
  if (current?.status === "UNKNOWN" && input.settle) {
    const salesOrderId = `so_reapply_${input.mappingTaskId}`
    const salesOrderNo = `SO-R-${input.mappingTaskId.slice(-4).toUpperCase()}`
    const salesOrderRevisionId = `${salesOrderId}_v1`
    const receivableResultReference = `AR-REF-${salesOrderNo}`
    reapplyState.set(input.mappingTaskId, {
      operationId: input.operationId,
      status: "SUCCEEDED",
      lastUpdatedAt: new Date().toISOString(),
      salesOrderId,
      salesOrderNo,
      salesOrderRevisionId,
      receivableResultReference,
    })
    return {
      status: "succeeded",
      operationId: input.operationId,
      reapplyOperationStatus: "SUCCEEDED",
      salesOrderId,
      salesOrderNo,
      salesOrderRevisionId,
      receivableResultReference,
      message: "查询后确认重新归集成功；映射结论此前未回滚。",
    }
  }
  if (current?.status === "UNKNOWN") {
    return {
      status: "unknown",
      reapplyOperationStatus: "UNKNOWN",
      operationId: input.operationId,
      message: "仍在处理中，结果未知。停留当前项。",
      idempotencyKey: input.operationId,
    }
  }
  return {
    status: "failed",
    code: "NO_PENDING",
    message: "未找到该重新归集操作",
  }
}

export async function assignMappingTask(input: {
  mappingTaskId: string
  targetOwnerRole: "SALES" | "OPERATIONS" | "FINANCE"
  reason: string
  demoRole: DemoRole
}): Promise<{ status: "succeeded" | "failed"; message: string; code?: string }> {
  await mockDelay(80)
  if (input.demoRole !== "admin") {
    return {
      status: "failed",
      code: "FORBIDDEN",
      message: "仅管理员可指派映射任务",
    }
  }
  const seed = MALL_MAPPING_TASKS.find(
    (t) => t.mappingTaskId === input.mappingTaskId
  )
  if (!seed) {
    return { status: "failed", code: "NOT_FOUND", message: "任务不存在" }
  }
  if (seed.ownerRoutingState === "MISSING") {
    return {
      status: "failed",
      code: "OWNER_ROUTING_MISSING",
      message: "责任路由未配置时不能指派可执行确认待办",
    }
  }
  return {
    status: "succeeded",
    message: `已追加指派审计至 ${OWNER_ROLE_LABEL[input.targetOwnerRole]}。管理员不能代确认映射。`,
  }
}

export function getMallSyncStageLabels() {
  return { STAGE_LABEL, DIRECTION_LABEL }
}
