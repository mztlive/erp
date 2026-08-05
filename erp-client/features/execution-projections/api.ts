/**
 * W23 session-mock API：queryFn / mutationFn 纯函数。
 * - 结果未知先查询：未明确前不成功、不跳过、不计入已确认
 * - RETRY 沿原投影修订与幂等键，不生成新修订
 * - ESCALATE 只返回 接口错误中心入口，不提供任务租约/领取/完成
 * - 批量仅接受显式选择快照（Q3 未确认前拒绝“当前筛选全部”）
 * - 前端不从销售单重组装投影字段
 */

import { mockDelay } from "@/lib/mock-delay"
import { filterRowsBySearch } from "@/lib/filter-utils"
import type {
  BulkItemOutcome,
  BulkProjectionJob,
  DeliveryCommandResultCode,
  DeliveryStatus,
  ExecutionProjectionListQuery,
  ExecutionProjectionListResult,
  ExecutionProjectionMetric,
  ExecutionProjectionMetricKey,
  ExecutionProjectionRow,
  ExecutionProjectionView,
  ProjectionDeliveryCommandResult,
  ProjectionWhitelistContent,
  RevisionLink,
  SalesOrderCollaborationSummary,
} from "@/features/execution-projections/types"
import {
  DELIVERY_STATUS_LABEL,
  DELIVERY_STATUS_TONE,
  LATENCY_LABEL,
  RECONCILIATION_LABEL,
  SOURCE_LABEL,
} from "@/features/execution-projections/types"
import {
  EXECUTION_PROJECTION_SEEDS,
  MALL_OPTIONS,
  seedToListRow,
  type ProjectionSeed,
} from "@/mock/execution-projections"

const PERMISSION_VERSION = "pv-w23-demo-1"

/** 批量操作上限（前端选择条需同步提示，超限禁用批量按钮） */
export const BULK_SELECTION_LIMIT = 20

type DeliveryOverlay = {
  status: DeliveryStatus
  attemptCount: number
  lastAttemptAt?: string
  nextAttemptAt?: string
  mallAckAt?: string
  mallExecutionBaseline?: string
  errorCode?: string
  errorSummary?: string
  workItemId?: string
  errorTaskId?: string
  objectVersion: string
  currentAckedRevisionNo?: number
}

const deliveryOverlays = new Map<string, DeliveryOverlay>()
const bulkJobs = new Map<string, BulkProjectionJob>()
const selectionSnapshots = new Map<
  string,
  {
    ids: string[]
    objectVersions: Record<string, string>
    permissionVersion: string
    createdAt: string
  }
>()

let opSeq = 1000

function nowIso() {
  return new Date().toISOString()
}

function formatNowLocal() {
  return new Date()
    .toLocaleString("zh-CN", { hour12: false })
    .replace(/\//g, "-")
}

function nextOperationId(kind: string) {
  opSeq += 1
  return `op-w23-${kind}-${opSeq}`
}

function getSeed(projectionId: string): ProjectionSeed | undefined {
  return EXECUTION_PROJECTION_SEEDS.find((s) => s.projectionId === projectionId)
}

function effectiveSeed(seed: ProjectionSeed): ProjectionSeed {
  const overlay = deliveryOverlays.get(seed.projectionId)
  if (!overlay) return seed
  return {
    ...seed,
    deliveryStatus: overlay.status,
    attemptCount: overlay.attemptCount,
    lastAttemptAt: overlay.lastAttemptAt ?? seed.lastAttemptAt,
    nextAttemptAt: overlay.nextAttemptAt,
    mallAckAt: overlay.mallAckAt ?? seed.mallAckAt,
    mallExecutionBaseline:
      overlay.mallExecutionBaseline ?? seed.mallExecutionBaseline,
    errorCode: overlay.errorCode,
    errorSummary: overlay.errorSummary,
    workItemId: overlay.workItemId ?? seed.workItemId,
    errorTaskId: overlay.errorTaskId ?? seed.errorTaskId,
    currentAckedRevisionNo:
      overlay.currentAckedRevisionNo ?? seed.currentAckedRevisionNo,
  }
}

function objectVersionOf(seed: ProjectionSeed): string {
  const overlay = deliveryOverlays.get(seed.projectionId)
  return overlay?.objectVersion ?? `ov-${seed.projectionId}-v1`
}

function recomputeActions(seed: ProjectionSeed): {
  allowedActions: string[]
  actionBlockers: ProjectionSeed["actionBlockers"]
} {
  const status = seed.deliveryStatus
  const blockers: ProjectionSeed["actionBlockers"] = []
  const allowed: string[] = []

  if (status === "UNKNOWN" || status === "SENDING" || status === "RETRYING") {
    allowed.push("QUERY_RESULT")
  }
  if (status === "FAILED") {
    allowed.push("RETRY", "QUERY_RESULT", "ESCALATE")
  }
  if (status === "UNKNOWN" && seed.latencyBand === "over_sla") {
    allowed.push("ESCALATE")
  }
  if (status === "ESCALATED_MANUAL") {
    allowed.push("ESCALATE")
    blockers.push(
      {
        action: "RETRY",
        code: "ESCALATED",
        message: "已转人工处理，按单据重试请到接口错误中心按原任务号处理。",
      },
      {
        action: "QUERY_RESULT",
        code: "ESCALATED",
        message: "已有错误记录，请在接口错误中心处理。",
      }
    )
  }
  if (status === "ACKED") {
    blockers.push(
      {
        action: "RETRY",
        code: "ALREADY_ACKED",
        message: "商城已确认，无需重试。",
      },
      {
        action: "QUERY_RESULT",
        code: "ALREADY_ACKED",
        message: "已有明确确认结果。",
      }
    )
  }
  if (status === "PENDING") {
    blockers.push(
      {
        action: "RETRY",
        code: "NOT_YET_SENT",
        message: "尚未首次发送，将由后台按计划执行。",
      },
      {
        action: "QUERY_RESULT",
        code: "NO_REQUEST",
        message: "尚无可查询的原请求。",
      }
    )
  }
  if (status === "SENDING" || status === "RETRYING") {
    blockers.push({
      action: "RETRY",
      code: "IN_FLIGHT",
      message: "正在发送中，请勿重复操作。",
    })
  }
  if (status === "UNKNOWN") {
    blockers.push({
      action: "RETRY",
      code: "RESULT_UNKNOWN",
      message: "结果未知须先查询最终结果，未明确前不得重试或标为成功。",
    })
  }

  // dedupe allowed
  const unique = [...new Set(allowed)]
  return { allowedActions: unique, actionBlockers: blockers }
}

function projectRow(seed: ProjectionSeed): ExecutionProjectionRow {
  const effective = effectiveSeed(seed)
  const actions = recomputeActions(effective)
  const row = seedToListRow({
    ...effective,
    allowedActions: actions.allowedActions,
    actionBlockers: actions.actionBlockers,
  })
  return { ...row, objectVersion: objectVersionOf(seed) }
}

function matchMetric(
  row: ExecutionProjectionRow,
  metric: ExecutionProjectionMetricKey
): boolean {
  switch (metric) {
    case "pending_send":
      return row.delivery.status === "PENDING"
    case "inflight":
      return (
        row.delivery.status === "SENDING" ||
        row.delivery.status === "RETRYING"
      )
    case "timeout":
      return (
        row.latencyBand === "over_sla" &&
        row.delivery.status !== "ACKED" &&
        row.delivery.status !== "ESCALATED_MANUAL"
      )
    case "fail_manual":
      return (
        row.delivery.status === "FAILED" ||
        row.delivery.status === "ESCALATED_MANUAL" ||
        row.delivery.status === "UNKNOWN"
      )
    case "acked":
      // 结果未知不得计入已确认
      return row.delivery.status === "ACKED"
    default:
      return true
  }
}

function parseStatuses(raw?: string): DeliveryStatus[] | null {
  if (!raw || raw === "all") return null
  const parts = raw.split(",").map((s) => s.trim()).filter(Boolean)
  const valid: DeliveryStatus[] = []
  for (const p of parts) {
    if (
      p === "PENDING" ||
      p === "SENDING" ||
      p === "RETRYING" ||
      p === "ACKED" ||
      p === "FAILED" ||
      p === "ESCALATED_MANUAL" ||
      p === "UNKNOWN"
    ) {
      valid.push(p)
    }
  }
  return valid.length ? valid : null
}

function filterRows(
  rows: ExecutionProjectionRow[],
  query: ExecutionProjectionListQuery
): ExecutionProjectionRow[] {
  const statuses = parseStatuses(query.deliveryStatus)
  let next = filterRowsBySearch(rows, query.q, (r) => [
    r.salesOrderNo,
    r.projectionNo,
    r.projectionId,
    r.customerLabel,
  ])

  if (query.mallId && query.mallId !== "all") {
    next = next.filter((r) => r.targetMallId === query.mallId)
  }
  if (statuses) {
    next = next.filter((r) => statuses.includes(r.delivery.status))
  }
  if (query.source && query.source !== "all") {
    next = next.filter((r) => r.projectionSource === query.source)
  }
  if (query.latency && query.latency !== "all") {
    next = next.filter((r) => r.latencyBand === query.latency)
  }
  if (query.reconciliation && query.reconciliation !== "all") {
    next = next.filter((r) => r.reconciliationStatus === query.reconciliation)
  }
  if (query.metric && query.metric !== "all") {
    const metricKey = query.metric
    next = next.filter((r) => matchMetric(r, metricKey))
  }

  // 风险优先：未知/转人工 → 失败 → 超时 → 待发送 → 已确认
  const rank = (s: DeliveryStatus) => {
    switch (s) {
      case "UNKNOWN":
        return 0
      case "ESCALATED_MANUAL":
        return 1
      case "FAILED":
        return 2
      case "RETRYING":
        return 3
      case "SENDING":
        return 4
      case "PENDING":
        return 5
      case "ACKED":
        return 6
      default:
        return 9
    }
  }
  return [...next].sort((a, b) => {
    const ra = rank(a.delivery.status)
    const rb = rank(b.delivery.status)
    if (ra !== rb) return ra - rb
    if (a.latencyBand === "over_sla" && b.latencyBand !== "over_sla") return -1
    if (b.latencyBand === "over_sla" && a.latencyBand !== "over_sla") return 1
    return a.salesOrderNo.localeCompare(b.salesOrderNo)
  })
}

function computeMetrics(
  all: ExecutionProjectionRow[]
): ExecutionProjectionMetric[] {
  const count = (key: ExecutionProjectionMetricKey) =>
    all.filter((r) => matchMetric(r, key)).length
  return [
    {
      key: "pending_send",
      label: "待发送",
      value: count("pending_send"),
    },
    {
      key: "inflight",
      label: "发送/重试中",
      value: count("inflight"),
    },
    {
      key: "timeout",
      label: "待确认超时",
      value: count("timeout"),
      detail: "已超时且未确认",
    },
    {
      key: "fail_manual",
      label: "失败/转人工",
      value: count("fail_manual"),
      detail: "含结果未知",
    },
    {
      key: "acked",
      label: "已确认",
      value: count("acked"),
      detail: "不含结果未知",
    },
  ]
}

function filterSummary(query: ExecutionProjectionListQuery): string {
  const parts: string[] = []
  if (query.metric && query.metric !== "all") {
    const labels: Record<ExecutionProjectionMetricKey, string> = {
      pending_send: "待发送",
      inflight: "发送/重试中",
      timeout: "待确认超时",
      fail_manual: "失败/转人工",
      acked: "已确认",
    }
    parts.push(`指标=${labels[query.metric]}`)
  }
  if (query.deliveryStatus && query.deliveryStatus !== "all") {
    const labels = parseStatuses(query.deliveryStatus)?.map(
      (s) => DELIVERY_STATUS_LABEL[s]
    )
    parts.push(`接收状态=${labels?.join("、") ?? query.deliveryStatus}`)
  }
  if (query.mallId && query.mallId !== "all") {
    const mall = MALL_OPTIONS.find((m) => m.id === query.mallId)
    parts.push(`商城=${mall?.name ?? query.mallId}`)
  }
  if (query.source && query.source !== "all") {
    parts.push(`来源=${SOURCE_LABEL[query.source]}`)
  }
  if (query.latency && query.latency !== "all") {
    parts.push(`等待时长=${LATENCY_LABEL[query.latency]}`)
  }
  if (query.reconciliation && query.reconciliation !== "all") {
    parts.push(`对账=${RECONCILIATION_LABEL[query.reconciliation]}`)
  }
  if (query.q?.trim()) parts.push(`搜索=${query.q.trim()}`)
  return parts.length ? parts.join(" · ") : "默认：风险优先 · 全状态"
}

export async function fetchExecutionProjectionList(
  query: ExecutionProjectionListQuery = {}
): Promise<ExecutionProjectionListResult> {
  await mockDelay()
  const all = EXECUTION_PROJECTION_SEEDS.map(projectRow)
  const filtered = filterRows(all, query)
  const page = Math.max(1, query.page ?? 1)
  const pageSize = Math.min(50, Math.max(1, query.pageSize ?? 8))
  const start = (page - 1) * pageSize
  const rows = filtered.slice(start, start + pageSize)
  const now = nowIso()

  return {
    rows,
    pageInfo: { page, pageSize, total: filtered.length },
    metrics: computeMetrics(all),
    malls: MALL_OPTIONS.map((m) => ({ id: m.id, name: m.name })),
    permissionVersion: PERMISSION_VERSION,
    sourceFactsAsOf: "2026-08-01T09:30:00+08:00",
    projectionUpdatedAt: "2026-08-01T09:34:00+08:00",
    deliveryStatusUpdatedAt: now,
    queriedAt: now,
    filterSummary: filterSummary(query),
    defaultViewNote:
      "运营默认关注未确认与失败；结果未知不计入已确认指标。",
  }
}

function buildTracks(seed: ProjectionSeed): ExecutionProjectionView["tracks"] {
  const delivery = seed.deliveryStatus
  const acked = seed.currentAckedRevisionNo

  return {
    salesFact: {
      label: seed.salesOrderStatus,
      tone: seed.salesOrderStatusTone,
      description: `ERP 销售版本 v${seed.salesOrderRevisionNo} 已形成；接收失败不回退销售记录或应收。`,
    },
    projectionDelivery: {
      label: DELIVERY_STATUS_LABEL[delivery],
      tone: DELIVERY_STATUS_TONE[delivery],
      description:
        delivery === "ACKED"
          ? "信息发送已完成"
          : delivery === "UNKNOWN"
            ? "结果未知：须先查询，未明确前不显示成功"
            : `尝试 ${seed.attemptCount} 次${seed.nextAttemptAt ? ` · 下次 ${seed.nextAttemptAt}` : ""}`,
    },
    mallConfirm: {
      label:
        delivery === "ACKED"
          ? "已确认"
          : acked != null
            ? `已确认至 v${acked}`
            : "尚未确认",
      tone: delivery === "ACKED" ? "success" : acked != null ? "info" : "neutral",
      description:
        seed.mallAckAt != null
          ? `商城确认时间 ${seed.mallAckAt}`
          : "尚无明确商城确认时间",
    },
  }
}

function buildRevisionLinks(
  seed: ProjectionSeed,
  selectedRevisionId: string
): RevisionLink[] {
  const links: RevisionLink[] = []
  const current: RevisionLink = {
    salesOrderRevisionId: seed.salesOrderRevisionId,
    salesOrderRevisionNo: seed.salesOrderRevisionNo,
    projectionRevisionId: seed.projectionRevisionId,
    projectionRevisionNo: seed.projectionRevisionNo,
    deliveryStatus: seed.deliveryStatus,
    deliveryStatusLabel: DELIVERY_STATUS_LABEL[seed.deliveryStatus],
    mallAckAt: seed.mallAckAt,
    sourceSalesRevisionNo: seed.salesOrderRevisionNo,
    isCurrentSelection: selectedRevisionId === seed.projectionRevisionId,
  }
  links.push(current)
  for (const h of seed.history ?? []) {
    links.push({
      salesOrderRevisionId: h.salesOrderRevisionId,
      salesOrderRevisionNo: h.salesOrderRevisionNo,
      projectionRevisionId: h.projectionRevisionId,
      projectionRevisionNo: h.projectionRevisionNo,
      deliveryStatus: h.deliveryStatus,
      deliveryStatusLabel: DELIVERY_STATUS_LABEL[h.deliveryStatus],
      mallAckAt: h.mallAckAt,
      // 历史固定来源销售版本，即使 W05 当前版更高也不覆盖
      sourceSalesRevisionNo: h.salesOrderRevisionNo,
      isCurrentSelection: selectedRevisionId === h.projectionRevisionId,
    })
  }
  return links.sort((a, b) => b.projectionRevisionNo - a.projectionRevisionNo)
}

function resolveContent(
  seed: ProjectionSeed,
  revisionId?: string
): {
  projectionRevisionId: string
  revisionNo: number
  projectionSource: ProjectionSeed["projectionSource"]
  salesOrderRevisionId: string
  salesOrderRevisionNo: number
  content: ProjectionWhitelistContent
} {
  if (revisionId && revisionId !== seed.projectionRevisionId) {
    const hist = seed.history?.find((h) => h.projectionRevisionId === revisionId)
    if (hist) {
      return {
        projectionRevisionId: hist.projectionRevisionId,
        revisionNo: hist.projectionRevisionNo,
        projectionSource: seed.projectionSource,
        salesOrderRevisionId: hist.salesOrderRevisionId,
        salesOrderRevisionNo: hist.salesOrderRevisionNo,
        content: { ...hist.content },
      }
    }
  }
  return {
    projectionRevisionId: seed.projectionRevisionId,
    revisionNo: seed.projectionRevisionNo,
    projectionSource: seed.projectionSource,
    salesOrderRevisionId: seed.salesOrderRevisionId,
    salesOrderRevisionNo: seed.salesOrderRevisionNo,
    content: { ...seed.content },
  }
}

export async function fetchExecutionProjectionDetail(input: {
  projectionId: string
  revisionId?: string
}): Promise<ExecutionProjectionView | null> {
  await mockDelay()
  const base = getSeed(input.projectionId)
  if (!base) return null
  const seed = effectiveSeed(base)
  const actions = recomputeActions(seed)
  const selected = resolveContent(seed, input.revisionId)
  const now = nowIso()

  const deliveries: ExecutionProjectionView["deliveries"] = [
    {
      deliveryId: `dlv_${seed.projectionId}`,
      status: seed.deliveryStatus,
      statusLabel: DELIVERY_STATUS_LABEL[seed.deliveryStatus],
      statusTone: DELIVERY_STATUS_TONE[seed.deliveryStatus],
      attemptCount: seed.attemptCount,
      lastAttemptAt: seed.lastAttemptAt,
      nextAttemptAt: seed.nextAttemptAt,
      mallAckAt: seed.mallAckAt,
      mallExecutionBaseline: seed.mallExecutionBaseline,
      errorCode: seed.errorCode,
      errorSummary: seed.errorSummary,
      workItemId: seed.workItemId,
      errorTaskId: seed.errorTaskId,
    },
  ]

  return {
    identity: {
      projectionId: seed.projectionId,
      projectionNo: seed.projectionNo,
      salesOrderId: seed.salesOrderId,
      salesOrderNo: seed.salesOrderNo,
      targetMallId: seed.targetMallId,
      targetMallName: seed.targetMallName,
    },
    tracks: buildTracks(seed),
    selectedRevision: {
      projectionRevisionId: selected.projectionRevisionId,
      revisionNo: selected.revisionNo,
      projectionSource: selected.projectionSource,
      salesOrderRevisionId: selected.salesOrderRevisionId,
      salesOrderRevisionNo: selected.salesOrderRevisionNo,
      content: selected.content,
    },
    currentAckedRevisionNo: seed.currentAckedRevisionNo,
    revisionLinks: buildRevisionLinks(seed, selected.projectionRevisionId),
    deliveries,
    salesOrderStatus: seed.salesOrderStatus,
    salesOrderStatusTone: seed.salesOrderStatusTone,
    ownerLabel: seed.ownerLabel,
    pendingDurationLabel: seed.pendingDurationLabel,
    latencyBand: seed.latencyBand,
    reconciliationStatus: seed.reconciliationStatus,
    allowedActions: actions.allowedActions,
    actionBlockers: actions.actionBlockers,
    fieldPermissions: {
      customerExternalIdentity: "masked",
      faceValue: "full",
      cardCount: "full",
      cardForm: "full",
      voucherExpiryAt: "full",
      contentHash: "full",
    },
    objectVersion: objectVersionOf(seed),
    sourceFactsAsOf: "2026-08-01T09:30:00+08:00",
    projectionUpdatedAt: "2026-08-01T09:34:00+08:00",
    deliveryStatusUpdatedAt: now,
    queriedAt: now,
    boundaryNotice:
      "数据不是销售单副本。接收失败不回退销售记录、销售版本或应收；业务内容变更须在销售单走变更单形成新版本后自动产生新数据。",
  }
}

/** W05 协同子区：按销售单读取当前投影摘要（无需进入 W23） */
export async function fetchSalesOrderCollaboration(
  salesOrderId: string
): Promise<SalesOrderCollaborationSummary> {
  await mockDelay(40)
  const seeds = EXECUTION_PROJECTION_SEEDS.filter(
    (s) => s.salesOrderId === salesOrderId
  ).map(effectiveSeed)

  if (seeds.length === 0) {
    // 尝试按销售单号匹配演示数据中的卡券单
    return {
      salesOrderId,
      salesOrderNo: salesOrderId,
      hasProjection: false,
      historyCount: 0,
      note: "当前销售单尚无执行信息。卡券销售版本生效后由系统自动形成数据。",
    }
  }

  const seed = seeds[0]!
  const row = projectRow(seed)
  const tracks = buildTracks(seed)
  const historyCount = 1 + (seed.history?.length ?? 0)

  return {
    salesOrderId: seed.salesOrderId,
    salesOrderNo: seed.salesOrderNo,
    hasProjection: true,
    projectionId: seed.projectionId,
    projectionNo: seed.projectionNo,
    salesOrderRevisionNo: seed.salesOrderRevisionNo,
    projectionRevisionNo: seed.projectionRevisionNo,
    targetMallName: seed.targetMallName,
    tracks,
    delivery: row.delivery,
    whitelistPreview: row.whitelistPreview,
    currentAckedRevisionNo: seed.currentAckedRevisionNo,
    reconciliationStatus: seed.reconciliationStatus,
    historyCount,
    w23Href: `/commerce/execution-projections?projectionId=${seed.projectionId}`,
    historyHref: `/commerce/execution-projections?projectionId=${seed.projectionId}&revision=${seed.projectionRevisionId}`,
    note: "协同状态读取系统数据；本区只读。变更销售内容请走销售变更单。",
  }
}

export type DeliveryCommandInput = {
  projectionId: string
  projectionRevisionId: string
  deliveryId: string
  action: "QUERY_RESULT" | "RETRY" | "ESCALATE"
  expectedObjectVersion: string
  requestId: string
  /** 演示：强制查询仍返回未知 */
  forceStillUnknown?: boolean
}

export async function submitProjectionDeliveryCommand(
  input: DeliveryCommandInput
): Promise<ProjectionDeliveryCommandResult> {
  await mockDelay(120)

  const base = getSeed(input.projectionId)
  if (!base) {
    throw new Error("数据不存在")
  }
  const seed = effectiveSeed(base)
  const version = objectVersionOf(seed)
  if (version !== input.expectedObjectVersion) {
    throw new Error("数据版本已变更，请刷新后重试")
  }

  const actions = recomputeActions(seed)
  if (!actions.allowedActions.includes(input.action)) {
    const blocker = actions.actionBlockers.find((b) => b.action === input.action)
    throw new Error(blocker?.message ?? `当前不可执行 ${input.action}`)
  }

  const occurredAt = formatNowLocal()
  let result: DeliveryCommandResultCode
  let resultLabel: string
  let nextAction: string
  let stillUnknown = false
  let workItemId = seed.workItemId
  let errorTaskId = seed.errorTaskId
  const overlay: DeliveryOverlay = {
    status: seed.deliveryStatus,
    attemptCount: seed.attemptCount,
    lastAttemptAt: seed.lastAttemptAt,
    nextAttemptAt: seed.nextAttemptAt,
    mallAckAt: seed.mallAckAt,
    mallExecutionBaseline: seed.mallExecutionBaseline,
    errorCode: seed.errorCode,
    errorSummary: seed.errorSummary,
    workItemId: seed.workItemId,
    errorTaskId: seed.errorTaskId,
    objectVersion: version,
    currentAckedRevisionNo: seed.currentAckedRevisionNo,
  }

  if (input.action === "QUERY_RESULT") {
    if (input.forceStillUnknown || seed.deliveryStatus === "SENDING") {
      result = "STILL_UNKNOWN"
      resultLabel = "结果仍未知"
      nextAction = "保留当前项，可再次查询或升级到接口错误中心"
      stillUnknown = true
      // 不得标成功、不得改已确认
      overlay.status = "UNKNOWN"
      overlay.lastAttemptAt = occurredAt
    } else if (
      seed.deliveryStatus === "UNKNOWN" ||
      seed.deliveryStatus === "RETRYING"
    ) {
      // 演示：查询后明确失败或确认
      if (seed.latencyBand === "over_sla" && seed.deliveryStatus === "UNKNOWN") {
        result = "FAILED"
        resultLabel = "查询后明确失败"
        nextAction = "可按单据重试或转到接口错误中心"
        overlay.status = "FAILED"
        overlay.errorCode = seed.errorCode ?? "QUERIED_FAILED"
        overlay.errorSummary =
          seed.errorSummary ?? "查询原请求后商城返回失败（脱敏）"
        overlay.lastAttemptAt = occurredAt
        overlay.nextAttemptAt = undefined
      } else {
        result = "ACKED"
        resultLabel = "查询后明确确认"
        nextAction = "可返回列表或查看销售单协同"
        overlay.status = "ACKED"
        overlay.mallAckAt = occurredAt
        overlay.mallExecutionBaseline =
          seed.mallExecutionBaseline ?? "mall-exec·queried"
        overlay.currentAckedRevisionNo = seed.projectionRevisionNo
        overlay.errorCode = undefined
        overlay.errorSummary = undefined
        overlay.nextAttemptAt = undefined
        overlay.lastAttemptAt = occurredAt
      }
    } else if (seed.deliveryStatus === "FAILED") {
      result = "FAILED"
      resultLabel = "查询确认仍为失败"
      nextAction = "可重试发送或升级到接口错误中心"
      overlay.lastAttemptAt = occurredAt
    } else {
      result = "STILL_UNKNOWN"
      resultLabel = "结果仍未知"
      nextAction = "未明确前不计入已确认，不跳过"
      stillUnknown = true
      overlay.status = "UNKNOWN"
    }
  } else if (input.action === "RETRY") {
    // 沿原修订继续，不生成新投影修订
    result = "RETRY_SCHEDULED"
    resultLabel = "已安排按原任务号重试"
    nextAction = "等待发送结果；超时请先查询"
    overlay.status = "RETRYING"
    overlay.attemptCount = seed.attemptCount + 1
    overlay.lastAttemptAt = occurredAt
    overlay.nextAttemptAt = "约 5 分钟后"
    overlay.errorSummary = seed.errorSummary
  } else {
    // ESCALATE：幂等创建/复用 接口错误记录
    workItemId = seed.workItemId ?? `wi_err_${seed.projectionId}`
    errorTaskId = seed.errorTaskId ?? `err_task_${seed.projectionId}`
    result = "ESCALATED"
    resultLabel = "已升级到接口错误中心"
    nextAction = "打开接口错误中心处理错误待办；本页不领取或完成任务"
    overlay.status = "ESCALATED_MANUAL"
    overlay.workItemId = workItemId
    overlay.errorTaskId = errorTaskId
    overlay.lastAttemptAt = occurredAt
    overlay.nextAttemptAt = undefined
  }

  overlay.objectVersion = `ov-${seed.projectionId}-v${Date.now().toString(36)}`
  deliveryOverlays.set(seed.projectionId, overlay)

  const payload: ProjectionDeliveryCommandResult = {
    operationId: nextOperationId(input.action.toLowerCase()),
    deliveryId: input.deliveryId,
    projectionId: seed.projectionId,
    salesOrderNo: seed.salesOrderNo,
    result,
    resultLabel,
    workItemId,
    errorTaskId,
    occurredAt,
    nextAction,
    stillUnknown,
    objectVersion: overlay.objectVersion,
  }

  return payload
}

export type BulkCommandInput = {
  action: "BULK_QUERY" | "BULK_RETRY"
  /** 仅显式选择的稳定 ID；拒绝“当前筛选全部” */
  projectionIds: string[]
  requestId: string
}

export async function submitBulkProjectionCommand(
  input: BulkCommandInput
): Promise<BulkProjectionJob> {
  await mockDelay(150)

  if (!input.projectionIds.length) {
    throw new Error("请先逐项显式勾选失败/可处理项")
  }
  if (input.projectionIds.length > BULK_SELECTION_LIMIT) {
    throw new Error(`批量最多 ${BULK_SELECTION_LIMIT} 条，超出部分请分批`)
  }

  const snapshotId = `snap-${input.requestId}`
  const objectVersions: Record<string, string> = {}
  for (const id of input.projectionIds) {
    const seed = getSeed(id)
    if (seed) objectVersions[id] = objectVersionOf(seed)
  }
  selectionSnapshots.set(snapshotId, {
    ids: [...input.projectionIds],
    objectVersions,
    permissionVersion: PERMISSION_VERSION,
    createdAt: nowIso(),
  })

  const items: BulkItemOutcome[] = []
  let succeeded = 0
  let skipped = 0
  const failed = 0
  let stillUnknown = 0

  for (const id of input.projectionIds) {
    const base = getSeed(id)
    if (!base) {
      items.push({
        projectionId: id,
        salesOrderNo: id,
        deliveryId: `dlv_${id}`,
        outcome: "skipped",
        reason: "数据不存在或已不在授权范围",
      })
      skipped += 1
      continue
    }
    const seed = effectiveSeed(base)
    const actions = recomputeActions(seed)

    if (input.action === "BULK_RETRY") {
      if (seed.deliveryStatus === "UNKNOWN") {
        items.push({
          projectionId: id,
          salesOrderNo: seed.salesOrderNo,
          deliveryId: `dlv_${id}`,
          outcome: "still_unknown",
          reason: "结果未知：批量重试跳过该项，须先查询",
        })
        stillUnknown += 1
        continue
      }
      if (seed.deliveryStatus === "ACKED") {
        items.push({
          projectionId: id,
          salesOrderNo: seed.salesOrderNo,
          deliveryId: `dlv_${id}`,
          outcome: "skipped",
          reason: "已确认项默认不参与批量重试",
        })
        skipped += 1
        continue
      }
      if (!actions.allowedActions.includes("RETRY")) {
        items.push({
          projectionId: id,
          salesOrderNo: seed.salesOrderNo,
          deliveryId: `dlv_${id}`,
          outcome: "skipped",
          reason:
            actions.actionBlockers.find((b) => b.action === "RETRY")?.message ??
            "当前不可重试",
        })
        skipped += 1
        continue
      }
      deliveryOverlays.set(id, {
        status: "RETRYING",
        attemptCount: seed.attemptCount + 1,
        lastAttemptAt: formatNowLocal(),
        nextAttemptAt: "约 5 分钟后",
        errorSummary: seed.errorSummary,
        objectVersion: `ov-${id}-bulk-${Date.now().toString(36)}`,
        currentAckedRevisionNo: seed.currentAckedRevisionNo,
      })
      items.push({
        projectionId: id,
        salesOrderNo: seed.salesOrderNo,
        deliveryId: `dlv_${id}`,
        outcome: "succeeded",
        reason: "已按原任务号安排重试",
      })
      succeeded += 1
    } else {
      // BULK_QUERY
      if (!actions.allowedActions.includes("QUERY_RESULT")) {
        items.push({
          projectionId: id,
          salesOrderNo: seed.salesOrderNo,
          deliveryId: `dlv_${id}`,
          outcome: "skipped",
          reason:
            actions.actionBlockers.find((b) => b.action === "QUERY_RESULT")
              ?.message ?? "当前不可查询",
        })
        skipped += 1
        continue
      }
      if (seed.deliveryStatus === "SENDING") {
        deliveryOverlays.set(id, {
          status: "UNKNOWN",
          attemptCount: seed.attemptCount,
          lastAttemptAt: formatNowLocal(),
          objectVersion: `ov-${id}-bulk-${Date.now().toString(36)}`,
        })
        items.push({
          projectionId: id,
          salesOrderNo: seed.salesOrderNo,
          deliveryId: `dlv_${id}`,
          outcome: "still_unknown",
          reason: "查询后仍未知，不计入已确认",
        })
        stillUnknown += 1
        continue
      }
      if (seed.deliveryStatus === "UNKNOWN") {
        deliveryOverlays.set(id, {
          status: "FAILED",
          attemptCount: seed.attemptCount,
          lastAttemptAt: formatNowLocal(),
          errorCode: seed.errorCode ?? "QUERIED_FAILED",
          errorSummary: seed.errorSummary ?? "批量查询后明确失败",
          objectVersion: `ov-${id}-bulk-${Date.now().toString(36)}`,
        })
        items.push({
          projectionId: id,
          salesOrderNo: seed.salesOrderNo,
          deliveryId: `dlv_${id}`,
          outcome: "succeeded",
          reason: "查询后明确为失败",
        })
        succeeded += 1
        continue
      }
      items.push({
        projectionId: id,
        salesOrderNo: seed.salesOrderNo,
        deliveryId: `dlv_${id}`,
        outcome: "succeeded",
        reason: "已刷新发送状态",
      })
      succeeded += 1
    }
  }

  const total = items.length
  const completed = total
  const failedCount = failed
  let status: BulkProjectionJob["status"] = "succeeded"
  if (failedCount > 0 && succeeded === 0 && stillUnknown === 0) status = "failed"
  else if (skipped > 0 || stillUnknown > 0 || failedCount > 0) status = "partial"

  const job: BulkProjectionJob = {
    jobId: `job-w23-${input.requestId}`,
    action: input.action,
    status,
    total,
    completed,
    succeeded,
    skipped,
    failed: failedCount,
    stillUnknown,
    selectionSnapshotId: snapshotId,
    items,
    startedAt: formatNowLocal(),
    finishedAt: formatNowLocal(),
    nextAction:
      stillUnknown > 0
        ? "存在仍未知项：勿按成功处理，可再次查询或升级到接口错误中心"
        : "可刷新列表查看逐项状态",
  }

  bulkJobs.set(job.jobId, job)
  return job
}

export async function fetchBulkJob(
  jobId: string
): Promise<BulkProjectionJob | null> {
  await mockDelay(30)
  return bulkJobs.get(jobId) ?? null
}
