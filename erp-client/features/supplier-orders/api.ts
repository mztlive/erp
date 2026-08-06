/**
 * W26 供应商订单 · 真实 HTTP API
 * 路径：/admin/supplier-fulfillment-orders、/admin/work-items、/admin/background-jobs
 * 后端视图较前端 mock 精简：缺失字段以安全默认值适配并登记 backend_gap。
 */

import { apiGet, apiPost, type Page } from "@/lib/api"
import type {
  AfterSalesActionInput,
  AfterSalesActionResult,
  CancelStatus,
  DeferTaskInput,
  DeferTaskResult,
  DemoRole,
  ExportCommand,
  ExportJobResult,
  FormalActionResponse,
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
} from "@/features/supplier-orders/types"
import {
  CANCEL_STATUS_LABEL,
  CANCEL_STATUS_TONE,
  COST_MASK,
  FULFILLMENT_STATUS_LABEL,
  FULFILLMENT_STATUS_TONE,
  REFUND_STATUS_LABEL,
  REFUND_STATUS_TONE,
} from "@/features/supplier-orders/types"

const PAYMENT_OCCURRED_NOTICE =
  "商城支付已发生。供应商履约结果独立记录，不得用取消/退款折入履约主状态。"
const PERMISSION_VERSION = "server"

// ---------------------------------------------------------------------------
// Backend wire types
// ---------------------------------------------------------------------------

type BackendOrder = {
  id: string
  fulfillment_order_no: string
  mall_order_id: string
  supplier_id: string
  connection_id: string
  split_no: number
  fulfillment_status: string
  cancel_status: string
  refund_status: string
  external_order_no?: string | null
  submitted_at?: number | null
  accepted_at?: number | null
  completed_at?: number | null
  version: number
  created_at: number
}

type BackendItem = {
  id: string
  supplier_fulfillment_order_id: string
  mall_order_item_id: string
  supplier_offering_revision_id: string
  supplier_catalog_sku_id: string
  quantity: string
  unit_cost_snapshot_gross: string
  cost_snapshot_total_gross: string
  input_tax_rate: string
}

type BackendStatusHistory = {
  id: string
  previous_status: string
  new_status: string
  supplier_status_version: string
  occurred_at: number
  received_at: number
  external_event_id: string
  source_type: string
  created_at: number
}

type BackendAction = {
  id: string
  supplier_fulfillment_order_id: string
  action_type: string
  after_sales_request_id?: string | null
  status: string
  external_request_id?: string | null
  request_summary?: string | null
  response_summary?: string | null
  attempt_count: number
  created_at: number
}

type BackendDetail = {
  order: BackendOrder
  items: BackendItem[]
  status_history: BackendStatusHistory[]
  actions: BackendAction[]
  refund_facts: Array<{
    id: string
    supplier_fulfillment_order_id: string
    external_refund_no: string
    refund_amount: string
    refunded_at: number
  }>
}

type BackendSubmitResult = {
  action: BackendAction
  lines: unknown[]
  order: BackendOrder
}

type BackendBackgroundJob = {
  id: string
  job_no: string
  status: string
  result_expires_at?: number | null
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function tsToIso(secs: number | null | undefined): string {
  if (secs == null || !Number.isFinite(Number(secs)) || Number(secs) <= 0)
    return ""
  return new Date(Number(secs) * 1000).toISOString()
}

function asFulfillment(raw: string): SupplierFulfillmentStatus {
  const u = raw.toUpperCase() as SupplierFulfillmentStatus
  const allowed: SupplierFulfillmentStatus[] = [
    "RECEIVED",
    "SUBMITTING",
    "ACCEPTED",
    "REJECTED",
    "RESULT_UNKNOWN",
    "FULFILLING",
    "SHIPPED",
    "COMPLETED",
    "EXCEPTION",
  ]
  return allowed.includes(u) ? u : "RECEIVED"
}

function asCancel(raw: string): CancelStatus {
  const u = raw.toUpperCase() as CancelStatus
  const allowed: CancelStatus[] = [
    "NONE",
    "CANCEL_PENDING",
    "CANCELED",
    "FAILED",
    "MANUAL",
  ]
  return allowed.includes(u) ? u : "NONE"
}

function asRefund(raw: string): RefundStatus {
  const u = raw.toUpperCase() as RefundStatus
  const allowed: RefundStatus[] = [
    "NONE",
    "REFUND_PENDING",
    "PARTIAL",
    "REFUNDED",
    "REFUND_FAILED",
    "MANUAL",
  ]
  return allowed.includes(u) ? u : "NONE"
}

function priorityOf(status: SupplierFulfillmentStatus): number {
  switch (status) {
    case "RESULT_UNKNOWN":
      return 100
    case "EXCEPTION":
    case "REJECTED":
      return 90
    case "SUBMITTING":
    case "RECEIVED":
      return 70
    default:
      return 10
  }
}

function mapListRow(o: BackendOrder, role: DemoRole): SupplierOrderListRow {
  const fulfillment = asFulfillment(o.fulfillment_status)
  const cancel = asCancel(o.cancel_status)
  const refund = asRefund(o.refund_status)
  const lastBusinessAt =
    tsToIso(o.completed_at) ||
    tsToIso(o.accepted_at) ||
    tsToIso(o.submitted_at) ||
    tsToIso(o.created_at)

  return {
    orderId: o.id,
    orderNo: o.fulfillment_order_no,
    mallOrderId: o.mall_order_id,
    mallOrderNo: o.mall_order_id,
    supplierId: o.supplier_id,
    supplierName: o.supplier_id,
    externalOrderNo: o.external_order_no ?? undefined,
    fulfillmentStatus: fulfillment,
    fulfillmentLabel: FULFILLMENT_STATUS_LABEL[fulfillment],
    fulfillmentTone: FULFILLMENT_STATUS_TONE[fulfillment],
    cancelStatus: cancel,
    cancelLabel: CANCEL_STATUS_LABEL[cancel],
    cancelTone: CANCEL_STATUS_TONE[cancel],
    refundStatus: refund,
    refundLabel: REFUND_STATUS_LABEL[refund],
    refundTone: REFUND_STATUS_TONE[refund],
    paidAt: tsToIso(o.created_at),
    updatedAt: lastBusinessAt,
    lastBusinessAt,
    costGross: role === "cs" || role === "ops" ? null : undefined,
    itemCount: 0,
    allowedActions: ["OPEN_CENTER", "NOTE"],
    actionBlockers: [],
    priority: priorityOf(fulfillment),
  }
}

function emptyMetrics(): SupplierOrderMetric[] {
  return [
    {
      key: "pending_submit",
      label: "待提交",
      value: 0,
      fulfillmentStatuses: ["RECEIVED", "SUBMITTING"],
    },
    {
      key: "result_unknown",
      label: "结果未知",
      value: 0,
      fulfillmentStatus: "RESULT_UNKNOWN",
    },
    {
      key: "exception",
      label: "履约异常",
      value: 0,
      fulfillmentStatuses: ["EXCEPTION", "REJECTED"],
    },
    {
      key: "aftersale",
      label: "售后待处理",
      value: 0,
      aftersalePending: true,
    },
    { key: "all", label: "全部订单", value: 0, view: "all" },
  ]
}

function filterSummary(query: SupplierOrderListQuery, total: number): string {
  const parts: string[] = []
  if (query.view === "actionable") parts.push("可操作")
  else if (query.view === "recent_completed") parts.push("最近完成")
  else parts.push("全部")
  if (query.q?.trim()) parts.push(`搜索「${query.q.trim()}」`)
  if (query.supplierId) parts.push(query.supplierId)
  if (query.fulfillmentStatuses?.length) {
    parts.push(
      query.fulfillmentStatuses
        .map((s) => FULFILLMENT_STATUS_LABEL[s])
        .join("/")
    )
  }
  parts.push(`${total} 条`)
  return parts.join(" · ")
}

function mapDetail(
  d: BackendDetail,
  role: DemoRole
): SupplierOrderDetailView {
  const o = d.order
  const fulfillment = asFulfillment(o.fulfillment_status)
  const cancel = asCancel(o.cancel_status)
  const refund = asRefund(o.refund_status)
  const costVisible =
    role === "procurement" || role === "finance" || role === "admin"
  const placeAction =
    d.actions?.find((a) => a.action_type === "PLACE") ?? d.actions?.[0]

  return {
    order: {
      id: o.id,
      orderNo: o.fulfillment_order_no,
      mallOrderId: o.mall_order_id,
      mallOrderNo: o.mall_order_id,
      paidAt: tsToIso(o.created_at),
      paymentFactKey: "",
      fulfillmentChain: "ERP_AUTOMATED",
      supplierId: o.supplier_id,
      supplierName: o.supplier_id,
      connectionCode: o.connection_id,
      connectionEnvironment: "production",
      supplyVersion: "",
      publicationVersion: "",
      externalOrderNo: o.external_order_no ?? undefined,
      fulfillmentStatus: fulfillment,
      fulfillmentLabel: FULFILLMENT_STATUS_LABEL[fulfillment],
      fulfillmentTone: FULFILLMENT_STATUS_TONE[fulfillment],
      cancelStatus: cancel,
      cancelLabel: CANCEL_STATUS_LABEL[cancel],
      cancelTone: CANCEL_STATUS_TONE[cancel],
      refundStatus: refund,
      refundLabel: REFUND_STATUS_LABEL[refund],
      refundTone: REFUND_STATUS_TONE[refund],
      lockVersion: o.version,
      paymentOccurredNotice: PAYMENT_OCCURRED_NOTICE,
    },
    items: (d.items ?? []).map((it) => ({
      itemId: it.id,
      mallLineId: it.mall_order_item_id,
      productName: it.supplier_catalog_sku_id,
      skuCode: it.supplier_catalog_sku_id,
      quantity: String(it.quantity),
      unit: "件",
      supplierProductId: it.supplier_catalog_sku_id,
      supplierProductName: it.supplier_catalog_sku_id,
      publicationVersion: "",
      supplyVersion: it.supplier_offering_revision_id,
      unitCostGross: costVisible
        ? String(it.unit_cost_snapshot_gross)
        : null,
      unitCostNet: null,
      inputTaxRate: costVisible ? String(it.input_tax_rate) : null,
      snapshotImmutable: true as const,
    })),
    logistics: {
      acceptedAt: tsToIso(o.accepted_at) || undefined,
      shippedAt: undefined,
      completedAt: tsToIso(o.completed_at) || undefined,
    },
    statusHistory: (d.status_history ?? []).map((h) => ({
      id: h.id,
      at: tsToIso(h.occurred_at),
      track: "fulfillment" as const,
      fromLabel:
        FULFILLMENT_STATUS_LABEL[asFulfillment(h.previous_status)] ??
        h.previous_status,
      toLabel:
        FULFILLMENT_STATUS_LABEL[asFulfillment(h.new_status)] ?? h.new_status,
      source: h.source_type,
    })),
    afterSales: [],
    costs: {
      costMasked: !costVisible,
      cumulativeCostGross: costVisible
        ? String(
            d.items?.[0]?.cost_snapshot_total_gross ?? null
          )
        : null,
      cumulativeCostNet: null,
      costSource: "下单成本快照",
      costVariance: null,
    },
    actions: (d.actions ?? []).map((a) => ({
      actionId: a.id,
      actionType: (a.action_type as SupplierOrderDetailView["actions"][number]["actionType"]) || "PLACE",
      actionLabel: a.action_type,
      at: tsToIso(a.created_at),
      actor: "系统",
      outcomeLabel: a.status,
      outcomeTone: "neutral" as const,
      idempotencyKeyTail: a.external_request_id
        ? `…${a.external_request_id.slice(-6)}`
        : "—",
      attemptCount: a.attempt_count,
      techSummary:
        role === "admin"
          ? [a.request_summary, a.response_summary].filter(Boolean).join(" · ") ||
            undefined
          : undefined,
      operationId: a.id,
    })),
    address: {
      masked: "—",
      phoneMasked: "—",
      recipientMasked: "—",
      canReveal: role === "procurement" || role === "cs" || role === "admin",
    },
    placeActionId: placeAction?.id ?? "",
    allowedActions: ["OPEN_CENTER", "NOTE"],
    actionBlockers: [],
    freshness: {
      updatedAt: tsToIso(o.created_at),
      state: "fresh",
    },
    role,
  }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export async function fetchSupplierOrders(
  query: SupplierOrderListQuery
): Promise<SupplierOrderListResult> {
  const now = new Date().toISOString()
  const pageRes = await apiGet<Page<BackendOrder>>(
    "/admin/supplier-fulfillment-orders",
    {
      page: query.page,
      page_size: query.pageSize,
      supplier_id: query.supplierId,
      fulfillment_status: query.fulfillmentStatuses?.[0],
      cancel_status: query.cancelStatuses?.[0],
      refund_status: query.refundStatuses?.[0],
      external_order_no: query.q?.trim() || undefined,
      sort_by:
        query.sortBy === "lastBusinessAt" ? "created_at" : "created_at",
      sort_dir: query.sortDir ?? "desc",
    }
  )

  let rows = (pageRes.items ?? []).map((o) => mapListRow(o, query.role))

  // 客户端视图投影（后端未提供 view/actionable 筛选）
  if (query.view === "actionable") {
    rows = rows.filter(
      (r) =>
        r.fulfillmentStatus === "RESULT_UNKNOWN" ||
        r.fulfillmentStatus === "EXCEPTION" ||
        r.fulfillmentStatus === "REJECTED" ||
        r.fulfillmentStatus === "SUBMITTING" ||
        r.fulfillmentStatus === "RECEIVED" ||
        r.cancelStatus === "FAILED" ||
        r.cancelStatus === "MANUAL" ||
        r.cancelStatus === "CANCEL_PENDING" ||
        r.refundStatus === "REFUND_FAILED" ||
        r.refundStatus === "MANUAL" ||
        r.refundStatus === "REFUND_PENDING"
    )
  } else if (query.view === "recent_completed") {
    rows = rows.filter((r) => r.fulfillmentStatus === "COMPLETED")
  }

  return {
    rows,
    pageInfo: {
      page: pageRes.page ?? query.page,
      pageSize: pageRes.page_size ?? query.pageSize,
      total: pageRes.total ?? rows.length,
    },
    metrics: emptyMetrics(),
    permissionVersion: PERMISSION_VERSION,
    sourceAsOf: now,
    queriedAt: now,
    filterSummary: filterSummary(query, pageRes.total ?? rows.length),
  }
}

export async function fetchSupplierOrderDetail(input: {
  orderId: string
  role?: DemoRole
}): Promise<SupplierOrderDetailView | null> {
  try {
    const detail = await apiGet<BackendDetail>(
      `/admin/supplier-fulfillment-orders/${encodeURIComponent(input.orderId)}`
    )
    return mapDetail(detail, input.role ?? "procurement")
  } catch (err) {
    const status =
      err && typeof err === "object" && "status" in err
        ? (err as { status?: number }).status
        : undefined
    if (status === 404) return null
    throw err
  }
}

/**
 * 查询原结果：后端集成在 integration_ops 错误任务上；
 * 履约订单域无独立 QUERY 端点 → 返回 blocked 并指向 W29。
 */
export async function querySupplierResult(
  input: QueryResultInput
): Promise<FormalActionResponse<QueryResultData>> {
  void input
  return {
    status: "blocked",
    message:
      "查询原结果请进入接口错误与对账中心（W29）。履约订单域未提供独立查询端点。",
    reference: "W29",
  }
}

/**
 * 安全重发：后端无独立 REPLAY 端点（在 integration_ops error-task replay）。
 */
export async function replaySupplierOrder(
  input: ReplayInput
): Promise<FormalActionResponse<ReplayResultData>> {
  void input
  return {
    status: "blocked",
    message:
      "安全重发请在接口错误中心按原任务号重放。履约订单域未提供独立重发端点。",
    reference: "W29",
  }
}

export async function deferSupplierOrderTask(
  input: DeferTaskInput
): Promise<FormalActionResponse<DeferTaskResult>> {
  if (!input.workItemId) {
    return { status: "failed", message: "无关联任务，无法跳过" }
  }
  // version: try parse from expectedSubjectHash trailing digits, fallback 1
  const versionMatch = input.expectedSubjectHash?.match(/(\d+)$/)
  const version = versionMatch ? Number(versionMatch[1]) || 1 : 1

  await apiPost(`/admin/work-items/${encodeURIComponent(input.workItemId)}/defer`, {
    version,
    comment: input.comment ?? input.reasonCode,
  })

  return {
    status: "succeeded",
    message: "已记录跳过原因。任务未完成、未转交，仍为待处理。",
    data: {
      reasonCode: input.reasonCode,
      queueContextId: input.queueContextId,
      leaseDisposition: "RELEASED",
      workItemStatus: "PENDING",
    },
  }
}

export async function submitAfterSalesAction(
  input: AfterSalesActionInput
): Promise<FormalActionResponse<AfterSalesActionResult>> {
  const path =
    input.action === "CANCEL"
      ? `/admin/supplier-fulfillment-orders/${encodeURIComponent(input.orderId)}/cancel`
      : `/admin/supplier-fulfillment-orders/${encodeURIComponent(input.orderId)}/refund`

  const result = await apiPost<BackendSubmitResult>(path, {
    after_sales_request_id: input.afterSalesRequestId,
    lines: [],
    reason_code: input.reasonCode,
    comment: input.comment,
  })

  const order = result.order
  return {
    status: "succeeded",
    message:
      input.action === "CANCEL"
        ? "取消动作已提交供应商"
        : "退款动作已提交供应商",
    reference: result.action?.id,
    operationId: input.operationId,
    data: {
      lockVersion: order.version,
      cancelStatus: asCancel(order.cancel_status),
      refundStatus: asRefund(order.refund_status),
      actionRecordId: result.action?.id ?? input.operationId,
      note: "动作已登记",
    },
  }
}

/**
 * 地址揭示：后端详情不返回明文地址（仅加密快照），无 reveal 端点。
 */
export async function revealSupplierOrderAddress(
  input: RevealAddressInput
): Promise<FormalActionResponse<RevealAddressResult>> {
  void input
  return {
    status: "blocked",
    message: "地址揭示端点尚未交付；详情仅提供脱敏摘要。",
  }
}

export async function clearAddressReveal(orderId: string): Promise<void> {
  void orderId
  // no server session to clear
}

/**
 * 协同说明：后端无 NOTE 端点 → blocked。
 */
export async function addCollaborationNote(
  input: NoteInput
): Promise<FormalActionResponse<{ lockVersion: number }>> {
  void input
  return {
    status: "blocked",
    message: "协同说明写入端点尚未交付。",
  }
}

export function listSupplierOptions(): { id: string; name: string }[] {
  // 供应商主数据由 W14/W20 提供；本域列表无独立 supplier 维表接口
  return []
}

export async function createSupplierOrderExportJob(
  command: ExportCommand
): Promise<ExportJobResult> {
  const job = await apiPost<BackendBackgroundJob>("/admin/background-jobs", {
    job_no: `EXP-W26-${command.requestId.slice(-12)}`,
    job_type: "export",
    domain_job_type: "supplier_fulfillment_order_export",
    selection_snapshot_id: command.selectionSnapshotId || null,
    request_id: command.requestId,
    total_count: Math.max(1, command.rowCount || 1),
    items: [
      {
        object_type: "supplier_fulfillment_order",
        object_id: command.selectionSnapshotId || command.requestId,
      },
    ],
  })

  return {
    jobId: job.id,
    requestId: command.requestId,
    rowCount: command.rowCount,
    permissionVersion: PERMISSION_VERSION,
    fieldSetId: command.fieldSetId,
    maskDisclaimer:
      "导出使用系统筛选快照与字段权限打码：收货地址、手机号不会以明文写入文件。",
    expiresAt: job.result_expires_at
      ? tsToIso(job.result_expires_at)
      : new Date(Date.now() + 7 * 24 * 60 * 60 * 1000).toISOString(),
    downloadLabel: `供应商订单_${job.job_no ?? job.id}.csv`,
    status: job.status === "completed" ? "succeeded" : "queued",
  }
}

export function formatCostDisplay(
  value: string | null | undefined,
  masked: boolean
): string {
  if (masked || value == null) return COST_MASK
  return value
}

/** test helper retained for type-compat; no session state */
export function __resetSupplierOrderSessions() {
  // no-op
}
