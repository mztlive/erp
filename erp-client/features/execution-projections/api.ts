/**
 * W23 · 销售单执行投影 · 真实 HTTP 适配层。
 * 保持对外导出签名稳定；白名单字段以外不组装商业字段。
 */

import { apiGet, apiPost, type Page } from "@/lib/api"
import type {
  BulkItemOutcome,
  BulkProjectionJob,
  DeliveryStatus,
  ExecutionProjectionListQuery,
  ExecutionProjectionListResult,
  ExecutionProjectionMetric,
  ExecutionProjectionRow,
  ExecutionProjectionView,
  ProjectionDeliveryCommandResult,
  ProjectionSource,
  ProjectionWhitelistContent,
  SalesOrderCollaborationSummary,
} from "@/features/execution-projections/types"
import {
  DELIVERY_STATUS_LABEL,
  DELIVERY_STATUS_TONE,
  LATENCY_LABEL,
  RECONCILIATION_LABEL,
  SOURCE_LABEL,
} from "@/features/execution-projections/types"

/** 批量操作上限（前端选择条需同步提示，超限禁用批量按钮） */
export const BULK_SELECTION_LIMIT = 20

// ─── Backend wire types ───────────────────────────────────────────────────────

type BackendProjection = {
  id: string
  sales_order_id: string
  target_mall_id: string
  current_acked_revision_id?: string | null
  version: number
  created_at: number
}

type BackendRevision = {
  id: string
  projection_id: string
  revision_no: number
  projection_source: string
  sales_order_revision_id: string
  customer_external_identity: string
  face_value: string
  card_count: number
  card_form: string
  effective_at: number
  version: number
  created_at: number
}

type BackendDelivery = {
  id: string
  projection_revision_id: string
  target_mall_id: string
  status: string
  attempt_count: number
  mall_ack_at?: number | null
  error_code?: string | null
  version: number
  created_at: number
}

type BackendDeliveryResult = {
  delivery_id: string
  delivery_status: string
  inbox_message_id: string
  error_task_id?: string | null
  mall_execution_baseline?: string | null
  projection_version: number
}

type SourceSystem = {
  id: string
  code: string
  name: string
  system_type?: string
}

// ─── In-memory bulk jobs (no backend bulk endpoint) ───────────────────────────

const bulkJobs = new Map<string, BulkProjectionJob>()

// ─── Helpers ──────────────────────────────────────────────────────────────────

function secsToIso(secs?: number | null): string {
  if (secs == null || secs <= 0) return new Date(0).toISOString()
  return new Date(secs * 1000).toISOString()
}

function mapDeliveryStatus(raw: string): DeliveryStatus {
  switch (raw) {
    case "pending_send":
      return "PENDING"
    case "sending":
      return "SENDING"
    case "retrying":
      return "RETRYING"
    case "confirmed":
      return "ACKED"
    case "failed":
      return "FAILED"
    case "manual":
      return "ESCALATED_MANUAL"
    default:
      return "PENDING"
  }
}

function mapSource(raw: string): ProjectionSource {
  if (raw === "cutover_snapshot") return "MIGRATION_BASELINE"
  return "ERP_SALES_REVISION"
}

function mapCardForm(raw: string): string {
  if (raw === "electronic") return "电子卡"
  if (raw === "physical") return "实体卡"
  return raw
}

function recomputeActions(status: DeliveryStatus): {
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
} {
  const blockers: Array<{ action: string; code: string; message: string }> = []
  const allowed: string[] = []

  if (status === "UNKNOWN" || status === "SENDING" || status === "RETRYING") {
    allowed.push("QUERY_RESULT")
  }
  if (status === "FAILED") {
    allowed.push("RETRY", "QUERY_RESULT", "ESCALATE")
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

  return { allowedActions: [...new Set(allowed)], actionBlockers: blockers }
}

function whitelistFromRevision(rev: BackendRevision): ProjectionWhitelistContent {
  return {
    customerExternalIdentity: rev.customer_external_identity,
    customerExternalIdentityCopyable: false,
    voucherCategoryExternalIdentity: "—",
    voucherCategoryErpName: "—",
    voucherExpiryAt: "—",
    faceValue: String(rev.face_value),
    cardCount: String(rev.card_count),
    cardForm: mapCardForm(rev.card_form),
    effectiveAt: secsToIso(rev.effective_at),
    contentHash: rev.id,
  }
}

async function loadMalls(): Promise<Array<{ id: string; name: string }>> {
  try {
    const page = await apiGet<Page<SourceSystem>>("/admin/source-systems", {
      page: 1,
      page_size: 100,
      system_type: "MALL",
    })
    return page.items.map((s) => ({ id: s.id, name: s.name }))
  } catch {
    return []
  }
}

function mallName(
  malls: Array<{ id: string; name: string }>,
  id: string
): string {
  return malls.find((m) => m.id === id)?.name ?? id
}

function computeMetrics(
  rows: ExecutionProjectionRow[]
): ExecutionProjectionMetric[] {
  return [
    {
      key: "pending_send",
      label: "待发送",
      value: rows.filter((r) => r.delivery.status === "PENDING").length,
    },
    {
      key: "inflight",
      label: "发送中",
      value: rows.filter((r) =>
        ["SENDING", "RETRYING", "UNKNOWN"].includes(r.delivery.status)
      ).length,
    },
    {
      key: "timeout",
      label: "已超时",
      value: rows.filter((r) => r.latencyBand === "over_sla").length,
    },
    {
      key: "fail_manual",
      label: "失败/转人工",
      value: rows.filter((r) =>
        ["FAILED", "ESCALATED_MANUAL"].includes(r.delivery.status)
      ).length,
    },
    {
      key: "acked",
      label: "已确认",
      value: rows.filter((r) => r.delivery.status === "ACKED").length,
    },
  ]
}

function filterSummary(query: ExecutionProjectionListQuery): string {
  const parts: string[] = []
  if (query.mallId) parts.push(`商城=${query.mallId}`)
  if (query.deliveryStatus) parts.push(`状态=${query.deliveryStatus}`)
  if (query.source && query.source !== "all") {
    parts.push(`来源=${SOURCE_LABEL[query.source as ProjectionSource] ?? query.source}`)
  }
  if (query.latency && query.latency !== "all") {
    parts.push(`等待时长=${LATENCY_LABEL[query.latency]}`)
  }
  if (query.reconciliation && query.reconciliation !== "all") {
    parts.push(
      `对账=${RECONCILIATION_LABEL[query.reconciliation] ?? query.reconciliation}`
    )
  }
  if (query.q?.trim()) parts.push(`搜索=${query.q.trim()}`)
  return parts.length ? parts.join(" · ") : "默认：风险优先 · 全状态"
}

function toRow(
  proj: BackendProjection,
  rev: BackendRevision | undefined,
  delivery: BackendDelivery | undefined,
  malls: Array<{ id: string; name: string }>
): ExecutionProjectionRow {
  const status = delivery
    ? mapDeliveryStatus(delivery.status)
    : ("PENDING" as DeliveryStatus)
  const actions = recomputeActions(status)
  const content = rev
    ? whitelistFromRevision(rev)
    : {
        customerExternalIdentity: "—",
        customerExternalIdentityCopyable: false,
        voucherCategoryExternalIdentity: "—",
        voucherCategoryErpName: "—",
        voucherExpiryAt: "—",
        faceValue: "0",
        cardCount: "0",
        cardForm: "—",
        effectiveAt: secsToIso(proj.created_at),
        contentHash: proj.id,
      }

  return {
    projectionId: proj.id,
    projectionNo: proj.id.slice(0, 12).toUpperCase(),
    projectionRevisionId: rev?.id ?? "",
    projectionRevisionNo: rev?.revision_no ?? 0,
    projectionSource: rev
      ? mapSource(rev.projection_source)
      : "ERP_SALES_REVISION",
    salesOrderId: proj.sales_order_id,
    salesOrderNo: proj.sales_order_id,
    salesOrderRevisionId: rev?.sales_order_revision_id ?? "",
    salesOrderRevisionNo: rev?.revision_no ?? 0,
    salesOrderStatus: "—",
    salesOrderStatusTone: "neutral",
    customerLabel: content.customerExternalIdentity,
    targetMallId: proj.target_mall_id,
    targetMallName: mallName(malls, proj.target_mall_id),
    currentAckedRevisionNo: undefined,
    delivery: {
      deliveryId: delivery?.id ?? `dlv_${proj.id}`,
      status,
      statusLabel: DELIVERY_STATUS_LABEL[status],
      statusTone: DELIVERY_STATUS_TONE[status],
      attemptCount: delivery?.attempt_count ?? 0,
      lastAttemptAt: delivery ? secsToIso(delivery.created_at) : undefined,
      mallAckAt: delivery?.mall_ack_at
        ? secsToIso(delivery.mall_ack_at)
        : undefined,
      errorCode: delivery?.error_code ?? undefined,
      errorSummary: delivery?.error_code ?? undefined,
      errorTaskId: undefined,
    },
    latencyBand: "normal",
    reconciliationStatus: "NONE",
    pendingDurationLabel: "—",
    ownerLabel: "—",
    allowedActions: actions.allowedActions,
    actionBlockers: actions.actionBlockers,
    objectVersion: String(proj.version),
    whitelistPreview: {
      voucherCategoryErpName: content.voucherCategoryErpName,
      faceValue: content.faceValue,
      cardCount: content.cardCount,
      cardForm: content.cardForm,
      voucherExpiryAt: content.voucherExpiryAt,
    },
  }
}

// ─── Public API ───────────────────────────────────────────────────────────────

export async function fetchExecutionProjectionList(
  query: ExecutionProjectionListQuery = {}
): Promise<ExecutionProjectionListResult> {
  const malls = await loadMalls()
  const page = Math.max(1, query.page ?? 1)
  const pageSize = Math.min(50, Math.max(1, query.pageSize ?? 20))

  const listQuery: Record<string, unknown> = {
    page,
    page_size: pageSize,
    sort_by: "updated_at",
    sort_dir: "desc",
  }
  if (query.mallId) listQuery.target_mall_id = query.mallId

  const pageResult = await apiGet<Page<BackendProjection>>(
    "/admin/sales-order-projections",
    listQuery
  )

  const deliveryPage = await apiGet<Page<BackendDelivery>>(
    "/admin/sales-order-projection-deliveries",
    {
      page: 1,
      page_size: 100,
      target_mall_id: query.mallId || undefined,
    }
  ).catch(() => ({
    items: [] as BackendDelivery[],
    total: 0,
    page: 1,
    page_size: 100,
  }))

  const rows: ExecutionProjectionRow[] = []
  for (const proj of pageResult.items) {
    const revisions = await apiGet<BackendRevision[]>(
      `/admin/sales-order-projections/${encodeURIComponent(proj.id)}/revisions`
    ).catch(() => [] as BackendRevision[])
    const latest = revisions[0]
    const delivery = deliveryPage.items.find(
      (d) => d.projection_revision_id === latest?.id
    )
    rows.push(toRow(proj, latest, delivery, malls))
  }

  let filtered = rows
  if (query.q?.trim()) {
    const q = query.q.trim().toUpperCase()
    filtered = filtered.filter(
      (r) =>
        r.projectionNo.toUpperCase().includes(q) ||
        r.salesOrderNo.toUpperCase().includes(q) ||
        r.customerLabel.toUpperCase().includes(q) ||
        r.targetMallName.toUpperCase().includes(q)
    )
  }
  if (query.deliveryStatus) {
    const statuses = query.deliveryStatus
      .split(",")
      .map((s) => s.trim().toUpperCase())
    filtered = filtered.filter((r) => statuses.includes(r.delivery.status))
  }
  if (query.source && query.source !== "all") {
    filtered = filtered.filter((r) => r.projectionSource === query.source)
  }
  if (query.latency && query.latency !== "all") {
    filtered = filtered.filter((r) => r.latencyBand === query.latency)
  }
  if (query.reconciliation && query.reconciliation !== "all") {
    filtered = filtered.filter(
      (r) => r.reconciliationStatus === query.reconciliation
    )
  }
  if (query.metric && query.metric !== "all") {
    if (query.metric === "pending_send") {
      filtered = filtered.filter((r) => r.delivery.status === "PENDING")
    } else if (query.metric === "inflight") {
      filtered = filtered.filter((r) =>
        ["SENDING", "RETRYING", "UNKNOWN"].includes(r.delivery.status)
      )
    } else if (query.metric === "timeout") {
      filtered = filtered.filter((r) => r.latencyBand === "over_sla")
    } else if (query.metric === "fail_manual") {
      filtered = filtered.filter((r) =>
        ["FAILED", "ESCALATED_MANUAL"].includes(r.delivery.status)
      )
    } else if (query.metric === "acked") {
      filtered = filtered.filter((r) => r.delivery.status === "ACKED")
    }
  }

  const asOf = secsToIso(
    Math.max(0, ...pageResult.items.map((p) => p.created_at))
  )

  return {
    rows: filtered,
    pageInfo: {
      page: pageResult.page,
      pageSize: pageResult.page_size,
      total: pageResult.total,
    },
    metrics: computeMetrics(filtered),
    malls,
    permissionVersion: "pv-live",
    sourceFactsAsOf: asOf,
    projectionUpdatedAt: asOf,
    deliveryStatusUpdatedAt: asOf,
    queriedAt: asOf,
    filterSummary: filterSummary(query),
    defaultViewNote:
      "运营默认关注未确认与失败；结果未知不计入已确认指标。",
  }
}

export async function fetchExecutionProjectionDetail(input: {
  projectionId: string
  revisionId?: string
}): Promise<ExecutionProjectionView | null> {
  const malls = await loadMalls()

  let proj: BackendProjection
  try {
    proj = await apiGet<BackendProjection>(
      `/admin/sales-order-projections/${encodeURIComponent(input.projectionId)}`
    )
  } catch (err) {
    const e = err as { kind?: string; status?: number }
    if (e?.kind === "Http" && e.status === 404) return null
    throw err
  }

  const revisions = await apiGet<BackendRevision[]>(
    `/admin/sales-order-projections/${encodeURIComponent(input.projectionId)}/revisions`
  ).catch(() => [] as BackendRevision[])

  const selected =
    revisions.find((r) => r.id === input.revisionId) ?? revisions[0]

  const deliveryPage = await apiGet<Page<BackendDelivery>>(
    "/admin/sales-order-projection-deliveries",
    {
      page: 1,
      page_size: 100,
      target_mall_id: proj.target_mall_id,
    }
  ).catch(() => ({
    items: [] as BackendDelivery[],
    total: 0,
    page: 1,
    page_size: 100,
  }))

  const revIds = new Set(revisions.map((r) => r.id))
  const deliveries = deliveryPage.items
    .filter((d) => revIds.has(d.projection_revision_id))
    .map((d) => {
      const st = mapDeliveryStatus(d.status)
      return {
        deliveryId: d.id,
        status: st,
        statusLabel: DELIVERY_STATUS_LABEL[st],
        statusTone: DELIVERY_STATUS_TONE[st],
        attemptCount: d.attempt_count,
        lastAttemptAt: secsToIso(d.created_at),
        mallAckAt: d.mall_ack_at ? secsToIso(d.mall_ack_at) : undefined,
        errorCode: d.error_code ?? undefined,
        errorSummary: d.error_code ?? undefined,
      }
    })

  const primaryDelivery = deliveries[0]
  const status = primaryDelivery?.status ?? "PENDING"
  const actions = recomputeActions(status)
  const content = selected
    ? whitelistFromRevision(selected)
    : {
        customerExternalIdentity: "—",
        customerExternalIdentityCopyable: false,
        voucherCategoryExternalIdentity: "—",
        voucherCategoryErpName: "—",
        voucherExpiryAt: "—",
        faceValue: "0",
        cardCount: "0",
        cardForm: "—",
        effectiveAt: secsToIso(proj.created_at),
        contentHash: proj.id,
      }

  const asOf = secsToIso(proj.created_at)

  return {
    identity: {
      projectionId: proj.id,
      projectionNo: proj.id.slice(0, 12).toUpperCase(),
      salesOrderId: proj.sales_order_id,
      salesOrderNo: proj.sales_order_id,
      targetMallId: proj.target_mall_id,
      targetMallName: mallName(malls, proj.target_mall_id),
    },
    tracks: {
      salesFact: {
        label: "销售事实",
        tone: "info",
        description: selected
          ? `ERP 销售版本 ${selected.sales_order_revision_id} 已形成；接收失败不回退销售记录或应收。`
          : "尚无投影修订。",
      },
      projectionDelivery: {
        label: DELIVERY_STATUS_LABEL[status],
        tone: DELIVERY_STATUS_TONE[status],
        description:
          status === "ACKED"
            ? "信息发送已完成"
            : `尝试 ${primaryDelivery?.attemptCount ?? 0} 次`,
      },
      mallConfirm: {
        label: status === "ACKED" ? "已确认" : "尚未确认",
        tone: status === "ACKED" ? "success" : "neutral",
        description: primaryDelivery?.mallAckAt
          ? `商城确认时间 ${primaryDelivery.mallAckAt}`
          : "尚无明确商城确认时间",
      },
    },
    selectedRevision: {
      projectionRevisionId: selected?.id ?? "",
      revisionNo: selected?.revision_no ?? 0,
      projectionSource: selected
        ? mapSource(selected.projection_source)
        : "ERP_SALES_REVISION",
      salesOrderRevisionId: selected?.sales_order_revision_id ?? "",
      salesOrderRevisionNo: selected?.revision_no ?? 0,
      content,
    },
    revisionLinks: revisions.map((r) => {
      const d = deliveries.find((x) => x.deliveryId && x)
      const ds =
        deliveryPage.items.find((x) => x.projection_revision_id === r.id)
          ?.status ?? "pending_send"
      const st = mapDeliveryStatus(ds)
      return {
        salesOrderRevisionId: r.sales_order_revision_id,
        salesOrderRevisionNo: r.revision_no,
        projectionRevisionId: r.id,
        projectionRevisionNo: r.revision_no,
        deliveryStatus: st,
        deliveryStatusLabel: DELIVERY_STATUS_LABEL[st],
        mallAckAt: d?.mallAckAt,
        sourceSalesRevisionNo: r.revision_no,
        isCurrentSelection: r.id === (selected?.id ?? ""),
      }
    }),
    deliveries,
    salesOrderStatus: "—",
    salesOrderStatusTone: "neutral",
    ownerLabel: "—",
    pendingDurationLabel: "—",
    latencyBand: "normal",
    reconciliationStatus: "NONE",
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
    objectVersion: String(proj.version),
    sourceFactsAsOf: asOf,
    projectionUpdatedAt: asOf,
    deliveryStatusUpdatedAt: asOf,
    queriedAt: asOf,
    boundaryNotice:
      "数据不是销售单副本。接收失败不回退销售记录、销售版本或应收；业务内容变更须在销售单走变更单形成新版本后自动产生新数据。",
  }
}

export async function fetchSalesOrderCollaboration(
  salesOrderId: string
): Promise<SalesOrderCollaborationSummary> {
  const page = await apiGet<Page<BackendProjection>>(
    "/admin/sales-order-projections",
    {
      sales_order_id: salesOrderId,
      page: 1,
      page_size: 10,
    }
  ).catch(() => ({
    items: [] as BackendProjection[],
    total: 0,
    page: 1,
    page_size: 10,
  }))

  if (page.items.length === 0) {
    return {
      salesOrderId,
      salesOrderNo: salesOrderId,
      hasProjection: false,
      historyCount: 0,
      note: "当前销售单尚无执行信息。卡券销售版本生效后由系统自动形成数据。",
    }
  }

  const proj = page.items[0]!
  const detail = await fetchExecutionProjectionDetail({
    projectionId: proj.id,
  })
  if (!detail) {
    return {
      salesOrderId,
      salesOrderNo: salesOrderId,
      hasProjection: true,
      projectionId: proj.id,
      projectionNo: proj.id.slice(0, 12).toUpperCase(),
      historyCount: 0,
      w23Href: `/commerce/execution-projections?projectionId=${encodeURIComponent(proj.id)}`,
      note: "已有投影身份，详情加载失败。",
    }
  }

  return {
    salesOrderId: detail.identity.salesOrderId,
    salesOrderNo: detail.identity.salesOrderNo,
    hasProjection: true,
    projectionId: detail.identity.projectionId,
    projectionNo: detail.identity.projectionNo,
    salesOrderRevisionNo: detail.selectedRevision.salesOrderRevisionNo,
    projectionRevisionNo: detail.selectedRevision.revisionNo,
    targetMallName: detail.identity.targetMallName,
    tracks: detail.tracks,
    delivery: detail.deliveries[0],
    whitelistPreview: {
      voucherCategoryErpName:
        detail.selectedRevision.content.voucherCategoryErpName,
      faceValue: detail.selectedRevision.content.faceValue,
      cardCount: detail.selectedRevision.content.cardCount,
      cardForm: detail.selectedRevision.content.cardForm,
      voucherExpiryAt: detail.selectedRevision.content.voucherExpiryAt,
    },
    currentAckedRevisionNo: detail.currentAckedRevisionNo,
    reconciliationStatus: detail.reconciliationStatus,
    historyCount: detail.revisionLinks.length,
    w23Href: `/commerce/execution-projections?projectionId=${encodeURIComponent(proj.id)}`,
    historyHref: `/commerce/execution-projections?projectionId=${encodeURIComponent(proj.id)}`,
    note: "投影字段仅含商城执行白名单，不含成交金额/配赠/税率/开票/应收。",
  }
}

export type DeliveryCommandInput = {
  projectionId: string
  projectionRevisionId: string
  deliveryId: string
  action: "QUERY_RESULT" | "RETRY" | "ESCALATE"
  expectedObjectVersion: string
  requestId: string
}

export async function submitProjectionDeliveryCommand(
  input: DeliveryCommandInput
): Promise<ProjectionDeliveryCommandResult> {
  if (input.action === "ESCALATE") {
    return {
      operationId: `op_esc_${input.requestId}`,
      deliveryId: input.deliveryId,
      projectionId: input.projectionId,
      salesOrderNo: input.projectionId,
      result: "ESCALATED",
      resultLabel: "已转人工",
      occurredAt: new Date().toISOString(),
      nextAction: "请到接口错误中心按原任务号处理。",
      stillUnknown: false,
      objectVersion: input.expectedObjectVersion,
    }
  }

  // QUERY_RESULT / RETRY：按修订号投递或查询投递列表
  const revisions = await apiGet<BackendRevision[]>(
    `/admin/sales-order-projections/${encodeURIComponent(input.projectionId)}/revisions`
  ).catch(() => [] as BackendRevision[])
  const latest =
    revisions.find((r) => r.id === input.projectionRevisionId) ?? revisions[0]
  if (!latest) {
    return {
      operationId: `op_${input.action}_${input.requestId}`,
      deliveryId: input.deliveryId,
      projectionId: input.projectionId,
      salesOrderNo: input.projectionId,
      result: "FAILED",
      resultLabel: "失败",
      occurredAt: new Date().toISOString(),
      nextAction: "无可投递的投影修订",
      stillUnknown: false,
      objectVersion: input.expectedObjectVersion,
    }
  }

  if (input.action === "QUERY_RESULT") {
    const deliveryPage = await apiGet<Page<BackendDelivery>>(
      "/admin/sales-order-projection-deliveries",
      { page: 1, page_size: 50 }
    ).catch(() => ({
      items: [] as BackendDelivery[],
      total: 0,
      page: 1,
      page_size: 50,
    }))
    const hit =
      deliveryPage.items.find((d) => d.id === input.deliveryId) ??
      deliveryPage.items.find(
        (d) => d.projection_revision_id === latest.id
      )
    if (!hit) {
      return {
        operationId: `op_query_${input.requestId}`,
        deliveryId: input.deliveryId,
        projectionId: input.projectionId,
        salesOrderNo: input.projectionId,
        result: "STILL_UNKNOWN",
        resultLabel: "结果未知",
        occurredAt: new Date().toISOString(),
        nextAction: "尚无投递记录，请稍后重试查询",
        stillUnknown: true,
        objectVersion: input.expectedObjectVersion,
      }
    }
    const st = mapDeliveryStatus(hit.status)
    if (st === "ACKED") {
      return {
        operationId: `op_query_${input.requestId}`,
        deliveryId: hit.id,
        projectionId: input.projectionId,
        salesOrderNo: input.projectionId,
        result: "ACKED",
        resultLabel: "已确认",
        occurredAt: secsToIso(hit.mall_ack_at ?? hit.created_at),
        nextAction: "无需进一步操作",
        stillUnknown: false,
        objectVersion: String(hit.version),
      }
    }
    if (st === "FAILED" || st === "ESCALATED_MANUAL") {
      return {
        operationId: `op_query_${input.requestId}`,
        deliveryId: hit.id,
        projectionId: input.projectionId,
        salesOrderNo: input.projectionId,
        result: "FAILED",
        resultLabel: DELIVERY_STATUS_LABEL[st],
        errorTaskId: undefined,
        occurredAt: secsToIso(hit.created_at),
        nextAction: "可重试或转人工",
        stillUnknown: false,
        objectVersion: String(hit.version),
      }
    }
    return {
      operationId: `op_query_${input.requestId}`,
      deliveryId: hit.id,
      projectionId: input.projectionId,
      salesOrderNo: input.projectionId,
      result: "STILL_UNKNOWN",
      resultLabel: "结果未知",
      occurredAt: secsToIso(hit.created_at),
      nextAction: "结果未明确，请继续查询",
      stillUnknown: true,
      objectVersion: String(hit.version),
    }
  }

  // RETRY
  const result = await apiPost<BackendDeliveryResult>(
    `/admin/sales-order-projections/${encodeURIComponent(input.projectionId)}/revisions/${latest.revision_no}/deliver`,
    { idempotency_key: input.requestId }
  )
  const st = mapDeliveryStatus(result.delivery_status)
  return {
    operationId: result.inbox_message_id,
    deliveryId: result.delivery_id,
    projectionId: input.projectionId,
    salesOrderNo: input.projectionId,
    result:
      st === "ACKED" ? "ACKED" : st === "FAILED" ? "FAILED" : "RETRY_SCHEDULED",
    resultLabel:
      st === "ACKED"
        ? "已确认"
        : st === "FAILED"
          ? "失败"
          : "已安排重试",
    errorTaskId: result.error_task_id ?? undefined,
    occurredAt: new Date().toISOString(),
    nextAction:
      st === "ACKED"
        ? "无需进一步操作"
        : "关注投递状态；结果未知时先查询",
    stillUnknown: false,
    objectVersion: String(result.projection_version),
  }
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
  if (!input.projectionIds.length) {
    return {
      jobId: `bulk_empty_${input.requestId}`,
      action: input.action,
      status: "failed",
      total: 0,
      completed: 0,
      succeeded: 0,
      skipped: 0,
      failed: 0,
      stillUnknown: 0,
      selectionSnapshotId: `snap-${input.requestId}`,
      items: [],
      startedAt: new Date().toISOString(),
      nextAction: "请先逐项显式勾选失败/可处理项",
    }
  }

  if (input.projectionIds.length > BULK_SELECTION_LIMIT) {
    return {
      jobId: `bulk_reject_${input.requestId}`,
      action: input.action,
      status: "failed",
      total: input.projectionIds.length,
      completed: 0,
      succeeded: 0,
      skipped: 0,
      failed: input.projectionIds.length,
      stillUnknown: 0,
      selectionSnapshotId: `snap-${input.requestId}`,
      items: input.projectionIds.map((id) => ({
        projectionId: id,
        salesOrderNo: id,
        deliveryId: "",
        outcome: "failed" as const,
        reason: `批量最多 ${BULK_SELECTION_LIMIT} 条，超出部分请分批`,
      })),
      startedAt: new Date().toISOString(),
      nextAction: `批量最多 ${BULK_SELECTION_LIMIT} 条，超出部分请分批`,
    }
  }

  const jobId = `bulk_${input.action}_${input.requestId}`
  const items: BulkItemOutcome[] = []
  let succeeded = 0
  let failed = 0
  let stillUnknown = 0

  for (const projectionId of input.projectionIds) {
    try {
      const detail = await fetchExecutionProjectionDetail({ projectionId })
      const result = await submitProjectionDeliveryCommand({
        action: input.action === "BULK_QUERY" ? "QUERY_RESULT" : "RETRY",
        projectionId,
        projectionRevisionId: detail?.selectedRevision.projectionRevisionId ?? "",
        deliveryId:
          detail?.deliveries[0]?.deliveryId ?? `dlv_${projectionId}`,
        expectedObjectVersion: detail?.objectVersion ?? "1",
        requestId: `${input.requestId}:${projectionId}`,
      })
      if (result.stillUnknown) {
        stillUnknown += 1
        items.push({
          projectionId,
          salesOrderNo: result.salesOrderNo,
          deliveryId: result.deliveryId,
          outcome: "still_unknown",
          reason: result.resultLabel,
        })
      } else if (result.result === "FAILED") {
        failed += 1
        items.push({
          projectionId,
          salesOrderNo: result.salesOrderNo,
          deliveryId: result.deliveryId,
          outcome: "failed",
          reason: result.resultLabel,
        })
      } else {
        succeeded += 1
        items.push({
          projectionId,
          salesOrderNo: result.salesOrderNo,
          deliveryId: result.deliveryId,
          outcome: "succeeded",
          reason: result.resultLabel,
        })
      }
    } catch {
      failed += 1
      items.push({
        projectionId,
        salesOrderNo: projectionId,
        deliveryId: "",
        outcome: "failed",
        reason: "请求失败",
      })
    }
  }

  const job: BulkProjectionJob = {
    jobId,
    action: input.action,
    status:
      failed === 0 && stillUnknown === 0
        ? "succeeded"
        : succeeded > 0
          ? "partial"
          : "failed",
    total: input.projectionIds.length,
    completed: items.length,
    succeeded,
    skipped: 0,
    failed,
    stillUnknown,
    selectionSnapshotId: `snap-${input.requestId}`,
    items,
    startedAt: new Date().toISOString(),
    nextAction:
      stillUnknown > 0
        ? "存在结果未知项：不得标成功，请逐项查询"
        : failed > 0
          ? "部分失败，请查看明细"
          : "批量完成",
  }
  bulkJobs.set(jobId, job)
  return job
}

export async function fetchBulkJob(
  jobId: string
): Promise<BulkProjectionJob | null> {
  return bulkJobs.get(jobId) ?? null
}
