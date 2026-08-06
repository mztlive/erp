/**
 * W27 API 供应商结算 · 真实 HTTP API
 * 路径：/admin/supplier-settlement-statements、items、differences
 */

import { apiGet, apiPost, type Page } from "@/lib/api"
import type {
  AppendEvidenceInput,
  CreateDraftInput,
  DifferenceType,
  FormalOutcome,
  RefreshDraftInput,
  ResolveDifferenceInput,
  ReviewDecisionInput,
  SettlementDetailView,
  SettlementListRow,
  SettlementListView,
  SettlementStatus,
  SettlementView,
  SubmitReviewInput,
} from "@/features/supplier-settlements/types"
import {
  DIFF_STATUS_LABEL,
  DIFF_TYPE_LABEL,
  RESOLUTION_TO_STATUS,
  STATUS_LABEL,
  STATUS_TONE,
  VIEW_LABEL,
} from "@/features/supplier-settlements/types"

// ---------------------------------------------------------------------------
// Backend wire types
// ---------------------------------------------------------------------------

type BackendStatement = {
  id: string
  statement_no: string
  supplier_id: string
  period_start: string
  period_end: string
  external_bill_no?: string | null
  external_bill_version?: string | null
  erp_amount: string
  supplier_amount: string
  difference_amount: string
  status: string
  prepared_by: string
  reviewed_by?: string | null
  confirmed_at?: number | null
  payable_account_id?: string | null
  version: number
  created_at: number
}

type BackendItem = {
  id: string
  statement_id: string
  supplier_fulfillment_order_id: string
  supplier_fulfillment_item_id: string
  order_amount: string
  freight_amount: string
  service_fee_amount: string
  refund_amount: string
  erp_calculated_amount: string
  supplier_billed_amount: string
  created_at: number
}

type BackendDifference = {
  id: string
  statement_item_id: string
  difference_type: string
  difference_amount: string
  status: string
  resolution?: string | null
  resolved_by?: string | null
  resolved_at?: number | null
  version: number
  created_at: number
}

type BackendDetail = {
  statement: BackendStatement
  items: BackendItem[]
  differences: BackendDifference[]
}

export type ListQueryInput = {
  view: SettlementView
  supplierId?: string
  periodFrom?: string
  periodTo?: string
  status?: string
  differenceType?: DifferenceType
  q?: string
  page: number
  pageSize?: number
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function tsToIso(secs: number | null | undefined): string {
  if (secs == null || !Number.isFinite(Number(secs)) || Number(secs) <= 0)
    return ""
  return new Date(Number(secs) * 1000).toISOString()
}

function asStatus(raw: string): SettlementStatus {
  const u = raw.toUpperCase() as SettlementStatus
  const allowed: SettlementStatus[] = [
    "DRAFT",
    "PENDING_RECONCILE",
    "HAS_DIFFERENCE",
    "PENDING_REVIEW",
    "CONFIRMED",
    "VOIDED",
  ]
  return allowed.includes(u) ? u : "DRAFT"
}

function directionLabel(diff?: string): string | undefined {
  if (diff == null) return undefined
  const n = Number(diff)
  if (!Number.isFinite(n) || n === 0) return "无差异"
  if (n > 0) return "供应商账单高于 ERP"
  return "ERP 高于供应商账单"
}

function toListRow(s: BackendStatement): SettlementListRow {
  const status = asStatus(s.status)
  const allowed = ["OPEN_CENTER", "VIEW", "OPEN_PREVIEW"]
  if (status !== "CONFIRMED" && status !== "VOIDED") {
    allowed.push("RESOLVE_DIFFERENCE", "SUBMIT_REVIEW")
  }
  if (status === "PENDING_REVIEW") {
    allowed.push("CONFIRM", "REJECT")
  }
  return {
    statementId: s.id,
    statementNo: s.statement_no,
    supplierId: s.supplier_id,
    supplierName: s.supplier_id,
    periodStart: s.period_start,
    periodEnd: s.period_end,
    periodLabel: s.period_start.slice(0, 7),
    status,
    statusLabel: STATUS_LABEL[status],
    statusTone: STATUS_TONE[status],
    erpAmountGross: String(s.erp_amount),
    supplierAmountGross: String(s.supplier_amount),
    differenceAmountGross: String(s.difference_amount),
    differenceDirectionLabel: directionLabel(String(s.difference_amount)),
    unresolvedDifferenceCount: 0,
    preparedBy: s.prepared_by
      ? { userId: s.prepared_by, displayName: s.prepared_by }
      : undefined,
    reviewedBy: s.reviewed_by
      ? { userId: s.reviewed_by, displayName: s.reviewed_by }
      : undefined,
    preparedByLabel: s.prepared_by || "—",
    reviewedByLabel: s.reviewed_by || "待复核人",
    updatedAt: tsToIso(s.created_at),
    allowedActions: allowed,
    actionBlockers: [],
  }
}

function toDetail(d: BackendDetail): SettlementDetailView {
  const s = d.statement
  const status = asStatus(s.status)
  const diffs = (d.differences ?? []).map((diff) => {
    const diffStatus = (diff.status?.toUpperCase() ||
      "PENDING") as SettlementDetailView["differences"][number]["status"]
    const type = (diff.difference_type?.toUpperCase() ||
      "AMOUNT") as DifferenceType
    return {
      differenceId: diff.id,
      type: DIFF_TYPE_LABEL[type] ? type : ("AMOUNT" as DifferenceType),
      typeLabel: DIFF_TYPE_LABEL[type] ?? diff.difference_type,
      status: DIFF_STATUS_LABEL[diffStatus] ? diffStatus : "PENDING",
      statusLabel: DIFF_STATUS_LABEL[diffStatus] ?? diff.status,
      statusTone:
        diffStatus === "PENDING"
          ? ("warning" as const)
          : diffStatus === "CLOSED"
            ? ("success" as const)
            : ("info" as const),
      blocking: diffStatus === "PENDING",
      erpSideLabel: "ERP 试算",
      supplierSideLabel: "供应商账单",
      amountDirectionLabel: directionLabel(String(diff.difference_amount)) ?? "—",
      amountGross: String(diff.difference_amount),
      version: diff.version,
      evidence: [],
      requiresProcurementEvidence: false,
      leftFields: [],
    }
  })

  const open = diffs.filter((x) => x.status === "PENDING").length
  const blocking = diffs.filter((x) => x.blocking).length
  const resolved = diffs.length - open
  const now = new Date().toISOString()
  const allowed = ["OPEN_CENTER", "VIEW"]
  if (status === "DRAFT" || status === "PENDING_RECONCILE" || status === "HAS_DIFFERENCE") {
    allowed.push("RESOLVE_DIFFERENCE", "SUBMIT_REVIEW")
  }
  if (status === "PENDING_REVIEW") {
    allowed.push("CONFIRM", "REJECT")
  }

  return {
    statement: {
      id: s.id,
      statementNo: s.statement_no,
      supplierId: s.supplier_id,
      supplierName: s.supplier_id,
      periodStart: s.period_start,
      periodEnd: s.period_end,
      periodLabel: s.period_start.slice(0, 7),
      externalBillNo: s.external_bill_no ?? undefined,
      externalBillVersion: s.external_bill_version ?? undefined,
      erpAmountGross: String(s.erp_amount),
      supplierAmountGross: String(s.supplier_amount),
      differenceAmountGross: String(s.difference_amount),
      differenceDirectionLabel: directionLabel(String(s.difference_amount)),
      status,
      statusLabel: STATUS_LABEL[status],
      statusTone: STATUS_TONE[status],
      preparedBy: s.prepared_by
        ? { userId: s.prepared_by, displayName: s.prepared_by }
        : undefined,
      reviewedBy: s.reviewed_by
        ? { userId: s.reviewed_by, displayName: s.reviewed_by }
        : undefined,
      lockVersion: s.version,
      sourceAsOf: tsToIso(s.created_at),
      sourceSnapshotAt: tsToIso(s.created_at),
      sourceSnapshotHash: `v${s.version}`,
    },
    totals: {
      // 金额一律来自结算单头；不在前端汇总明细
      orderAmountGross: String(s.erp_amount),
      freightGross: "0.00",
      serviceFeeGross: "0.00",
      refundGross: "0.00",
      erpAmountGross: String(s.erp_amount),
      supplierAmountGross: String(s.supplier_amount),
      differenceAmountGross: String(s.difference_amount),
      differenceDirectionLabel: directionLabel(String(s.difference_amount)),
      taxBasisLabel: "含税",
    },
    items: (d.items ?? []).map((it) => ({
      itemId: it.id,
      supplierOrderNo: it.supplier_fulfillment_order_id,
      externalOrderNo: it.supplier_fulfillment_order_id,
      productName: it.supplier_fulfillment_item_id,
      quantity: "—",
      factLabel: "履约结算",
      orderAmountGross: String(it.order_amount),
      freightGross: String(it.freight_amount),
      serviceFeeGross: String(it.service_fee_amount),
      refundGross: String(it.refund_amount),
      erpAmountGross: String(it.erp_calculated_amount),
      supplierBillLineGross: String(it.supplier_billed_amount),
      readOnly: true as const,
    })),
    differences: diffs,
    differenceSummary: {
      total: diffs.length,
      open,
      blocking,
      resolved,
    },
    reviewRecords: [],
    payable: s.payable_account_id
      ? {
          payableAccountId: s.payable_account_id,
          payableNo: s.payable_account_id,
          grossAmount: String(s.erp_amount),
          dueDate: "",
          statusLabel: "已生成",
          w12Href: `/finance/supplier-accounts?view=payable&q=${encodeURIComponent(s.payable_account_id)}`,
        }
      : undefined,
    auditEvents: [],
    allowedActions: allowed,
    actionBlockers: [],
    freshness: {
      immutableFactsAsOf: tsToIso(s.created_at),
      queriedAt: now,
    },
    canEditBillOrOrder: false,
  }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export async function fetchSettlementList(
  input: ListQueryInput
): Promise<SettlementListView> {
  const queriedAt = new Date().toISOString()
  const pageSize = input.pageSize ?? 50
  const emptyBase: SettlementListView = {
    view: input.view,
    rows: [],
    page: 1,
    pageSize,
    total: 0,
    totals: {
      pendingReconcile: 0,
      hasDifference: 0,
      pendingReview: 0,
      confirmedAmountThisPeriod: "0.00",
    },
    metrics: {
      pending: 0,
      hasDifference: 0,
      pendingReview: 0,
      confirmedAmount: "0.00",
    },
    suppliers: [],
    permissionVersion: "server",
    sourceAsOf: queriedAt,
    queriedAt,
    filterSummary: "",
    hasModulePermission: true,
    hasDataScope: true,
  }

  // Map view → status filter when possible
  let statusFilter = input.status
  if (!statusFilter) {
    if (input.view === "confirmed") statusFilter = "CONFIRMED"
    else if (input.view === "pending") statusFilter = undefined
  }

  const pageRes = await apiGet<Page<BackendStatement>>(
    "/admin/supplier-settlement-statements",
    {
      page: input.page,
      page_size: pageSize,
      supplier_id: input.supplierId,
      status: statusFilter?.split(",")[0]?.trim() || undefined,
      statement_no: input.q?.trim() || undefined,
      sort_by: "period_end",
      sort_dir: "asc",
    }
  )

  let statements = pageRes.items ?? []

  // Client-side view filters not supported by backend
  if (input.view === "pending") {
    statements = statements.filter((s) => {
      const st = asStatus(s.status)
      return (
        st === "DRAFT" ||
        st === "PENDING_RECONCILE" ||
        st === "HAS_DIFFERENCE" ||
        st === "PENDING_REVIEW"
      )
    })
  }

  if (input.periodFrom) {
    statements = statements.filter((s) => s.period_start >= input.periodFrom!)
  }
  if (input.periodTo) {
    statements = statements.filter((s) => s.period_end <= input.periodTo!)
  }

  const rows = statements.map(toListRow)
  const total = pageRes.total ?? rows.length
  const suppliersMap = new Map<string, string>()
  for (const s of statements) suppliersMap.set(s.supplier_id, s.supplier_id)

  const filterParts = [
    input.view !== "pending" ? `视图=${VIEW_LABEL[input.view]}` : null,
    input.supplierId ? `供应商=${input.supplierId}` : null,
    input.periodFrom || input.periodTo
      ? `期间=${input.periodFrom ?? "…"} ~ ${input.periodTo ?? "…"}`
      : null,
    input.q ? `搜索=${input.q}` : null,
  ].filter(Boolean)

  return {
    view: input.view,
    rows,
    page: pageRes.page ?? input.page,
    pageSize: pageRes.page_size ?? pageSize,
    total,
    totals: emptyBase.totals,
    metrics: emptyBase.metrics,
    suppliers: Array.from(suppliersMap.entries()).map(
      ([supplierId, supplierName]) => ({ supplierId, supplierName })
    ),
    emptyReason: total === 0 ? "NO_STATEMENTS" : undefined,
    hasModulePermission: true,
    hasDataScope: true,
    permissionVersion: "server",
    sourceAsOf: queriedAt,
    queriedAt,
    filterSummary: filterParts.length
      ? filterParts.join(" · ")
      : "默认待处理视图",
  }
}

export async function fetchSettlementDetail(input: {
  statementId: string
}): Promise<SettlementDetailView | null> {
  try {
    const detail = await apiGet<BackendDetail>(
      `/admin/supplier-settlement-statements/${encodeURIComponent(input.statementId)}`
    )
    return toDetail(detail)
  } catch (err) {
    const status =
      err && typeof err === "object" && "status" in err
        ? (err as { status?: number }).status
        : undefined
    if (status === 404) return null
    throw err
  }
}

export async function createSettlementDraft(
  input: CreateDraftInput
): Promise<FormalOutcome> {
  // Backend requires statement_no + items; frontend CreateDraftInput lacks items
  // → create with empty-items will 422. Document gap and call with minimal shape.
  try {
    const created = await apiPost<BackendStatement>(
      "/admin/supplier-settlement-statements",
      {
        statement_no: `ST-${input.periodStart.replace(/-/g, "").slice(0, 6)}-${input.requestId.slice(-6)}`,
        supplier_id: input.supplierId,
        period_start: input.periodStart,
        period_end: input.periodEnd,
        items: [],
      }
    )
    return {
      status: "succeeded",
      title: "结算草稿已创建",
      message: "已创建结算草稿。",
      reference: created.statement_no,
      statementId: created.id,
      lockVersion: created.version,
      facts: [
        { label: "结算单号", value: created.statement_no },
        { label: "供应商", value: created.supplier_id },
        {
          label: "期间",
          value: `${input.periodStart} ~ ${input.periodEnd}`,
        },
      ],
    }
  } catch (err) {
    const message =
      err && typeof err === "object" && "message" in err
        ? String((err as { message: string }).message)
        : "创建草稿失败"
    // Backend requires ≥1 item — surface as blocked with gap
    if (message.includes("至少一行") || message.includes("明细")) {
      return {
        status: "blocked",
        code: "ITEMS_REQUIRED",
        title: "无法创建空草稿",
        message:
          "后端要求结算明细至少一行；前端创建草稿未携带明细行（backend_gap：需试算生成明细接口）。",
      }
    }
    throw err
  }
}

/**
 * 刷新试算：后端无 refresh 端点。
 */
export async function refreshSettlementTrial(
  input: RefreshDraftInput
): Promise<FormalOutcome> {
  void input
  return {
    status: "blocked",
    code: "NOT_IMPLEMENTED",
    title: "刷新试算未交付",
    message: "后端尚未提供结算试算刷新接口。",
  }
}

/**
 * 追加采购证据：后端无 evidence 端点。
 */
export async function appendDifferenceEvidence(
  input: AppendEvidenceInput
): Promise<FormalOutcome> {
  void input
  return {
    status: "blocked",
    code: "NOT_IMPLEMENTED",
    title: "追加证据未交付",
    message: "后端尚未提供差异证据追加接口。",
  }
}

export async function resolveDifference(
  input: ResolveDifferenceInput
): Promise<FormalOutcome> {
  const status = RESOLUTION_TO_STATUS[input.resolution]
  await apiPost(
    `/admin/supplier-settlement-differences/${encodeURIComponent(input.differenceId)}/resolve`,
    {
      version: input.expectedDifferenceVersion,
      status,
      resolution: input.reasonCode,
      resolved_at: Math.floor(Date.now() / 1000),
    }
  )

  return {
    status: "succeeded",
    title: "差异结论已登记",
    message: "差异结论已写入，未改写订单原值。",
    operationId: input.operationId,
    statementId: input.statementId,
    facts: [
      { label: "结论", value: status },
      { label: "原因", value: input.reasonCode },
    ],
  }
}

export async function submitSettlementReview(
  input: SubmitReviewInput
): Promise<FormalOutcome> {
  await apiPost(
    `/admin/supplier-settlement-statements/${encodeURIComponent(input.statementId)}/submit-review`,
    {
      version: input.expectedLockVersion,
      comment: input.comment,
    }
  )
  return {
    status: "succeeded",
    title: "已提交复核",
    message: "结算单已进入待复核。",
    operationId: input.operationId,
    statementId: input.statementId,
  }
}

/**
 * 领取复核：后端无独立 claim；由 confirm 路径覆盖。
 */
export async function claimSettlementReview(input: {
  statementId: string
  workItemId: string
  expectedSubjectVersion?: string
  idempotencyKey?: string
}): Promise<FormalOutcome> {
  void input
  return {
    status: "succeeded",
    title: "已进入复核",
    message: "可继续确认或驳回。",
    statementId: input.statementId,
    idempotencyKey: input.idempotencyKey,
  }
}

export async function decideSettlementReview(
  input: ReviewDecisionInput
): Promise<FormalOutcome> {
  if (input.action === "CONFIRM") {
    await apiPost(
      `/admin/supplier-settlement-statements/${encodeURIComponent(input.statementId)}/confirm`,
      {
        version: input.expectedLockVersion,
      }
    )
    return {
      status: "succeeded",
      title: "结算已确认",
      message: "确认后形成应付，结算单永久只读。",
      operationId: input.operationId,
      statementId: input.statementId,
    }
  }

  // REJECT: backend has void, not reject-to-prep. Use void only if intentional —
  // reject is a backend_gap. Return blocked.
  return {
    status: "blocked",
    code: "REJECT_NOT_IMPLEMENTED",
    title: "驳回未交付",
    message:
      "后端提供作废接口，未提供「驳回至经办」接口。请勿将驳回映射为作废。",
    operationId: input.operationId,
    statementId: input.statementId,
  }
}

export type { SettlementStatus }
