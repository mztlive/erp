/**
 * W30 session-mock API：queryFn / mutationFn 纯函数。
 * - processingStatus 与 reportReviewStatus 独立
 * - rangeStart 固定 = requiredHistoryStart；覆盖缺口阻断 START
 * - RESUME 复用原 job/范围/原任务标识
 * - 禁止重叠业务批次；报告策略缺失时确认 fail-closed
 * - mock 永不返回卡号/卡密/手机/完整地址/原始报文
 */

import { mockDelay } from "@/lib/mock-delay"
import type {
  CreateBackfillContext,
  FormalCommandAction,
  HistoryBackfillCommandInput,
  HistoryBackfillCommandResult,
  HistoryBackfillDetailQuery,
  HistoryBackfillDetailView,
  HistoryBackfillItemView,
  HistoryBackfillJobCore,
  HistoryBackfillListItem,
  HistoryBackfillListQuery,
  HistoryBackfillListView,
  HistoryBackfillProcessingStatus,
  ViewerRoleDemo,
} from "@/features/history-backfill/types"
import {
  ACTIVE_PROCESSING,
  PROCESSING_STATUS_LABEL,
} from "@/features/history-backfill/types"
import {
  CREATE_CONTEXT_GAP,
  CREATE_CONTEXT_SEED,
  JOB_SEEDS,
  ITEM_SEEDS,
  buildReportForJob,
} from "@/mock/history-backfill"
import {
  getIdempotencyEntry,
  queryIdempotencyResult,
  setIdempotencySucceeded,
} from "@/mock/session-state"

const IDEM_KIND = "W30_HISTORY_BACKFILL"

type JobOverlay = Partial<
  Pick<
    HistoryBackfillJobCore,
    | "processingStatus"
    | "reportReviewStatus"
    | "pipelineStage"
    | "lockVersion"
    | "progress"
    | "coverageComplete"
    | "coverageGaps"
    | "sourceCoverageStart"
    | "allowedActions"
    | "actionBlockers"
    | "formalDownstreamUnlocked"
    | "costBasis"
    | "coverageRate"
    | "coveragePercent"
  >
>

const jobOverlays = new Map<string, JobOverlay>()
const sessionJobs: HistoryBackfillJobCore[] = []
let createContextMode: "ok" | "gap" = "ok"
let forceUnknownNext = false

function nowIso() {
  return new Date().toISOString()
}

function allJobs(): HistoryBackfillJobCore[] {
  const byId = new Map<string, HistoryBackfillJobCore>()
  for (const seed of JOB_SEEDS) {
    byId.set(seed.id, projectJob(seed))
  }
  for (const created of sessionJobs) {
    byId.set(created.id, projectJob(created))
  }
  return Array.from(byId.values())
}

function projectJob(seed: HistoryBackfillJobCore): HistoryBackfillJobCore {
  const overlay = jobOverlays.get(seed.id)
  const merged: HistoryBackfillJobCore = {
    ...seed,
    ...overlay,
    progress: overlay?.progress ?? seed.progress,
    coverageGaps: overlay?.coverageGaps ?? seed.coverageGaps,
    costBasis: overlay?.costBasis ?? seed.costBasis,
    allowedActions: overlay?.allowedActions ?? seed.allowedActions,
    actionBlockers: overlay?.actionBlockers ?? seed.actionBlockers,
  }

  // 策略缺失 + COMPLETED → 强制未解锁
  if (
    merged.processingStatus === "COMPLETED" &&
    (merged.reportReviewStatus === "POLICY_NOT_CONFIGURED" ||
      merged.reportReviewStatus === "NOT_READY" ||
      merged.reportReviewStatus === "PENDING" ||
      merged.reportReviewStatus === "REJECTED")
  ) {
    merged.formalDownstreamUnlocked = false
  }
  if (merged.reportReviewStatus === "CONFIRMED" && merged.coverageComplete) {
    merged.formalDownstreamUnlocked = true
  }

  return merged
}

function formatRange(start: string, end: string) {
  const s = start.slice(0, 10)
  const e = end.slice(0, 10)
  return `${s} 至 ${e}`
}

function toListItem(job: HistoryBackfillJobCore): HistoryBackfillListItem {
  const p = job.progress
  const none = job.costBasis.find((c) => c.basis === "NONE")
  const costCoverageLabel =
    job.coverageRate == null
      ? "—"
      : `${job.coverageRate}${none && none.count > 0 ? ` · NONE ${none.count}` : ""}`

  return {
    id: job.id,
    jobNo: job.jobNo,
    mallId: job.mallId,
    mallName: job.mallName,
    environment: job.environment,
    rangeStart: job.rangeStart,
    rangeEnd: job.rangeEnd,
    rangeLabel: formatRange(job.rangeStart, job.rangeEnd),
    processingStatus: job.processingStatus,
    reportReviewStatus: job.reportReviewStatus,
    progressLabel:
      p.totalCount > 0
        ? `${p.processedCount.toLocaleString("zh-CN")} / ${p.totalCount.toLocaleString("zh-CN")}`
        : PROCESSING_STATUS_LABEL[job.processingStatus],
    processedCount: p.processedCount,
    totalCount: p.totalCount,
    deduplicatedCount: p.deduplicatedCount,
    unattributedCount: p.unattributedCount,
    costCoverageLabel,
    coverageComplete: job.coverageComplete,
    lastProgressAt: p.lastProgressAt,
  }
}

function getCreateContext(): CreateBackfillContext {
  const base =
    createContextMode === "gap" ? CREATE_CONTEXT_GAP : CREATE_CONTEXT_SEED
  const overlapping = allJobs().find(
    (j) =>
      j.mallId === base.mallId &&
      j.environment === base.environment &&
      j.rangeStart === base.requiredHistoryStart &&
      j.rangeEnd === base.rangeEnd &&
      j.processingStatus !== "DRAFT" &&
      ACTIVE_PROCESSING.includes(j.processingStatus)
  )
  if (overlapping) {
    return {
      ...base,
      hasOverlappingFormalJob: true,
      overlappingJobNo: overlapping.jobNo,
      canCreateDraft: false,
      blockReasons: [
        ...base.blockReasons,
        `已存在回填任务 ${overlapping.jobNo} 覆盖同一范围起点至截止时点的批次，禁止新建重叠批次；请续跑原任务。`,
      ],
    }
  }
  return { ...base }
}

export function setHistoryBackfillCreateContextMode(mode: "ok" | "gap") {
  createContextMode = mode
}

export function setHistoryBackfillForceUnknown(next: boolean) {
  forceUnknownNext = next
}

function roleAllowsModule(role: ViewerRoleDemo) {
  return role !== "NO_MODULE"
}

function roleCanFormal(role: ViewerRoleDemo) {
  return role === "SYSTEM_ADMIN"
}

function roleCanConfirmReport(role: ViewerRoleDemo, job: HistoryBackfillJobCore) {
  if (!job.reportReviewPolicy) return false
  return role === "FINANCE" || role === "SYSTEM_ADMIN"
}

export async function fetchHistoryBackfillList(
  query: HistoryBackfillListQuery
): Promise<HistoryBackfillListView> {
  await mockDelay()
  const role = query.role ?? "SYSTEM_ADMIN"
  if (!roleAllowsModule(role)) {
    return {
      metrics: {
        running: 0,
        unattributed: 0,
        deduplicated: 0,
        noneConsumption: 0,
        failed: 0,
      },
      rows: [],
      totalCount: 0,
      queriedAt: nowIso(),
      createContext: getCreateContext(),
    }
  }

  let rows = allJobs()

  if (query.environment) {
    rows = rows.filter((j) => j.environment === query.environment)
  }
  if (query.mallId) {
    rows = rows.filter((j) => j.mallId === query.mallId)
  }
  if (query.processingStatus) {
    rows = rows.filter((j) => j.processingStatus === query.processingStatus)
  }
  if (query.reportReviewStatus) {
    rows = rows.filter((j) => j.reportReviewStatus === query.reportReviewStatus)
  }
  if (query.basis) {
    const basis = query.basis
    rows = rows.filter((j) =>
      j.costBasis.some((c) => c.basis === basis && c.count > 0)
    )
  }
  if (query.q?.trim()) {
    const q = query.q.trim().toLowerCase()
    rows = rows.filter(
      (j) =>
        j.jobNo.toLowerCase().includes(q) ||
        j.mallName.toLowerCase().includes(q) ||
        j.id.toLowerCase().includes(q)
    )
  }

  if (query.view === "active") {
    rows = rows.filter((j) => ACTIVE_PROCESSING.includes(j.processingStatus))
  } else if (query.view === "processing_completed") {
    rows = rows.filter((j) => j.processingStatus === "COMPLETED")
  } else if (query.view === "report_pending") {
    rows = rows.filter(
      (j) =>
        j.reportReviewStatus === "PENDING" ||
        j.reportReviewStatus === "POLICY_NOT_CONFIGURED"
    )
  }

  // 运行中优先
  const rank = (s: HistoryBackfillProcessingStatus) => {
    const order: HistoryBackfillProcessingStatus[] = [
      "RUNNING",
      "PARTIAL",
      "FAILED",
      "VALIDATING",
      "READY",
      "DRAFT",
      "COMPLETED",
    ]
    return order.indexOf(s)
  }
  rows = [...rows].sort((a, b) => rank(a.processingStatus) - rank(b.processingStatus))

  const all = allJobs()
  const metrics = {
    running: all.filter((j) => j.processingStatus === "RUNNING").length,
    unattributed: all.reduce((n, j) => n + j.progress.unattributedCount, 0),
    deduplicated: all.reduce((n, j) => n + j.progress.deduplicatedCount, 0),
    noneConsumption: all.reduce(
      (n, j) => n + (j.costBasis.find((c) => c.basis === "NONE")?.count ?? 0),
      0
    ),
    failed: all.reduce((n, j) => n + j.progress.failedCount, 0),
  }

  const start = (query.page - 1) * query.pageSize
  const pageRows = rows.slice(start, start + query.pageSize)

  return {
    metrics,
    rows: pageRows.map(toListItem),
    totalCount: rows.length,
    queriedAt: nowIso(),
    createContext: getCreateContext(),
  }
}

export async function fetchHistoryBackfillDetail(
  query: HistoryBackfillDetailQuery
): Promise<HistoryBackfillDetailView | null> {
  await mockDelay()
  const role = query.role ?? "SYSTEM_ADMIN"
  if (!roleAllowsModule(role)) return null

  const job = allJobs().find((j) => j.id === query.jobId)
  if (!job) return null

  let items: HistoryBackfillItemView[] = ITEM_SEEDS.filter(
    (i) => i.jobId === query.jobId
  )

  // 会话新建任务可能无明细
  if (items.length === 0 && sessionJobs.some((j) => j.id === query.jobId)) {
    items = []
  }

  if (query.results?.length) {
    const set = new Set(query.results)
    items = items.filter((i) => set.has(i.result))
  }
  if (query.factTypes?.length) {
    const set = new Set(query.factTypes)
    items = items.filter((i) => set.has(i.factType))
  }
  if (query.costBases?.length) {
    const set = new Set(query.costBases)
    items = items.filter(
      (i) => i.costBasis && i.costBasis !== "N_A" && set.has(i.costBasis)
    )
  }
  if (query.q?.trim()) {
    const q = query.q.trim().toLowerCase()
    items = items.filter(
      (i) =>
        i.mallOrderNo.toLowerCase().includes(q) ||
        i.businessFactKeySummary.toLowerCase().includes(q) ||
        (i.sourceDocNo?.toLowerCase().includes(q) ?? false)
    )
  }

  const start = (query.page - 1) * query.pageSize
  const pageItems = items.slice(start, start + query.pageSize)

  // 按角色裁剪确认动作
  const allowedActions = [...job.allowedActions]
  const actionBlockers = [...job.actionBlockers]
  if (
    job.processingStatus === "COMPLETED" &&
    job.reportReviewStatus === "POLICY_NOT_CONFIGURED"
  ) {
    if (!actionBlockers.some((b) => b.action === "CONFIRM_REPORT")) {
      actionBlockers.push({
        action: "CONFIRM_REPORT",
        code: "REPORT_REVIEW_POLICY_MISSING",
        message:
          "报告复核策略未配置：系统不返回确认动作；技术报告可下载但固定标「未确认」。",
      })
    }
  }
  if (
    job.reportReviewPolicy &&
    job.processingStatus === "COMPLETED" &&
    job.reportReviewStatus === "PENDING" &&
    roleCanConfirmReport(role, job)
  ) {
    if (!allowedActions.includes("CONFIRM_REPORT")) {
      allowedActions.push("CONFIRM_REPORT")
    }
  }
  if (!roleCanFormal(role)) {
    for (const a of ["START", "RESUME", "CREATE_DRAFT", "VALIDATE_SOURCE"] as FormalCommandAction[]) {
      const idx = allowedActions.indexOf(a)
      if (idx >= 0) allowedActions.splice(idx, 1)
      if (!actionBlockers.some((b) => b.action === a && b.code === "ROLE_DENIED")) {
        actionBlockers.push({
          action: a,
          code: "ROLE_DENIED",
          message: "当前演示角色不能执行回填/续跑（仅系统管理员）。",
        })
      }
    }
  }

  return {
    job: {
      ...job,
      allowedActions,
      actionBlockers,
    },
    items: pageItems,
    report: buildReportForJob(job),
    queriedAt: nowIso(),
    permissionVersion: "pv-w30-1",
  }
}

export async function submitHistoryBackfillCommand(
  input: HistoryBackfillCommandInput
): Promise<HistoryBackfillCommandResult> {
  await mockDelay()
  const role = input.role ?? "SYSTEM_ADMIN"
  const { action, operationId, idempotencyKey } = input

  const existing = getIdempotencyEntry(idempotencyKey)
  if (existing?.state === "succeeded" && existing.payload) {
    return existing.payload as HistoryBackfillCommandResult
  }

  if (forceUnknownNext) {
    forceUnknownNext = false
    return {
      status: "RESULT_UNKNOWN",
      title: "结果未知 · 请查询原操作",
      description:
        "提交超时或响应丢失。请按 operationId 查询，禁止新建第二任务。",
      operationId,
      idempotencyKey,
      jobId: input.jobId,
      nextStep: "使用相同 idempotencyKey 查询最终结果",
    }
  }

  if (action === "CREATE_DRAFT") {
    if (!roleCanFormal(role)) {
      return {
        status: "BLOCKED",
        title: "无权限创建回填任务",
        description: "仅系统管理员可创建草稿并启动回填。",
        operationId,
        idempotencyKey,
        blockers: ["ROLE_DENIED"],
      }
    }
    const ctx = getCreateContext()
    if (!ctx.canCreateDraft) {
      return {
        status: "BLOCKED",
        title: "无法创建回填任务",
        description: ctx.blockReasons.join("；") || "前置条件未满足",
        operationId,
        idempotencyKey,
        blockers: ctx.blockReasons,
      }
    }
    // rangeStart 必须等于 requiredHistoryStart
    const rangeStart = ctx.requiredHistoryStart
    if (input.rangeStart && input.rangeStart !== rangeStart) {
      return {
        status: "BLOCKED",
        title: "范围起点非法",
        description:
          "不能选择更晚的起点；范围起点必须等于系统登记的「必须覆盖起点」。",
        operationId,
        idempotencyKey,
        blockers: ["RANGE_START_FIXED"],
      }
    }
    if (!ctx.coverageComplete) {
      return {
        status: "BLOCKED",
        title: "来源覆盖不足 · 阻断创建执行",
        description:
          "来源覆盖起点晚于 requiredHistoryStart 或存在区间缺口时不得创建可执行任务。",
        operationId,
        idempotencyKey,
        blockers: ["COVERAGE_INCOMPLETE"],
      }
    }

    const id = `hb_job_session_${Date.now().toString(36)}`
    const jobNo = `HB-SESS-${Date.now().toString(36).toUpperCase()}`
    const job: HistoryBackfillJobCore = {
      id,
      jobNo,
      mallId: ctx.mallId,
      mallName: ctx.mallName,
      environment: ctx.environment,
      cutoverId: ctx.cutoverId,
      requiredHistoryStart: ctx.requiredHistoryStart,
      rangeStart,
      rangeEnd: ctx.rangeEnd,
      cutoverAt: ctx.cutoverAt,
      sourceCoverageStart: ctx.sourceCoverageStart,
      coverageComplete: ctx.coverageComplete,
      coverageGaps: [],
      processingStatus: "READY",
      reportReviewStatus: "NOT_READY",
      pipelineStage: "SCOPE",
      formalDownstreamUnlocked: false,
      lockVersion: 1,
      requestedBy: "系统管理员 · 演示",
      requestedAt: nowIso(),
      sourceAsOf: nowIso(),
      fulfillmentNote: "历史记录追加写入，不覆盖实时记录",
      scopeNote:
        "生效范围从范围起点至截止时点（截止时点当天除外）；截止时点当天发生的记录不进入历史回填，按实时/补录规则处理。",
      legacyManualNote:
        "截止时点前支付只补台账，履约链固定为历史手工口径，不创建供应商订单。",
      progress: {
        totalCount: ctx.estimatedFactCount,
        processedCount: 0,
        insertedCount: 0,
        deduplicatedCount: 0,
        unattributedCount: 0,
        failedCount: 0,
      },
      costBasis: [
        { basis: "ACTUAL", count: 0, consumptionAmountGross: "¥0.00", costAmountNet: "¥0.00" },
        {
          basis: "STANDARD",
          count: 0,
          consumptionAmountGross: "¥0.00",
          costAmountNet: "¥0.00",
        },
        { basis: "NONE", count: 0, consumptionAmountGross: "¥0.00", costAmountNet: null },
      ],
      coverageRate: null,
      coveragePercent: 0,
      allowedActions: ["VALIDATE_SOURCE", "START"],
      actionBlockers: [],
      idempotencyNamespace: `mall-backfill:${jobNo}`,
    }
    sessionJobs.unshift(job)

    const result: HistoryBackfillCommandResult = {
      status: "COMMITTED",
      title: "已创建回填任务草稿",
      description: `任务 ${jobNo} 范围固定为 ${formatRange(rangeStart, ctx.rangeEnd)}；截止时点前不下单，只追加缺失记录。`,
      jobId: id,
      jobNo,
      operationId,
      idempotencyKey,
      nextStep: "完成来源校验后开始回填",
    }
    setIdempotencySucceeded(idempotencyKey, IDEM_KIND, result)
    return result
  }

  if (action === "VALIDATE_SOURCE") {
    if (!input.jobId) {
      return {
        status: "FAILED",
        title: "缺少任务 ID",
        description: "校验来源必须引用既有任务。",
        operationId,
        idempotencyKey,
      }
    }
    const job = allJobs().find((j) => j.id === input.jobId)
    if (!job) {
      return {
        status: "FAILED",
        title: "任务不存在",
        description: `未找到任务 ${input.jobId}`,
        operationId,
        idempotencyKey,
      }
    }
    if (!job.coverageComplete) {
      jobOverlays.set(job.id, {
        ...jobOverlays.get(job.id),
        processingStatus: "VALIDATING",
        pipelineStage: "VALIDATE_SOURCE",
        lockVersion: job.lockVersion + 1,
      })
      return {
        status: "BLOCKED",
        title: "来源校验未通过 · 覆盖不足",
        description: `requiredHistoryStart=${job.requiredHistoryStart.slice(0, 10)}，sourceCoverageStart=${(job.sourceCoverageStart ?? "—").slice(0, 10)}。禁止缩晚起点。`,
        jobId: job.id,
        jobNo: job.jobNo,
        operationId,
        idempotencyKey,
        blockers: job.coverageGaps.map((g) => g.reasonLabel),
      }
    }
    jobOverlays.set(job.id, {
      ...jobOverlays.get(job.id),
      processingStatus: "READY",
      pipelineStage: "VALIDATE_SOURCE",
      lockVersion: job.lockVersion + 1,
      allowedActions: ["START"],
      actionBlockers: [],
    })
    const result: HistoryBackfillCommandResult = {
      status: "COMMITTED",
      title: "来源校验通过",
      description: "五类记录在范围起点至截止时点间连续可取，可开始回填。",
      jobId: job.id,
      jobNo: job.jobNo,
      operationId,
      idempotencyKey,
      nextStep: "确认后开始回填（后台执行）",
    }
    setIdempotencySucceeded(idempotencyKey, IDEM_KIND, result)
    return result
  }

  if (action === "START") {
    if (!roleCanFormal(role)) {
      return {
        status: "BLOCKED",
        title: "无权限开始回填",
        description: "仅系统管理员可启动回填。",
        operationId,
        idempotencyKey,
        blockers: ["ROLE_DENIED"],
      }
    }
    if (!input.jobId) {
      return {
        status: "FAILED",
        title: "缺少任务 ID",
        description: "START 必须引用既有任务草稿。",
        operationId,
        idempotencyKey,
      }
    }
    const job = allJobs().find((j) => j.id === input.jobId)
    if (!job) {
      return {
        status: "FAILED",
        title: "任务不存在",
        description: `未找到任务 ${input.jobId}`,
        operationId,
        idempotencyKey,
      }
    }
    if (job.rangeStart !== job.requiredHistoryStart) {
      return {
        status: "BLOCKED",
        title: "范围非法",
        description: "范围起点必须等于必须覆盖起点。",
        operationId,
        idempotencyKey,
        jobId: job.id,
        jobNo: job.jobNo,
        blockers: ["RANGE_START_FIXED"],
      }
    }
    if (!job.coverageComplete) {
      return {
        status: "BLOCKED",
        title: "覆盖不足 · 禁止开始",
        description: "来源覆盖不完整时不得开始，也不能改晚范围起点。",
        operationId,
        idempotencyKey,
        jobId: job.id,
        jobNo: job.jobNo,
        blockers: ["COVERAGE_INCOMPLETE"],
      }
    }
    if (
      job.processingStatus === "RUNNING" ||
      job.processingStatus === "COMPLETED" ||
      job.processingStatus === "PARTIAL"
    ) {
      return {
        status: "BLOCKED",
        title: "禁止新建/重复启动重叠批次",
        description: `任务 ${job.jobNo} 已处于 ${PROCESSING_STATUS_LABEL[job.processingStatus]}；失败请续跑原任务。`,
        operationId,
        idempotencyKey,
        jobId: job.id,
        jobNo: job.jobNo,
        blockers: ["JOB_ALREADY_EXISTS"],
      }
    }

    jobOverlays.set(job.id, {
      ...jobOverlays.get(job.id),
      processingStatus: "RUNNING",
      pipelineStage: "INGEST",
      lockVersion: job.lockVersion + 1,
      progress: {
        ...job.progress,
        lastProgressAt: nowIso(),
        heartbeatAt: nowIso(),
      },
      allowedActions: [],
      actionBlockers: [
        {
          action: "START",
          code: "ALREADY_RUNNING",
          message: "任务已在后台运行。",
        },
      ],
    })

    const result: HistoryBackfillCommandResult = {
      status: "COMMITTED",
      title: "回填已提交后台",
      description: `任务 ${job.jobNo} 已冻结范围 ${formatRange(job.rangeStart, job.rangeEnd)} 并启动异步作业；进度以心跳与任务记录为准，不伪装同步完成。`,
      jobId: job.id,
      jobNo: job.jobNo,
      operationId,
      idempotencyKey,
      nextStep: "在任务详情查看处理进度",
    }
    setIdempotencySucceeded(idempotencyKey, IDEM_KIND, result)
    return result
  }

  if (action === "RESUME") {
    if (!roleCanFormal(role)) {
      return {
        status: "BLOCKED",
        title: "无权限续跑",
        description: "仅系统管理员可续跑失败/中断任务。",
        operationId,
        idempotencyKey,
        blockers: ["ROLE_DENIED"],
      }
    }
    if (!input.jobId) {
      return {
        status: "FAILED",
        title: "缺少任务 ID",
        description: "续跑必须引用原任务。",
        operationId,
        idempotencyKey,
      }
    }
    const job = allJobs().find((j) => j.id === input.jobId)
    if (!job) {
      return {
        status: "FAILED",
        title: "任务不存在",
        description: `未找到任务 ${input.jobId}`,
        operationId,
        idempotencyKey,
      }
    }
    if (job.processingStatus !== "PARTIAL" && job.processingStatus !== "FAILED") {
      return {
        status: "BLOCKED",
        title: "当前状态不可续跑",
        description: `仅部分完成或失败可续跑；当前为 ${PROCESSING_STATUS_LABEL[job.processingStatus]}。`,
        operationId,
        idempotencyKey,
        jobId: job.id,
        jobNo: job.jobNo,
      }
    }

    // 续跑：同一 job、范围、原任务标识
    jobOverlays.set(job.id, {
      ...jobOverlays.get(job.id),
      processingStatus: "RUNNING",
      pipelineStage: "INGEST",
      lockVersion: job.lockVersion + 1,
      progress: {
        ...job.progress,
        lastProgressAt: nowIso(),
        heartbeatAt: nowIso(),
      },
      allowedActions: [],
      actionBlockers: [
        {
          action: "RESUME",
          code: "ALREADY_RUNNING",
          message: "续跑已提交，沿原记录键处理剩余项。",
        },
      ],
    })

    const result: HistoryBackfillCommandResult = {
      status: "COMMITTED",
      title: "已续跑原任务",
      description: `沿 ${job.jobNo} 原范围 ${formatRange(job.rangeStart, job.rangeEnd)} 与提交身份续跑；已成功记录不回滚。`,
      jobId: job.id,
      jobNo: job.jobNo,
      operationId,
      idempotencyKey,
      nextStep: "查看进度与失败明细",
    }
    setIdempotencySucceeded(idempotencyKey, IDEM_KIND, result)
    return result
  }

  if (action === "REATTRIBUTE") {
    if (!input.jobId) {
      return {
        status: "FAILED",
        title: "缺少任务 ID",
        description: "重新归集必须引用原任务与原记录。",
        operationId,
        idempotencyKey,
      }
    }
    const job = allJobs().find((j) => j.id === input.jobId)
    if (!job) {
      return {
        status: "FAILED",
        title: "任务不存在",
        description: `未找到任务 ${input.jobId}`,
        operationId,
        idempotencyKey,
      }
    }
    const result: HistoryBackfillCommandResult = {
      status: "COMMITTED",
      title: "已提交重新归集",
      description:
        "引用原 mall_order_fact 重新归集并追加成本评估；不复制业务记录、不改写原消费。",
      jobId: job.id,
      jobNo: job.jobNo,
      operationId,
      idempotencyKey,
      nextStep: "归集完成后刷新未归集清单",
    }
    setIdempotencySucceeded(idempotencyKey, IDEM_KIND, result)
    return result
  }

  if (action === "CONFIRM_REPORT") {
    if (!input.jobId) {
      return {
        status: "FAILED",
        title: "缺少任务 ID",
        description: "报告确认必须引用任务与报告版本。",
        operationId,
        idempotencyKey,
      }
    }
    const job = allJobs().find((j) => j.id === input.jobId)
    if (!job) {
      return {
        status: "FAILED",
        title: "任务不存在",
        description: `未找到任务 ${input.jobId}`,
        operationId,
        idempotencyKey,
      }
    }
    if (job.processingStatus !== "COMPLETED") {
      return {
        status: "BLOCKED",
        title: "技术处理未完成",
        description: "仅处理完成后可进入报告确认。",
        operationId,
        idempotencyKey,
        jobId: job.id,
        jobNo: job.jobNo,
      }
    }
    if (
      job.reportReviewStatus === "POLICY_NOT_CONFIGURED" ||
      !job.reportReviewPolicy
    ) {
      return {
        status: "BLOCKED",
        title: "报告复核策略未配置 · 已阻断",
        description:
          "策略缺失时系统不返回确认动作；技术报告仅可下载并固定标「未确认」，不解锁下游。",
        operationId,
        idempotencyKey,
        jobId: job.id,
        jobNo: job.jobNo,
        blockers: ["REPORT_REVIEW_POLICY_MISSING"],
      }
    }
    if (!roleCanConfirmReport(role, job)) {
      return {
        status: "BLOCKED",
        title: "无报告确认权限",
        description: "当前角色不满足报告复核策略。",
        operationId,
        idempotencyKey,
        blockers: ["ROLE_DENIED"],
      }
    }

    jobOverlays.set(job.id, {
      ...jobOverlays.get(job.id),
      reportReviewStatus: "CONFIRMED",
      formalDownstreamUnlocked: job.coverageComplete,
      lockVersion: job.lockVersion + 1,
    })
    const result: HistoryBackfillCommandResult = {
      status: "COMMITTED",
      title: "报告已确认",
      description:
        "仅更新报告确认状态；不改写已入库记录或处理状态。",
      jobId: job.id,
      jobNo: job.jobNo,
      operationId,
      idempotencyKey,
      nextStep: job.coverageComplete
        ? "可在门禁通过后进入下游"
        : "覆盖仍不完整，下游功能保持关闭",
    }
    setIdempotencySucceeded(idempotencyKey, IDEM_KIND, result)
    return result
  }

  return {
    status: "FAILED",
    title: "未知动作",
    description: `不支持的 action: ${action}`,
    operationId,
    idempotencyKey,
  }
}

export async function queryHistoryBackfillIdempotency(input: {
  idempotencyKey: string
}): Promise<HistoryBackfillCommandResult | null> {
  await mockDelay(120)
  const hit = queryIdempotencyResult(input.idempotencyKey)
  if (!hit || hit.state !== "succeeded" || !hit.payload) return null
  return hit.payload as HistoryBackfillCommandResult
}

export function listMallOptions() {
  const map = new Map<string, string>()
  for (const j of JOB_SEEDS) map.set(j.mallId, j.mallName)
  return Array.from(map.entries()).map(([id, name]) => ({ id, name }))
}
