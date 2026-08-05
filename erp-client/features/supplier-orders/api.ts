/**
 * W26 session-mock API：queryFn / mutationFn 纯函数。
 * - RESULT_UNKNOWN 主路径仅 QUERY_RESULT；REPLAY 仅在明确无结果 + canSafeRetry
 * - 取消/退款必须引用既有商城售后请求
 * - 查询/重放/暂挂保持任务非终结
 * - 快照字段不可变；敏感地址短时揭示；成本按字段权限掩码
 */

import { mockDelay } from "@/lib/mock-delay"
import type {
  ActionBlocker,
  AfterSalesActionInput,
  AfterSalesActionResult,
  CancelStatus,
  CostView,
  DeferTaskInput,
  DeferTaskResult,
  DemoRole,
  ExportCommand,
  ExportJobResult,
  FormalActionResponse,
  InvestigationEvidenceView,
  NoteInput,
  QueryResultData,
  QueryResultInput,
  RefundStatus,
  ReplayInput,
  ReplayResultData,
  RevealAddressInput,
  RevealAddressResult,
  SupplierFulfillmentStatus,
  SupplierOrderDetailView,
  SupplierOrderListQuery,
  SupplierOrderListResult,
  SupplierOrderListRow,
  SupplierOrderMetric,
  WorkItemView,
} from "@/features/supplier-orders/types"
import {
  ACTIONABLE_FULFILLMENT,
  CANCEL_STATUS_LABEL,
  CANCEL_STATUS_TONE,
  COST_MASK,
  FULFILLMENT_STATUS_LABEL,
  FULFILLMENT_STATUS_TONE,
  REFUND_STATUS_LABEL,
  REFUND_STATUS_TONE,
} from "@/features/supplier-orders/types"
import {
  PAYMENT_OCCURRED_NOTICE,
  SUPPLIER_ORDER_SEEDS,
  toListRow,
  type SeedOrder,
} from "@/mock/supplier-orders"

type SessionPatch = {
  fulfillmentStatus?: SupplierFulfillmentStatus
  cancelStatus?: CancelStatus
  refundStatus?: RefundStatus
  externalOrderNo?: string
  lockVersion: number
  lastInvestigation?: InvestigationEvidenceView
  actionsExtra: SupplierOrderDetailView["actions"]
  notes: string[]
  workItemStatus?: WorkItemView["workItemStatus"]
  workItemHeld?: boolean
  subjectHash?: string
  subjectVersion?: string
  afterSalesPatches?: Record<
    string,
    {
      cancelStatus?: CancelStatus
      refundStatus?: RefundStatus
      supplierRefundStatus?: RefundStatus
    }
  >
  /** 已提交售后动作的幂等记录 */
  aftersaleIdempotency: Map<string, AfterSalesActionResult>
  placeAttemptCount?: number
}

const sessions = new Map<string, SessionPatch>()
const revealSessions = new Map<
  string,
  { expiresAt: number; reason: string; auditEventId: string }
>()
const investigationIdem = new Map<string, FormalActionResponse<QueryResultData | ReplayResultData>>()

function getSession(orderId: string): SessionPatch {
  let s = sessions.get(orderId)
  if (!s) {
    const seed = SUPPLIER_ORDER_SEEDS.find((x) => x.orderId === orderId)
    s = {
      lockVersion: seed?.lockVersion ?? 1,
      actionsExtra: [],
      notes: [],
      aftersaleIdempotency: new Map(),
      placeAttemptCount: 1,
    }
    sessions.set(orderId, s)
  }
  return s
}

function findSeed(orderId: string): SeedOrder | undefined {
  return SUPPLIER_ORDER_SEEDS.find((x) => x.orderId === orderId)
}

function costVisible(role: DemoRole): boolean {
  return role === "procurement" || role === "finance" || role === "admin"
}

function sensitiveAllowed(role: DemoRole): boolean {
  return role === "procurement" || role === "cs" || role === "admin"
}

function techVisible(role: DemoRole): boolean {
  return role === "admin"
}

function computeAllowedActions(
  seed: SeedOrder,
  session: SessionPatch,
  role: DemoRole
): { allowed: string[]; blockers: ActionBlocker[] } {
  const fulfillment =
    session.fulfillmentStatus ?? seed.fulfillmentStatus
  const allowed: string[] = ["OPEN_CENTER", "NOTE"]
  const blockers: ActionBlocker[] = []

  const hasQuery = seed.hasQueryCapability !== false
  const lastInv = session.lastInvestigation

  if (fulfillment === "RESULT_UNKNOWN") {
    if (hasQuery) {
      allowed.push("QUERY_RESULT")
    } else {
      blockers.push({
        action: "QUERY_RESULT",
        code: "NO_QUERY_CAPABILITY",
        message: "供应商无查询能力，请进入接口错误与对账处理",
        destinationWorkspaceId: "W29",
      })
      allowed.push("ESCALATE_W29")
    }

    // REPLAY only after VERIFIED_NO_RESULT + canSafeRetry
    if (
      lastInv?.outcome === "VERIFIED_NO_RESULT" &&
      lastInv.canSafeRetry
    ) {
      allowed.push("REPLAY")
    } else {
      blockers.push({
        action: "REPLAY",
        code: "REPLAY_NOT_SAFE",
        message:
          lastInv?.outcome === "VERIFIED_NO_RESULT"
            ? "已确认无结果，但系统尚未判定可安全重试"
            : "结果未知时不可直接重发，请先查询原结果",
      })
    }
  }

  // Aftersale actions only with mall request
  for (const as of seed.afterSales) {
    const patch = session.afterSalesPatches?.[as.requestId]
    const cancel = patch?.cancelStatus ?? as.cancelStatus
    const refund =
      patch?.supplierRefundStatus ?? as.supplierRefund.status
    if (
      as.allowedActions.includes("CANCEL") &&
      cancel !== "CANCELED" &&
      cancel !== "CANCEL_PENDING"
    ) {
      if (role === "cs" || role === "procurement" || role === "admin") {
        allowed.push("CANCEL")
      }
    }
    if (
      as.allowedActions.includes("REFUND") &&
      refund !== "REFUNDED" &&
      refund !== "REFUND_PENDING"
    ) {
      if (role === "cs" || role === "procurement" || role === "admin") {
        allowed.push("REFUND")
      }
    }
  }

  if (!asHasMallRequest(seed) && (allowed.includes("CANCEL") || allowed.includes("REFUND"))) {
    // still require request at submit time
  }

  if (!asHasMallRequest(seed)) {
    if (!seed.afterSales.length) {
      blockers.push({
        action: "CANCEL",
        code: "NO_MALL_AFTERSALE",
        message: "取消必须引用既有商城售后请求，禁止脱离请求任意取消",
      })
      blockers.push({
        action: "REFUND",
        code: "NO_MALL_AFTERSALE",
        message: "退款必须引用既有商城售后请求，禁止脱离请求任意退款",
      })
    }
  }

  if (seed.workItem && session.workItemStatus !== "COMPLETED") {
    for (const a of seed.workItem.allowedTaskActions) {
      if (!allowed.includes(a)) allowed.push(a)
    }
  }

  if (sensitiveAllowed(role)) {
    allowed.push("REVEAL_ADDRESS")
  } else {
    blockers.push({
      action: "REVEAL_ADDRESS",
      code: "FIELD_PERMISSION",
      message: "当前角色无权揭示收货地址",
    })
  }

  // Never allow direct re-place as primary for RESULT_UNKNOWN
  if (fulfillment === "RESULT_UNKNOWN") {
    const idx = allowed.indexOf("PLACE")
    if (idx >= 0) allowed.splice(idx, 1)
  }

  return { allowed: [...new Set(allowed)], blockers }
}

function asHasMallRequest(seed: SeedOrder): boolean {
  return seed.afterSales.some((a) => Boolean(a.mallRequestRef))
}

function projectListRow(
  seed: SeedOrder,
  session: SessionPatch,
  role: DemoRole
): SupplierOrderListRow {
  const fulfillment = session.fulfillmentStatus ?? seed.fulfillmentStatus
  const cancel = session.cancelStatus ?? seed.cancelStatus
  const refund = session.refundStatus ?? seed.refundStatus
  const { allowed, blockers } = computeAllowedActions(seed, session, role)
  const base = toListRow({
    ...seed,
    fulfillmentStatus: fulfillment,
    cancelStatus: cancel,
    refundStatus: refund,
    externalOrderNo: session.externalOrderNo ?? seed.externalOrderNo,
  })
  return {
    ...base,
    costGross: costVisible(role) ? seed.costs.cumulativeCostGross : null,
    itemCount: seed.items.length,
    allowedActions: allowed,
    actionBlockers: blockers,
  }
}

function matchesQuery(
  row: SupplierOrderListRow,
  seed: SeedOrder,
  q: SupplierOrderListQuery
): boolean {
  if (q.view === "actionable") {
    const actionableFulfillment = ACTIONABLE_FULFILLMENT.includes(
      row.fulfillmentStatus
    )
    const aftersalePending =
      row.cancelStatus === "FAILED" ||
      row.cancelStatus === "MANUAL" ||
      row.cancelStatus === "CANCEL_PENDING" ||
      row.refundStatus === "REFUND_FAILED" ||
      row.refundStatus === "MANUAL" ||
      row.refundStatus === "REFUND_PENDING" ||
      seed.afterSales.some(
        (a) =>
          a.mallRefund.status === "PENDING" ||
          a.supplierRefund.status === "REFUND_FAILED" ||
          a.supplierRefund.status === "NONE" &&
            a.allowedActions.length > 0
      )
    if (!actionableFulfillment && !aftersalePending) return false
  }
  if (q.view === "recent_completed") {
    if (row.fulfillmentStatus !== "COMPLETED") return false
  }

  if (q.supplierId && row.supplierId !== q.supplierId) return false

  if (q.fulfillmentStatuses?.length) {
    if (!q.fulfillmentStatuses.includes(row.fulfillmentStatus)) return false
  }
  if (q.cancelStatuses?.length) {
    if (!q.cancelStatuses.includes(row.cancelStatus)) return false
  }
  if (q.refundStatuses?.length) {
    if (!q.refundStatuses.includes(row.refundStatus)) return false
  }

  if (q.aftersalePending) {
    const pending =
      row.cancelStatus === "CANCEL_PENDING" ||
      row.cancelStatus === "FAILED" ||
      row.cancelStatus === "MANUAL" ||
      row.refundStatus === "REFUND_PENDING" ||
      row.refundStatus === "REFUND_FAILED" ||
      row.refundStatus === "MANUAL" ||
      row.refundStatus === "PARTIAL" ||
      seed.afterSales.some(
        (a) =>
          a.mallRefund.status === "PENDING" ||
          a.supplierRefund.status === "REFUND_FAILED" ||
          a.supplierRefund.status === "NONE" &&
            a.allowedActions.length > 0
      )
    if (!pending) return false
  }

  if (q.paidFrom && row.paidAt.slice(0, 10) < q.paidFrom) return false
  if (q.paidTo && row.paidAt.slice(0, 10) > q.paidTo) return false

  if (q.q?.trim()) {
    const needle = q.q.trim().toLowerCase()
    const hay = [
      row.orderNo,
      row.mallOrderNo,
      row.externalOrderNo,
      row.supplierName,
    ]
      .filter(Boolean)
      .join(" ")
      .toLowerCase()
    if (!hay.includes(needle)) return false
  }

  return true
}

function buildMetrics(rows: SupplierOrderListRow[]): SupplierOrderMetric[] {
  const all = SUPPLIER_ORDER_SEEDS.length
  return [
    {
      key: "pending_submit",
      label: "待提交",
      value: rows.filter(
        (r) =>
          r.fulfillmentStatus === "RECEIVED" ||
          r.fulfillmentStatus === "SUBMITTING"
      ).length,
      fulfillmentStatuses: ["RECEIVED", "SUBMITTING"],
    },
    {
      key: "result_unknown",
      label: "结果未知",
      value: rows.filter((r) => r.fulfillmentStatus === "RESULT_UNKNOWN")
        .length,
      fulfillmentStatus: "RESULT_UNKNOWN",
    },
    {
      key: "exception",
      label: "履约异常",
      value: rows.filter(
        (r) =>
          r.fulfillmentStatus === "EXCEPTION" ||
          r.fulfillmentStatus === "REJECTED"
      ).length,
      fulfillmentStatuses: ["EXCEPTION", "REJECTED"],
    },
    {
      key: "aftersale",
      label: "售后待处理",
      value: rows.filter(
        (r) =>
          r.cancelStatus === "FAILED" ||
          r.cancelStatus === "MANUAL" ||
          r.cancelStatus === "CANCEL_PENDING" ||
          r.refundStatus === "REFUND_FAILED" ||
          r.refundStatus === "MANUAL" ||
          r.refundStatus === "REFUND_PENDING" ||
          r.refundStatus === "PARTIAL"
      ).length,
      aftersalePending: true,
    },
    {
      key: "all",
      label: "全部订单",
      value: all,
      view: "all",
    },
  ]
}

const PERMISSION_VERSION = "pv-w26-1"

function filterSummary(
  query: SupplierOrderListQuery,
  total: number
): string {
  const parts: string[] = []
  if (query.view === "actionable") parts.push("可操作")
  else if (query.view === "recent_completed") parts.push("最近完成")
  else parts.push("全部")
  if (query.q?.trim()) parts.push(`搜索「${query.q.trim()}」`)
  if (query.supplierId) {
    parts.push(
      SUPPLIER_ORDER_SEEDS.find((s) => s.supplierId === query.supplierId)
        ?.supplierName ?? query.supplierId
    )
  }
  if (query.fulfillmentStatuses?.length) {
    parts.push(
      query.fulfillmentStatuses
        .map((s) => FULFILLMENT_STATUS_LABEL[s])
        .join("/")
    )
  }
  if (query.cancelStatuses?.length) {
    parts.push(
      query.cancelStatuses.map((s) => CANCEL_STATUS_LABEL[s]).join("/")
    )
  }
  if (query.refundStatuses?.length) {
    parts.push(
      query.refundStatuses.map((s) => REFUND_STATUS_LABEL[s]).join("/")
    )
  }
  if (query.paidFrom) parts.push(`支付自 ${query.paidFrom}`)
  if (query.paidTo) parts.push(`至 ${query.paidTo}`)
  parts.push(`${total} 条`)
  return parts.join(" · ")
}

function applySupplierOrderSort(
  rows: SupplierOrderListRow[],
  query: SupplierOrderListQuery
): SupplierOrderListRow[] {
  const sortBy = query.sortBy
  if (sortBy) {
    const direction = query.sortDir === "asc" ? 1 : -1
    return [...rows].sort((a, b) => {
      const left = (a[sortBy] ?? "") as string
      const right = (b[sortBy] ?? "") as string
      const comparison = left.localeCompare(right)
      if (comparison !== 0) return comparison * direction
      return a.orderId.localeCompare(b.orderId)
    })
  }
  return [...rows].sort((a, b) => {
    if (b.priority !== a.priority) return b.priority - a.priority
    if (a.lastBusinessAt !== b.lastBusinessAt) {
      return a.lastBusinessAt.localeCompare(b.lastBusinessAt)
    }
    return a.orderId.localeCompare(b.orderId)
  })
}

export async function fetchSupplierOrders(
  query: SupplierOrderListQuery
): Promise<SupplierOrderListResult> {
  await mockDelay()
  const role = query.role
  const projected = SUPPLIER_ORDER_SEEDS.map((seed) => {
    const session = getSession(seed.orderId)
    return { seed, row: projectListRow(seed, session, role) }
  })

  // Metrics follow non-view filters (供应商/日期/搜索)，与点击后筛选结果保持一致
  const metricRows = projected
    .filter((p) => matchesQuery(p.row, p.seed, { ...query, view: "all" }))
    .map((p) => p.row)
  const metrics = buildMetrics(metricRows)

  const filtered = projected
    .filter((p) => matchesQuery(p.row, p.seed, query))
    .map((p) => p.row)

  const sorted = applySupplierOrderSort(filtered, query)

  const page = query.page
  const pageSize = query.pageSize
  const start = (page - 1) * pageSize
  const rows = sorted.slice(start, start + pageSize)
  const now = new Date().toISOString()

  return {
    rows,
    pageInfo: { page, pageSize, total: filtered.length },
    metrics,
    permissionVersion: PERMISSION_VERSION,
    sourceAsOf: now,
    queriedAt: now,
    filterSummary: filterSummary(query, filtered.length),
  }
}

function projectCost(seed: SeedOrder, role: DemoRole): CostView {
  const visible = costVisible(role)
  if (!visible) {
    return {
      costMasked: true,
      cumulativeCostGross: null,
      cumulativeCostNet: null,
      costSource: seed.costs.costSource,
      costVariance: null,
      settlementId: seed.costs.settlementId,
      settlementNo: seed.costs.settlementNo,
      payableEntryLabel: seed.costs.payableEntryLabel,
    }
  }
  return {
    costMasked: false,
    cumulativeCostGross: seed.costs.cumulativeCostGross,
    cumulativeCostNet: seed.costs.cumulativeCostNet,
    costSource: seed.costs.costSource,
    costVariance: seed.costs.costVariance ?? null,
    settlementId: seed.costs.settlementId,
    settlementNo: seed.costs.settlementNo,
    payableEntryLabel: seed.costs.payableEntryLabel,
  }
}

function projectDetail(
  seed: SeedOrder,
  role: DemoRole
): SupplierOrderDetailView {
  const session = getSession(seed.orderId)
  const fulfillment = session.fulfillmentStatus ?? seed.fulfillmentStatus
  const cancel = session.cancelStatus ?? seed.cancelStatus
  const refund = session.refundStatus ?? seed.refundStatus
  const { allowed, blockers } = computeAllowedActions(seed, session, role)
  const cost = projectCost(seed, role)
  const canReveal = sensitiveAllowed(role)
  const reveal = revealSessions.get(seed.orderId)
  const revealActive = reveal && reveal.expiresAt > Date.now()

  const items = seed.items.map((item) => ({
    ...item,
    unitCostGross: cost.costMasked ? null : item.unitCostGross,
    unitCostNet: cost.costMasked ? null : item.unitCostNet,
    inputTaxRate: cost.costMasked ? null : item.inputTaxRate,
    snapshotImmutable: true as const,
  }))

  const afterSales = seed.afterSales.map((as) => {
    const patch = session.afterSalesPatches?.[as.requestId]
    const cancelStatus = patch?.cancelStatus ?? as.cancelStatus
    const supplierRefundStatus =
      patch?.supplierRefundStatus ?? as.supplierRefund.status
    return {
      ...as,
      cancelStatus,
      cancelLabel: CANCEL_STATUS_LABEL[cancelStatus],
      supplierRefund: {
        ...as.supplierRefund,
        status: supplierRefundStatus,
        statusLabel: REFUND_STATUS_LABEL[supplierRefundStatus],
        amount: cost.costMasked ? null : as.supplierRefund.amount,
      },
      mallRefund: {
        ...as.mallRefund,
        amount: cost.costMasked ? null : as.mallRefund.amount,
      },
    }
  })

  const actions = [
    ...seed.actions.map((a) => ({
      ...a,
      techSummary: techVisible(role) ? a.techSummary : undefined,
    })),
    ...session.actionsExtra.map((a) => ({
      ...a,
      techSummary: techVisible(role) ? a.techSummary : undefined,
    })),
  ].sort((a, b) => b.at.localeCompare(a.at))

  let workItem: WorkItemView | undefined
  if (seed.workItem) {
    workItem = {
      workItemId: seed.workItem.workItemId,
      workItemType: seed.workItem.workItemType,
      businessObjectType: "SUPPLIER_FULFILLMENT_ORDER",
      businessObjectId: seed.orderId,
      subjectVersion: session.subjectVersion ?? seed.workItem.subjectVersion,
      subjectHash: session.subjectHash ?? seed.workItem.subjectHash,
      completionAction: seed.workItem.completionAction,
      allowedTaskActions: seed.workItem.allowedTaskActions,
      workItemStatus: session.workItemStatus ?? "PENDING",
      claimedBy:
        session.workItemStatus === "IN_PROGRESS"
          ? { userId: "u-demo", displayName: "当前处理人" }
          : undefined,
      held: session.workItemHeld,
    }
  }

  return {
    order: {
      id: seed.orderId,
      orderNo: seed.orderNo,
      mallOrderId: seed.mallOrderId,
      mallOrderNo: seed.mallOrderNo,
      paidAt: seed.paidAt,
      paymentFactKey: seed.paymentFactKey,
      fulfillmentChain: "ERP_AUTOMATED",
      supplierId: seed.supplierId,
      supplierName: seed.supplierName,
      connectionCode: seed.connectionCode,
      connectionEnvironment: seed.connectionEnvironment,
      supplyVersion: seed.supplyVersion,
      publicationVersion: seed.publicationVersion,
      externalOrderNo: session.externalOrderNo ?? seed.externalOrderNo,
      fulfillmentStatus: fulfillment,
      fulfillmentLabel: FULFILLMENT_STATUS_LABEL[fulfillment],
      fulfillmentTone: FULFILLMENT_STATUS_TONE[fulfillment],
      cancelStatus: cancel,
      cancelLabel: CANCEL_STATUS_LABEL[cancel],
      cancelTone: CANCEL_STATUS_TONE[cancel],
      refundStatus: refund,
      refundLabel: REFUND_STATUS_LABEL[refund],
      refundTone: REFUND_STATUS_TONE[refund],
      lockVersion: session.lockVersion,
      paymentOccurredNotice: PAYMENT_OCCURRED_NOTICE,
      errorSummary: seed.errorSummary,
    },
    items,
    logistics: seed.logistics,
    statusHistory: seed.statusHistory,
    afterSales,
    costs: cost,
    actions,
    address: {
      masked: seed.address.masked,
      revealed:
        revealActive && canReveal ? seed.address.full : undefined,
      phoneMasked: seed.address.phoneMasked,
      phoneRevealed:
        revealActive && canReveal ? seed.address.phoneFull : undefined,
      recipientMasked: seed.address.recipientMasked,
      recipientRevealed:
        revealActive && canReveal ? seed.address.recipientFull : undefined,
      canReveal,
      revealExpiresAt: revealActive
        ? new Date(reveal!.expiresAt).toISOString()
        : undefined,
      auditNote: revealActive
        ? `已记录揭示审计（原因：${reveal!.reason}）`
        : undefined,
    },
    workItem,
    lastInvestigation: session.lastInvestigation,
    placeActionId: seed.placeActionId,
    allowedActions: allowed,
    actionBlockers: blockers,
    freshness: {
      updatedAt: seed.updatedAt,
      state: "fresh",
    },
    role,
  }
}

export async function fetchSupplierOrderDetail(input: {
  orderId: string
  role?: DemoRole
}): Promise<SupplierOrderDetailView | null> {
  await mockDelay(60)
  const seed = findSeed(input.orderId)
  if (!seed) return null
  return projectDetail(seed, input.role ?? "procurement")
}

export async function querySupplierResult(
  input: QueryResultInput
): Promise<FormalActionResponse<QueryResultData>> {
  await mockDelay(120)

  const existing = investigationIdem.get(input.idempotencyKey)
  if (existing) return existing as FormalActionResponse<QueryResultData>

  const seed = findSeed(input.orderId)
  if (!seed) {
    return { status: "failed", message: "订单不存在" }
  }
  const session = getSession(input.orderId)
  if (input.expectedLockVersion !== session.lockVersion) {
    return {
      status: "blocked",
      message: "订单版本已变化，请刷新后重试查询",
    }
  }

  if (seed.hasQueryCapability === false) {
    return {
      status: "blocked",
      message: "供应商无查询能力，请进入接口错误与对账中心",
      reference: "W29",
    }
  }

  if (input.simulateUnknown) {
    const res: FormalActionResponse<QueryResultData> = {
      status: "unknown",
      message: "查询请求超时，结果未知。请按原任务号查询，勿重复提交。",
      operationId: input.operationId,
    }
    investigationIdem.set(input.idempotencyKey, res)
    return res
  }

  const preset = seed.queryPreset ?? "STILL_UNKNOWN"
  let outcome: InvestigationEvidenceView["outcome"]
  let canSafeRetry = false
  let outcomeLabel: string
  let summary: string
  let externalOrderNo: string | undefined

  if (preset === "VERIFIED_NO_RESULT") {
    outcome = "VERIFIED_NO_RESULT"
    canSafeRetry = Boolean(seed.canSafeRetryAfterNoResult)
    outcomeLabel = "明确无结果"
    summary = canSafeRetry
      ? "供应商确认无对应订单，系统允许沿用原任务号重新提交。"
      : "供应商确认无对应订单，但当前不可安全重试。"
  } else if (preset === "VERIFIED_TERMINAL") {
    outcome = "VERIFIED_TERMINAL"
    canSafeRetry = false
    outcomeLabel = "已取得处理结果"
    externalOrderNo = seed.externalOrderNo ?? "EXT-VERIFIED-001"
    summary = "已查到供应商侧已有处理结果，不得重发。"
  } else {
    outcome = "RESULT_UNKNOWN"
    canSafeRetry = false
    outcomeLabel = "仍未知"
    summary = "查询后仍无法确认供应商侧结果，请继续查询或转人工。"
  }

  const evidence: InvestigationEvidenceView = {
    evidenceId: `evq_${input.orderId}_${Date.now()}`,
    targetSupplierActionId: input.targetSupplierActionId,
    outcome,
    outcomeLabel,
    recordedAt: new Date().toISOString(),
    canSafeRetry,
    externalOrderNo,
    summary,
  }

  session.lockVersion += 1
  session.lastInvestigation = evidence
  session.actionsExtra = [
    ...session.actionsExtra,
    {
      actionId: `act-query-${Date.now()}`,
      actionType: "QUERY_RESULT",
      actionLabel: "查询原结果",
      at: evidence.recordedAt,
      actor: "当前用户",
      outcomeLabel,
      outcomeTone:
        outcome === "VERIFIED_NO_RESULT"
          ? "success"
          : outcome === "VERIFIED_TERMINAL"
            ? "success"
            : "warning",
      idempotencyKeyTail: `…${input.idempotencyKey.slice(-6)}`,
      attemptCount: 1,
      operationId: input.operationId,
    },
  ]

  // Task stays non-terminal
  if (input.workItemId && seed.workItem) {
    session.workItemStatus = "IN_PROGRESS"
    session.subjectHash = `sh_${seed.orderId}_v${session.lockVersion}`
    session.subjectVersion = String(session.lockVersion)
  }

  if (outcome === "VERIFIED_TERMINAL" && externalOrderNo) {
    session.externalOrderNo = externalOrderNo
    // do not auto-complete task; do not flip to success unless evidence says so
    // keep RESULT_UNKNOWN until explicit confirm — mock: stay RESULT_UNKNOWN
  }

  const { allowed, blockers } = computeAllowedActions(
    seed,
    session,
    "procurement"
  )

  const data: QueryResultData = {
    evidence,
    lockVersion: session.lockVersion,
    workItemStatus: input.workItemId ? "IN_PROGRESS" : undefined,
    subjectHash: session.subjectHash,
    allowedActions: allowed,
    actionBlockers: blockers,
  }

  const res: FormalActionResponse<QueryResultData> = {
    status: "succeeded",
    message: `查询完成：${outcomeLabel}。任务仍待处理，不会自动下一项。`,
    reference: `查询记录 QR-${seed.orderNo.slice(-4)}-${Date.now()
      .toString()
      .slice(-4)}`,
    operationId: input.operationId,
    data,
  }
  investigationIdem.set(input.idempotencyKey, res)
  return res
}

export async function replaySupplierOrder(
  input: ReplayInput
): Promise<FormalActionResponse<ReplayResultData>> {
  await mockDelay(150)

  const existing = investigationIdem.get(input.idempotencyKey)
  if (existing) return existing as FormalActionResponse<ReplayResultData>

  const seed = findSeed(input.orderId)
  if (!seed) return { status: "failed", message: "订单不存在" }
  const session = getSession(input.orderId)

  if (input.expectedLockVersion !== session.lockVersion) {
    return {
      status: "blocked",
      message: "订单版本已变化，请刷新后重试",
    }
  }

  const inv = session.lastInvestigation
  if (!inv || inv.outcome !== "VERIFIED_NO_RESULT" || !inv.canSafeRetry) {
    return {
      status: "blocked",
      message:
        "仅当确认无结果且系统判定可安全重试时才可重发。请先查询原结果。",
    }
  }

  const externalOrderNo = `EXT-REPLAY-${seed.orderNo.slice(-4)}`
  const now = new Date().toISOString()
  session.lockVersion += 1
  session.externalOrderNo = externalOrderNo
  session.fulfillmentStatus = "ACCEPTED"
  session.placeAttemptCount = (session.placeAttemptCount ?? 1) + 1

  const evidence: InvestigationEvidenceView = {
    evidenceId: `evr_${input.orderId}_${Date.now()}`,
    targetSupplierActionId: input.targetSupplierActionId,
    outcome: "VERIFIED_TERMINAL",
    outcomeLabel: "重发已接单",
    recordedAt: now,
    canSafeRetry: false,
    externalOrderNo,
    summary: "沿用原任务号重新提交成功，已取得外部单号。任务处理结果待确认。",
  }
  session.lastInvestigation = evidence
  session.actionsExtra = [
    ...session.actionsExtra,
    {
      actionId: `act-replay-${Date.now()}`,
      actionType: "REPLAY",
      actionLabel: "安全重发",
      at: now,
      actor: "当前用户",
      outcomeLabel: "已接单",
      outcomeTone: "success",
      idempotencyKeyTail: `…${input.idempotencyKey.slice(-6)}`,
      attemptCount: session.placeAttemptCount,
      operationId: input.operationId,
      techSummary: "replay with original place idempotency key (server-side)",
    },
  ]

  if (input.workItemId) {
    session.workItemStatus = "IN_PROGRESS"
    session.subjectHash = `sh_${seed.orderId}_v${session.lockVersion}`
  }

  const { allowed, blockers } = computeAllowedActions(
    seed,
    session,
    "procurement"
  )

  const data: ReplayResultData = {
    evidence,
    lockVersion: session.lockVersion,
    workItemStatus: input.workItemId ? "IN_PROGRESS" : undefined,
    externalOrderNo,
    fulfillmentStatus: "ACCEPTED",
    allowedActions: allowed,
    actionBlockers: blockers,
  }

  const res: FormalActionResponse<ReplayResultData> = {
    status: "succeeded",
    message:
      "重发已受理并取得接单结果。任务仍在处理中，需确认处理结果。",
    reference: `重发记录 RR-${seed.orderNo.slice(-4)}-${Date.now()
      .toString()
      .slice(-4)}`,
    operationId: input.operationId,
    data,
  }
  investigationIdem.set(input.idempotencyKey, res)
  return res
}

export async function deferSupplierOrderTask(
  input: DeferTaskInput
): Promise<FormalActionResponse<DeferTaskResult>> {
  await mockDelay(80)
  const seed = findSeed(input.orderId)
  if (!seed?.workItem) {
    return { status: "failed", message: "无关联任务，无法跳过" }
  }
  const session = getSession(input.orderId)
  session.workItemHeld = true
  session.workItemStatus = "PENDING"
  session.notes.push(
    `跳过：${input.reasonCode}${input.comment ? ` · ${input.comment}` : ""}`
  )
  session.actionsExtra = [
    ...session.actionsExtra,
    {
      actionId: `act-defer-${Date.now()}`,
      actionType: "NOTE",
      actionLabel: "本轮跳过",
      at: new Date().toISOString(),
      actor: "当前用户",
      outcomeLabel: "任务仍待处理",
      outcomeTone: "info",
      idempotencyKeyTail: `…${input.idempotencyKey.slice(-6)}`,
      attemptCount: 1,
    },
  ]

  return {
    status: "succeeded",
    message: "已记录跳过原因。任务未完成、未转交，仍为待处理。",
    reference: `跳过记录 DF-${seed.orderNo.slice(-4)}-${Date.now()
      .toString()
      .slice(-4)}`,
    data: {
      reasonCode: input.reasonCode,
      queueContextId: input.queueContextId,
      leaseDisposition: "RELEASED",
      nextQueueCursor: undefined,
      workItemStatus: "PENDING",
    },
  }
}

export async function submitAfterSalesAction(
  input: AfterSalesActionInput
): Promise<FormalActionResponse<AfterSalesActionResult>> {
  await mockDelay(100)
  const seed = findSeed(input.orderId)
  if (!seed) return { status: "failed", message: "订单不存在" }
  const session = getSession(input.orderId)

  const existing = session.aftersaleIdempotency.get(input.idempotencyKey)
  if (existing) {
    return {
      status: "succeeded",
      message: "重复提交返回原结果，未再次调用供应商",
      reference: existing.actionRecordId,
      data: existing,
    }
  }

  if (input.expectedLockVersion !== session.lockVersion) {
    return {
      status: "blocked",
      message: "订单版本已变化，请刷新后重试",
    }
  }

  const asReq = seed.afterSales.find(
    (a) => a.requestId === input.afterSalesRequestId
  )
  if (!asReq || !asReq.mallRequestRef) {
    return {
      status: "blocked",
      message: "必须引用既有商城售后请求，禁止脱离请求创建取消/退款",
    }
  }

  session.lockVersion += 1
  if (!session.afterSalesPatches) session.afterSalesPatches = {}
  const patch = session.afterSalesPatches[input.afterSalesRequestId] ?? {}

  let cancelStatus = session.cancelStatus ?? seed.cancelStatus
  let refundStatus = session.refundStatus ?? seed.refundStatus
  let note: string

  if (input.action === "CANCEL") {
    cancelStatus = "CANCEL_PENDING"
    session.cancelStatus = "CANCEL_PENDING"
    patch.cancelStatus = "CANCEL_PENDING"
    note = `已提交取消，引用售后 ${asReq.mallRequestRef}。领域动作不读写任务。`
  } else {
    refundStatus = "REFUND_PENDING"
    session.refundStatus = "REFUND_PENDING"
    patch.supplierRefundStatus = "REFUND_PENDING"
    note = `已提交退款，引用售后 ${asReq.mallRequestRef}。领域动作不读写任务。`
  }
  session.afterSalesPatches[input.afterSalesRequestId] = patch

  const actionRecordId = `act-as-${Date.now()}`
  session.actionsExtra = [
    ...session.actionsExtra,
    {
      actionId: actionRecordId,
      actionType: input.action,
      actionLabel: input.action === "CANCEL" ? "取消" : "退款",
      at: new Date().toISOString(),
      actor: "当前用户",
      outcomeLabel: "处理中",
      outcomeTone: "info",
      idempotencyKeyTail: `…${input.idempotencyKey.slice(-6)}`,
      attemptCount: 1,
      operationId: input.operationId,
    },
  ]

  const data: AfterSalesActionResult = {
    lockVersion: session.lockVersion,
    cancelStatus,
    refundStatus,
    actionRecordId,
    note,
  }
  session.aftersaleIdempotency.set(input.idempotencyKey, data)

  return {
    status: "succeeded",
    message: note,
    reference: `售后处理记录 AS-${seed.orderNo.slice(-4)}-${Date.now()
      .toString()
      .slice(-4)}`,
    operationId: input.operationId,
    data,
  }
}

export async function revealSupplierOrderAddress(
  input: RevealAddressInput
): Promise<FormalActionResponse<RevealAddressResult>> {
  await mockDelay(40)
  const seed = findSeed(input.orderId)
  if (!seed) return { status: "failed", message: "订单不存在" }

  const auditEventId = `aud-reveal-${Date.now()}`
  const expiresAt = Date.now() + 5 * 60_000
  revealSessions.set(input.orderId, {
    expiresAt,
    reason: input.reason,
    auditEventId,
  })

  const detail = projectDetail(seed, "procurement")
  return {
    status: "succeeded",
    message: "已短时揭示收货信息，并写入审计。离开页面或超时后自动清除。",
    data: {
      address: detail.address,
      auditEventId,
    },
  }
}

export async function clearAddressReveal(orderId: string): Promise<void> {
  revealSessions.delete(orderId)
}

export async function addCollaborationNote(
  input: NoteInput
): Promise<FormalActionResponse<{ lockVersion: number }>> {
  await mockDelay(50)
  const seed = findSeed(input.orderId)
  if (!seed) return { status: "failed", message: "订单不存在" }
  const session = getSession(input.orderId)
  if (input.expectedLockVersion !== session.lockVersion) {
    return {
      status: "blocked",
      message: "数据已更新，协同说明未写入。请刷新后重试。",
    }
  }
  session.lockVersion += 1
  session.notes.push(input.comment)
  session.actionsExtra = [
    ...session.actionsExtra,
    {
      actionId: `act-note-${Date.now()}`,
      actionType: "NOTE",
      actionLabel: "协同说明",
      at: new Date().toISOString(),
      actor: "当前用户",
      outcomeLabel: "已记录",
      outcomeTone: "neutral",
      idempotencyKeyTail: `…${input.idempotencyKey.slice(-6)}`,
      attemptCount: 1,
    },
  ]
  return {
    status: "succeeded",
    message: "已追加协同说明，未改变订单状态。",
    data: { lockVersion: session.lockVersion },
  }
}

export function listSupplierOptions(): { id: string; name: string }[] {
  const map = new Map<string, string>()
  for (const s of SUPPLIER_ORDER_SEEDS) {
    map.set(s.supplierId, s.supplierName)
  }
  return [...map.entries()].map(([id, name]) => ({ id, name }))
}

export async function createSupplierOrderExportJob(
  command: ExportCommand
): Promise<ExportJobResult> {
  await mockDelay(120)
  const jobId = `exp-w26-${command.requestId.slice(-8)}`
  const expiresAt = new Date(
    Date.now() + 7 * 24 * 60 * 60 * 1000
  ).toISOString()
  return {
    jobId,
    requestId: command.requestId,
    rowCount: command.rowCount,
    permissionVersion: PERMISSION_VERSION,
    fieldSetId: command.fieldSetId,
    maskDisclaimer:
      "导出使用系统筛选快照与字段权限打码：收货地址、手机号不会以明文写入文件，导出默认列不含敏感地址；下载时重新鉴权，结果 7 天内可下载。",
    expiresAt,
    downloadLabel: `供应商订单_${new Date().toISOString().slice(0, 10)}.csv`,
    status: "succeeded",
  }
}

export function formatCostDisplay(
  value: string | null | undefined,
  masked: boolean
): string {
  if (masked || value == null) return COST_MASK
  return value
}

/** test helper */
export function __resetSupplierOrderSessions() {
  sessions.clear()
  revealSessions.clear()
  investigationIdem.clear()
}
