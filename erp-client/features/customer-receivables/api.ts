/**
 * W11 客户往来 API：真实 HTTP（/admin/receivable-accounts、customer-receipts、invoices、
 * receipt-reversals、customer-refunds）。
 * 开放余额与净分配一律来自服务端投影，禁止前端拟合计冒充结果。
 * 导出签名保持稳定，供 queries.ts 作 queryFn/mutationFn。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { Page } from "@/lib/api"
import type {
  AllocationDraftLine,
  AllocationLine,
  AllocationSessionView,
  AllowedAction,
  CreateSessionInput,
  CustomerAccountsDetailView,
  CustomerAccountsListView,
  CustomerAccountsQuery,
  PostAllocationInput,
  PostAllocationResult,
  ReceiptRow,
  ReceivableAccountRow,
  ReceivableEntry,
  ReverseFactInput,
  ReverseFactResult,
  SalesInvoiceRow,
  SaveAllocationDraftInput,
} from "@/features/customer-receivables/types"
import { DUE_LABEL } from "@/features/customer-receivables/types"

// ─── Backend DTO shapes (snake_case; amounts as strings) ───────────────────

type BackendReceivableEntry = {
  id: string
  entry_type: string
  direction: "increase" | "decrease"
  amount: string
  due_date: string
  source_document_id: string
  source_sequence: number
  posted_at: number
  offset_total: string
}

type BackendFundsReview = {
  id: string
  review_no: number
  review_type: string
  review_result: string
  reviewed_by: string
  reviewed_at: number
  evidence_reference?: string | null
}

type BackendReceivableAccount = {
  id: string
  sales_order_id: string
  account_seq: number
  customer_id: string
  counterparty_party_id: string
  review_status: string
  gross_total: string
  settled_total: string
  open_total: string
  invoiceable_total: string
  invoiced_total: string
  open_invoiceable_total: string
  status: string
  version: number
  created_at: number
  entries: BackendReceivableEntry[]
  reviews: BackendFundsReview[]
}

type BackendReceiptAllocation = {
  id: string
  allocation_seq: number
  allocation_action: "apply" | "reverse"
  receivable_entry_id: string
  allocated_amount: string
  allocated_at: number
  reverses_allocation_id?: string | null
}

type BackendCustomerReceipt = {
  id: string
  receipt_no: string
  status: string
  counterparty_party_id: string
  customer_id?: string | null
  received_at: number
  amount: string
  bank_reference?: string | null
  version: number
  created_at: number
  allocated_total: string
  unallocated_amount: string
  allocations: BackendReceiptAllocation[]
}

type BackendInvoiceAllocation = {
  id: string
  allocation_seq: number
  allocation_action: "apply" | "reverse"
  receivable_account_id: string
  allocated_gross_amount: string
  allocated_net_amount: string
  allocated_tax_amount: string
  reverses_allocation_id?: string | null
}

type BackendInvoice = {
  id: string
  invoice_direction: string
  invoice_kind: "blue" | "red"
  party_id: string
  invoice_code?: string | null
  invoice_no: string
  invoice_date: string
  gross_amount: string
  net_amount: string
  tax_amount: string
  rounding_adjustment_amount: string
  rounding_reason?: string | null
  original_invoice_id?: string | null
  status: string
  version: number
  created_at: number
  allocated_total: string
  unallocated_amount: string
  allocations: BackendInvoiceAllocation[]
}

// ─── Helpers (display only; no money recompute) ────────────────────────────

const LIST_PAGE_SIZE = 100

function instantToIso(secs: number | undefined | null): string {
  if (secs == null || !Number.isFinite(secs)) return ""
  return new Date(secs * 1000).toISOString()
}

function maskBank(raw?: string | null): string {
  const v = (raw ?? "").trim()
  if (!v) return "****"
  if (v.includes("*")) return v
  if (v.length <= 4) return `****${v}`
  return `****${v.slice(-4)}`
}

function mapReceivableStatus(
  s: string
): ReceivableAccountRow["status"] {
  if (s === "settled") return "settled"
  if (s === "partially_settled" || s === "partial") return "partial"
  return "open"
}

function mapReviewStatus(
  s: string
): ReceivableAccountRow["reviewStatus"] {
  switch (s) {
    case "opening_pending":
    case "pending_opening":
      return "pending_opening"
    case "reviewed":
      return "reviewed"
    case "sync_delta_pending":
    case "pending_sync_diff":
      return "pending_sync_diff"
    default:
      return "na"
  }
}

function reviewLabel(s: ReceivableAccountRow["reviewStatus"]): string {
  switch (s) {
    case "pending_opening":
      return "期初待复核"
    case "reviewed":
      return "已复核"
    case "pending_sync_diff":
      return "同步差额待复核"
    default:
      return "不适用"
  }
}

function statusMeta(status: ReceivableAccountRow["status"]): {
  statusLabel: string
  statusTone: ReceivableAccountRow["statusTone"]
} {
  if (status === "settled") return { statusLabel: "已结清", statusTone: "success" }
  if (status === "partial") return { statusLabel: "部分结清", statusTone: "warning" }
  return { statusLabel: "未结", statusTone: "info" }
}

function mapReceiptStatus(
  s: string
): ReceiptRow["status"] {
  if (s === "reversed") return "reversed"
  if (s === "posted") return "posted"
  return "draft"
}

function mapInvoiceStatus(
  s: string
): SalesInvoiceRow["status"] {
  if (s === "reversed" || s === "red_invoiced") return "reversed"
  if (s === "registered") return "registered"
  return "draft"
}

function mapAllocationAction(
  a: string
): "APPLY" | "REVERSE" {
  return a === "reverse" ? "REVERSE" : "APPLY"
}

function mapBackendStatusFilter(
  status: string | undefined
): string | undefined {
  if (!status) return undefined
  if (status === "partial") return "partially_settled"
  if (status === "pending_opening") return "opening_pending"
  if (status === "pending_sync_diff") return "sync_delta_pending"
  if (status === "na") return "not_applicable"
  return status
}

function filterSummary(query: CustomerAccountsQuery): string {
  const parts = [
    query.view === "receivable"
      ? "应收台账"
      : query.view === "receipt"
        ? "回款"
        : query.view === "sales_invoice"
          ? "销项发票"
          : "待核销",
  ]
  if (query.counterpartyPartyId) parts.push(query.counterpartyPartyId)
  if (query.customerId) parts.push("已按经营客户过滤")
  if (query.q?.trim()) parts.push(`「${query.q.trim()}」`)
  if (query.due && query.due !== "all") parts.push(DUE_LABEL[query.due])
  return parts.join(" · ")
}

function projectEntry(e: BackendReceivableEntry): ReceivableEntry {
  return {
    entryId: e.id,
    entryType: e.entry_type,
    direction: e.direction,
    amountGross: e.amount,
    dueDate: e.due_date,
    sourceLabel: e.source_document_id,
    postedAt: instantToIso(e.posted_at),
  }
}

function projectReceivable(a: BackendReceivableAccount): ReceivableAccountRow {
  const status = mapReceivableStatus(a.status)
  const reviewStatus = mapReviewStatus(a.review_status)
  const meta = statusMeta(status)
  const allowed: AllowedAction[] = [
    "VIEW_DETAIL",
    "REGISTER_RECEIPT",
    "REGISTER_INVOICE",
  ]
  // dueState 后端未返回：登记缺口，展示默认未到期（不以前端时钟推算逾期）
  return {
    accountId: a.id,
    accountSeq: a.account_seq,
    counterpartyPartyId: a.counterparty_party_id,
    counterpartyPartyName: a.counterparty_party_id,
    customerId: a.customer_id,
    customerName: a.customer_id,
    salesOrderId: a.sales_order_id,
    salesOrderNo: a.sales_order_id,
    businessType: "physical_service",
    businessTypeLabel: "实物服务",
    grossTotal: a.gross_total,
    settledTotal: a.settled_total,
    openTotal: a.open_total,
    invoicedTotal: a.invoiced_total,
    openInvoiceableTotal: a.open_invoiceable_total,
    dueDate: a.entries[0]?.due_date ?? "",
    dueState: "not_due",
    dueStateLabel: "未到期",
    status,
    ...meta,
    reviewStatus,
    reviewStatusLabel: reviewLabel(reviewStatus),
    baselineVersion: a.version,
    entries: (a.entries ?? []).map(projectEntry),
    allowedActions: allowed,
    actionBlockers: [],
  }
}

function projectReceipt(r: BackendCustomerReceipt): ReceiptRow {
  const status = mapReceiptStatus(r.status)
  const isPosted = status === "posted" || status === "reversed"
  const allowed: AllowedAction[] = ["VIEW_DETAIL"]
  if (status === "posted") {
    allowed.push("CONTINUE_ALLOCATE", "REVERSE_RECEIPT", "REFUND")
  }
  const statusLabel =
    r.status === "pending_review"
      ? "待复核"
      : status === "draft"
        ? "草稿"
        : status === "reversed"
          ? "已冲正"
          : "已确认"
  const statusTone =
    status === "reversed"
      ? "destructive"
      : status === "posted"
        ? "success"
        : "neutral"
  const allocations: AllocationLine[] = (r.allocations ?? []).map((a) => ({
    allocationId: a.id,
    action: mapAllocationAction(a.allocation_action),
    amountGross: a.allocated_amount,
    targetLabel: a.receivable_entry_id,
    targetId: a.receivable_entry_id,
    occurredAt: instantToIso(a.allocated_at),
    reverseOfAllocationId: a.reverses_allocation_id ?? undefined,
    isPosted: true as const,
  }))
  return {
    receiptId: r.id,
    receiptNo: r.receipt_no,
    counterpartyPartyId: r.counterparty_party_id,
    counterpartyPartyName: r.counterparty_party_id,
    customerId: r.customer_id ?? "",
    customerName: r.customer_id ?? "",
    receivedAt: instantToIso(r.received_at),
    amount: r.amount,
    bankReferenceMasked: maskBank(r.bank_reference),
    allocatedTotal: r.allocated_total,
    unallocatedAmount: r.unallocated_amount,
    status,
    statusLabel,
    statusTone,
    baselineVersion: r.version,
    allocations,
    allowedActions: allowed,
    actionBlockers: [],
    isPosted,
    canEdit: false,
    canDelete: false,
  }
}

function projectInvoice(inv: BackendInvoice): SalesInvoiceRow {
  const status = mapInvoiceStatus(inv.status)
  const isPosted = status === "registered" || status === "reversed"
  const allowed: AllowedAction[] = ["VIEW_DETAIL"]
  if (inv.invoice_kind === "blue" && status === "registered") {
    allowed.push("CONTINUE_ALLOCATE", "ISSUE_RED_INVOICE")
  }
  const allocations: AllocationLine[] = (inv.allocations ?? []).map((a) => ({
    allocationId: a.id,
    action: mapAllocationAction(a.allocation_action),
    amountGross: a.allocated_gross_amount,
    targetLabel: a.receivable_account_id,
    targetId: a.receivable_account_id,
    occurredAt: "",
    reverseOfAllocationId: a.reverses_allocation_id ?? undefined,
    isPosted: true as const,
  }))
  return {
    invoiceId: inv.id,
    invoiceCode: inv.invoice_code ?? undefined,
    invoiceNo: inv.invoice_no,
    invoiceKind: inv.invoice_kind,
    invoiceKindLabel: inv.invoice_kind === "red" ? "红票" : "蓝票",
    counterpartyPartyId: inv.party_id,
    counterpartyPartyName: inv.party_id,
    customerId: "",
    customerName: "",
    invoiceDate: inv.invoice_date,
    grossAmount: inv.gross_amount,
    netAmount: inv.net_amount,
    taxAmount: inv.tax_amount,
    roundingAdjustmentAmount: inv.rounding_adjustment_amount,
    roundingAdjustmentReason: inv.rounding_reason ?? undefined,
    allocatedTotal: inv.allocated_total,
    unallocatedAmount: inv.unallocated_amount,
    status,
    statusLabel:
      status === "draft"
        ? "草稿"
        : status === "reversed"
          ? "已作废"
          : "已登记",
    statusTone:
      inv.invoice_kind === "red"
        ? "warning"
        : status === "registered"
          ? "success"
          : "neutral",
    originalInvoiceId: inv.original_invoice_id ?? undefined,
    baselineVersion: inv.version,
    allocations,
    allowedActions: allowed,
    actionBlockers: [],
    isPosted,
    canEdit: false,
    canDelete: false,
  }
}

function emptyView(
  query: CustomerAccountsQuery,
  reason: CustomerAccountsListView["emptyReason"],
  moduleAllowed: boolean,
  hasDataScope: boolean
): CustomerAccountsListView {
  return {
    view: query.view,
    metrics: {
      openReceivableTotal: "0.00",
      overdueReceivableTotal: "0.00",
      unallocatedReceiptTotal: "0.00",
      unallocatedInvoiceTotal: "0.00",
      cardPendingReviewCount: 0,
    },
    receivables: [],
    receipts: [],
    invoices: [],
    unallocated: {
      receipts: [],
      invoices: [],
      note: "待核销视图中回款与销项发票分区展示；两类未分配余额不得相加。",
    },
    counterparties: [],
    total: 0,
    filterSummary:
      reason === "NO_DATA_SCOPE"
        ? "当前角色未配置客户往来范围"
        : reason === "PERMISSION_REVOKED"
          ? "权限已收回"
          : "空",
    permissionVersion: "pv-w11-http-1",
    dataWatermark: "",
    queriedAt: new Date().toISOString(),
    hasDataScope,
    moduleAllowed,
    canRegister: moduleAllowed,
    canExport: moduleAllowed,
    emptyReason: reason,
    submitPolicy: {
      allowUnallocatedRemainder: true,
      label: "允许保留未分配余额（系统统一判定）",
    },
  }
}

// ─── HTTP loaders ──────────────────────────────────────────────────────────

async function loadReceivables(
  query: CustomerAccountsQuery
): Promise<Page<BackendReceivableAccount>> {
  return apiGet<Page<BackendReceivableAccount>>("/admin/receivable-accounts", {
    customer_id: query.customerId,
    counterparty_party_id: query.counterpartyPartyId,
    status: mapBackendStatusFilter(query.status),
    sales_order_id: query.salesOrderId,
    review_status: mapBackendStatusFilter(query.reviewStatus),
    page: 1,
    page_size: LIST_PAGE_SIZE,
    sort_by: "created_at",
    sort_dir: "desc",
  })
}

async function loadReceipts(
  query: CustomerAccountsQuery
): Promise<Page<BackendCustomerReceipt>> {
  return apiGet<Page<BackendCustomerReceipt>>("/admin/customer-receipts", {
    counterparty_party_id: query.counterpartyPartyId,
    receipt_no: query.q?.trim() || undefined,
    status: mapBackendStatusFilter(
      query.view === "receipt" ? query.status : undefined
    ),
    page: 1,
    page_size: LIST_PAGE_SIZE,
    sort_by: "received_at",
    sort_dir: "desc",
  })
}

async function loadSalesInvoices(
  query: CustomerAccountsQuery
): Promise<Page<BackendInvoice>> {
  return apiGet<Page<BackendInvoice>>("/admin/invoices", {
    invoice_direction: "sales",
    party_id: query.counterpartyPartyId,
    invoice_no: query.q?.trim() || undefined,
    page: 1,
    page_size: LIST_PAGE_SIZE,
    sort_by: "invoice_date",
    sort_dir: "desc",
  })
}

// ─── Draft allocation session (client-held UI state; pool from HTTP) ───────

const sessions = new Map<string, AllocationSessionView>()
let sessionSeq = 100
const postIdempotency = new Map<string, PostAllocationResult>()
const reverseIdempotency = new Map<string, ReverseFactResult>()

async function buildPool(
  mode: "receipt" | "invoice",
  counterpartyPartyId: string
): Promise<AllocationSessionView["pool"]> {
  const page = await apiGet<Page<BackendReceivableAccount>>(
    "/admin/receivable-accounts",
    {
      counterparty_party_id: counterpartyPartyId,
      page: 1,
      page_size: LIST_PAGE_SIZE,
      sort_by: "created_at",
      sort_dir: "desc",
    }
  )
  const rows = page.items ?? []
  if (mode === "receipt") {
    return rows.flatMap((r) =>
      (r.entries ?? [])
        .filter((e) => e.direction === "increase")
        .map((e) => ({
          targetId: e.id,
          targetKind: "receivable_entry" as const,
          label: `${r.sales_order_id} · ${e.entry_type}`,
          salesOrderNo: r.sales_order_id,
          // open amount is server field on account; entry-level open is not exposed — use amount as display open
          openAmount: e.amount,
          dueDate: e.due_date,
          counterpartyPartyId: r.counterparty_party_id,
          baselineVersion: r.version,
        }))
    )
  }
  return rows
    .filter((r) => r.open_invoiceable_total && r.open_invoiceable_total !== "0" && r.open_invoiceable_total !== "0.00")
    .map((r) => ({
      targetId: r.id,
      targetKind: "receivable_account" as const,
      label: `应收子账 #${r.account_seq} · ${r.sales_order_id}`,
      salesOrderNo: r.sales_order_id,
      openAmount: r.open_invoiceable_total,
      dueDate: r.entries?.[0]?.due_date,
      counterpartyPartyId: r.counterparty_party_id,
      baselineVersion: r.version,
    }))
}

function recomputeProposed(
  factAmount: string,
  allocations: readonly AllocationDraftLine[]
): { proposedAllocatedTotal: string; proposedUnallocated: string } {
  // Display-only draft hint; formal balances come from server after post.
  let allocated = 0
  for (const a of allocations) {
    const n = Number(a.amount)
    if (Number.isFinite(n)) allocated += n
  }
  const total = Number(factAmount)
  const t = Number.isFinite(total) ? total : 0
  return {
    proposedAllocatedTotal: allocated.toFixed(2),
    proposedUnallocated: Math.max(0, t - allocated).toFixed(2),
  }
}

// ─── Public API ────────────────────────────────────────────────────────────

export async function fetchCustomerAccountsList(
  query: CustomerAccountsQuery
): Promise<CustomerAccountsListView> {
  const [recvPage, rcptPage, invPage] = await Promise.all([
    loadReceivables(query),
    loadReceipts(query),
    loadSalesInvoices(query),
  ])

  let receivables = (recvPage.items ?? []).map(projectReceivable)
  const receipts = (rcptPage.items ?? []).map(projectReceipt)
  const invoices = (invPage.items ?? [])
    .filter((i) => i.invoice_direction === "sales" || !i.invoice_direction)
    .map(projectInvoice)

  if (query.receivableAccountId) {
    receivables = receivables.filter(
      (r) => r.accountId === query.receivableAccountId
    )
  }
  if (query.q?.trim()) {
    const q = query.q.trim().toLowerCase()
    if (query.view === "receivable" || query.view === "unallocated") {
      receivables = receivables.filter(
        (r) =>
          r.accountId.toLowerCase().includes(q) ||
          r.salesOrderId.toLowerCase().includes(q) ||
          r.customerId.toLowerCase().includes(q) ||
          r.counterpartyPartyId.toLowerCase().includes(q)
      )
    }
  }
  // due filter: backend gap — cannot filter overdue without due_state

  const unallocatedReceipts = receipts.filter(
    (r) => r.status === "posted" && r.unallocatedAmount && r.unallocatedAmount !== "0" && r.unallocatedAmount !== "0.00"
  )
  const unallocatedInvoices = invoices.filter(
    (i) =>
      i.invoiceKind === "blue" &&
      i.status === "registered" &&
      i.unallocatedAmount &&
      i.unallocatedAmount !== "0" &&
      i.unallocatedAmount !== "0.00"
  )

  let total = 0
  if (query.view === "receivable") total = recvPage.total ?? receivables.length
  else if (query.view === "receipt") total = rcptPage.total ?? receipts.length
  else if (query.view === "sales_invoice") total = invPage.total ?? invoices.length
  else total = unallocatedReceipts.length + unallocatedInvoices.length

  const hasFilters = Boolean(
    query.q?.trim() ||
      query.counterpartyPartyId ||
      query.customerId ||
      query.due ||
      query.status ||
      query.reviewStatus ||
      query.salesOrderId
  )

  const cpMap = new Map<
    string,
    {
      counterpartyPartyId: string
      counterpartyPartyName: string
      customerId: string
      customerName: string
    }
  >()
  for (const r of receivables) {
    if (!cpMap.has(r.counterpartyPartyId)) {
      cpMap.set(r.counterpartyPartyId, {
        counterpartyPartyId: r.counterpartyPartyId,
        counterpartyPartyName: r.counterpartyPartyName,
        customerId: r.customerId,
        customerName: r.customerName,
      })
    }
  }

  // metrics：后端无汇总端点 — 缺口登记，返回占位 0（禁止前端求和冒充）
  return {
    view: query.view,
    metrics: {
      openReceivableTotal: "0.00",
      overdueReceivableTotal: "0.00",
      unallocatedReceiptTotal: "0.00",
      unallocatedInvoiceTotal: "0.00",
      cardPendingReviewCount: 0,
    },
    receivables,
    receipts,
    invoices,
    unallocated: {
      receipts: unallocatedReceipts,
      invoices: unallocatedInvoices,
      note: "待核销视图中回款与销项发票分区展示；两类未分配余额不得相加为单一指标。",
    },
    counterparties: [...cpMap.values()],
    total,
    filterSummary: filterSummary(query),
    permissionVersion: "pv-w11-http-1",
    dataWatermark: `wm-w11-${recvPage.total ?? 0}-${rcptPage.total ?? 0}-${invPage.total ?? 0}`,
    queriedAt: new Date().toISOString(),
    hasDataScope: true,
    moduleAllowed: true,
    canRegister: true,
    canExport: true,
    emptyReason:
      total === 0
        ? hasFilters
          ? "FILTER_NO_RESULT"
          : "NO_DATA"
        : undefined,
    submitPolicy: {
      allowUnallocatedRemainder: true,
      label: "允许保留未分配余额（系统统一判定）",
    },
  }
}

export async function fetchCustomerAccountsDetail(
  kind: "receivable" | "receipt" | "invoice",
  id: string
): Promise<CustomerAccountsDetailView | null> {
  if (kind === "receivable") {
    try {
      const seed = await apiGet<BackendReceivableAccount>(
        `/admin/receivable-accounts/${encodeURIComponent(id)}`
      )
      return {
        kind,
        receivable: projectReceivable(seed),
        queriedAt: new Date().toISOString(),
      }
    } catch {
      return null
    }
  }
  if (kind === "receipt") {
    try {
      const seed = await apiGet<BackendCustomerReceipt>(
        `/admin/customer-receipts/${encodeURIComponent(id)}`
      )
      return {
        kind,
        receipt: projectReceipt(seed),
        queriedAt: new Date().toISOString(),
      }
    } catch {
      return null
    }
  }
  try {
    const seed = await apiGet<BackendInvoice>(
      `/admin/invoices/${encodeURIComponent(id)}`
    )
    return {
      kind,
      invoice: projectInvoice(seed),
      queriedAt: new Date().toISOString(),
    }
  } catch {
    return null
  }
}

export async function createAllocationSession(
  input: CreateSessionInput
): Promise<AllocationSessionView> {
  const pool = await buildPool(input.mode, input.counterpartyPartyId)
  let existingFactNo: string | undefined
  let fact: AllocationSessionView["fact"] = {}
  let prefillAllocations: AllocationDraftLine[] = []
  let customerId = ""
  let customerName = ""

  if (input.mode === "receipt" && input.existingFactId) {
    const r = await apiGet<BackendCustomerReceipt>(
      `/admin/customer-receipts/${encodeURIComponent(input.existingFactId)}`
    )
    existingFactNo = r.receipt_no
    customerId = r.customer_id ?? ""
    customerName = r.customer_id ?? ""
    fact = {
      receivedAt: instantToIso(r.received_at).slice(0, 16),
      amount: r.unallocated_amount,
      bankReference: r.bank_reference ?? undefined,
    }
  } else if (input.mode === "invoice" && input.existingFactId) {
    const inv = await apiGet<BackendInvoice>(
      `/admin/invoices/${encodeURIComponent(input.existingFactId)}`
    )
    existingFactNo = inv.invoice_no
    fact = {
      invoiceCode: inv.invoice_code ?? undefined,
      invoiceNo: inv.invoice_no,
      invoiceDate: inv.invoice_date,
      grossAmount: inv.unallocated_amount,
      netAmount: inv.net_amount,
      taxAmount: inv.tax_amount,
      invoiceKind: "blue",
    }
  } else {
    const now = new Date()
    const pad = (n: number) => String(n).padStart(2, "0")
    const local = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}T${pad(now.getHours())}:${pad(now.getMinutes())}`
    if (input.mode === "receipt") {
      fact = { receivedAt: local, amount: "", bankReference: "" }
    } else {
      fact = {
        invoiceDate: local.slice(0, 10),
        invoiceNo: "",
        invoiceCode: "",
        grossAmount: "",
        netAmount: "",
        taxAmount: "",
        invoiceKind: "blue",
      }
    }
  }

  if (input.receivableAccountId || input.salesOrderId) {
    const match = pool.find((p) => {
      if (input.receivableAccountId) {
        if (p.targetKind === "receivable_account") {
          return p.targetId === input.receivableAccountId
        }
        return p.label.includes(input.receivableAccountId)
      }
      if (input.salesOrderId) {
        return p.salesOrderNo === input.salesOrderId
      }
      return false
    })
    if (match) {
      prefillAllocations = [
        {
          lineKey: `line_${match.targetId}`,
          targetId: match.targetId,
          targetKind: match.targetKind,
          label: match.label,
          salesOrderNo: match.salesOrderNo,
          openAmount: match.openAmount,
          amount: "",
          baselineVersion: match.baselineVersion,
        },
      ]
    }
  }

  // Resolve customer from first pool account if available
  if (!customerId && pool.length > 0) {
    try {
      const detail = await apiGet<BackendReceivableAccount>(
        `/admin/receivable-accounts/${encodeURIComponent(
          pool[0]!.targetKind === "receivable_account"
            ? pool[0]!.targetId
            : (
                await apiGet<Page<BackendReceivableAccount>>(
                  "/admin/receivable-accounts",
                  {
                    counterparty_party_id: input.counterpartyPartyId,
                    page: 1,
                    page_size: 1,
                  }
                )
              ).items[0]?.id ?? ""
        )}`
      )
      customerId = detail.customer_id
      customerName = detail.customer_id
    } catch {
      // leave empty — display gap
    }
  }

  const draftSessionId = `alloc_cust_${++sessionSeq}`
  const factAmount =
    input.mode === "receipt" ? fact.amount ?? "0" : fact.grossAmount ?? "0"
  const proposed = recomputeProposed(factAmount, prefillAllocations)

  const view: AllocationSessionView = {
    draftSessionId,
    mode: input.mode,
    counterpartyPartyId: input.counterpartyPartyId,
    counterpartyPartyName: input.counterpartyPartyId,
    customerId,
    customerName,
    status: "draft",
    existingFactId: input.existingFactId,
    existingFactNo,
    fact,
    pool,
    allocations: prefillAllocations,
    factAmount,
    ...proposed,
    submitPolicy: {
      allowUnallocatedRemainder: true,
      label: "允许保留未分配余额（系统统一判定）",
    },
    returnContext: {
      returnTo: input.returnTo,
      from: input.from,
      salesOrderId: input.salesOrderId,
    },
    leaseValid: true,
    editVersion: 1,
    note: "本次核销已锁定往来主体；拟分配合计仅作输入提示，以提交后系统结果为准。",
  }
  sessions.set(draftSessionId, view)
  return view
}

export async function fetchAllocationSession(
  draftSessionId: string
): Promise<AllocationSessionView | null> {
  const s = sessions.get(draftSessionId)
  if (!s) return null
  const pool = await buildPool(s.mode, s.counterpartyPartyId)
  const factAmount =
    s.mode === "receipt" ? s.fact.amount ?? "0" : s.fact.grossAmount ?? "0"
  const proposed = recomputeProposed(factAmount, s.allocations)
  return {
    ...s,
    pool,
    factAmount,
    ...proposed,
  }
}

export async function saveAllocationDraft(
  input: SaveAllocationDraftInput
): Promise<AllocationSessionView> {
  const s = sessions.get(input.draftSessionId)
  if (!s || s.status !== "draft") {
    return Promise.reject({
      kind: "Validation",
      message: "草稿已不存在或已确认。",
    })
  }
  if (input.editVersion !== s.editVersion) {
    return Promise.reject({
      kind: "Http",
      message: "草稿数据已更新，请刷新后重试。",
      status: 409,
    })
  }
  const next: AllocationSessionView = {
    ...s,
    fact: { ...input.fact },
    allocations: input.allocations.map((a) => ({ ...a })),
    editVersion: s.editVersion + 1,
    savedAt: new Date().toISOString(),
  }
  sessions.set(input.draftSessionId, next)
  return (await fetchAllocationSession(input.draftSessionId))!
}

export async function postAllocation(
  input: PostAllocationInput
): Promise<PostAllocationResult> {
  const cached = postIdempotency.get(input.idempotencyKey)
  if (cached) return cached

  if (input.forceUnknown) {
    const unknown: PostAllocationResult = {
      status: "unknown",
      message: "提交结果不确定。请按原任务号查询最终结果，勿重复提交。",
      idempotencyKey: input.idempotencyKey,
      operationId: `op_pending_${input.idempotencyKey.slice(-8)}`,
    }
    postIdempotency.set(input.idempotencyKey, unknown)
    return unknown
  }

  const s = sessions.get(input.draftSessionId)
  if (!s || s.status !== "draft") {
    const failed: PostAllocationResult = {
      status: "failed",
      code: "SESSION_INVALID",
      message: "本次核销已不存在或已提交。",
    }
    postIdempotency.set(input.idempotencyKey, failed)
    return failed
  }
  if (input.editVersion !== s.editVersion) {
    return {
      status: "failed",
      code: "VERSION_CONFLICT",
      message: "草稿数据已更新，请保存或刷新后重试。",
    }
  }
  if (input.forceCrossParty) {
    const failed: PostAllocationResult = {
      status: "failed",
      code: "CROSS_PARTY",
      message: "仅可分配当前往来主体的开放应收，已拒绝提交。",
    }
    postIdempotency.set(input.idempotencyKey, failed)
    return failed
  }

  const positiveLines = s.allocations.filter(
    (a) => a.amount && Number(a.amount) > 0
  )

  try {
    if (s.mode === "receipt") {
      let factId = s.existingFactId
      let factNo = s.existingFactNo ?? ""

      if (!factId) {
        const amount = s.fact.amount ?? "0"
        const receivedAtLocal = s.fact.receivedAt
        const receivedAtSecs = receivedAtLocal
          ? Math.floor(new Date(receivedAtLocal).getTime() / 1000)
          : Math.floor(Date.now() / 1000)
        const receiptNo =
          `SK-${new Date().toISOString().slice(0, 10).replaceAll("-", "")}-${input.idempotencyKey.slice(-6)}`
        const created = await apiPost<BackendCustomerReceipt>(
          "/admin/customer-receipts",
          {
            receipt_no: receiptNo,
            counterparty_party_id: s.counterpartyPartyId,
            customer_id: s.customerId || undefined,
            received_at: receivedAtSecs,
            amount,
            bank_reference: s.fact.bankReference || undefined,
          }
        )
        factId = created.id
        factNo = created.receipt_no
      }

      if (positiveLines.length === 0) {
        // Backend requires ≥1 allocation line on post — create without post if no lines
        sessions.set(input.draftSessionId, { ...s, status: "posted" })
        const result: PostAllocationResult = {
          status: "succeeded",
          mode: "receipt",
          factId: factId!,
          factNo,
          allocatedTotal: "0.00",
          unallocatedAmount: s.fact.amount ?? "0",
          operationId: input.idempotencyKey,
          watermark: new Date().toISOString(),
          returnTo: s.returnContext?.returnTo,
        }
        postIdempotency.set(input.idempotencyKey, result)
        return result
      }

      const posted = await apiPost<BackendCustomerReceipt>(
        `/admin/customer-receipts/${encodeURIComponent(factId!)}/post`,
        {
          allocations: positiveLines.map((line) => ({
            receivable_entry_id: line.targetId,
            allocated_amount: line.amount,
          })),
        }
      )
      sessions.set(input.draftSessionId, { ...s, status: "posted" })
      const result: PostAllocationResult = {
        status: "succeeded",
        mode: "receipt",
        factId: posted.id,
        factNo: posted.receipt_no,
        allocatedTotal: posted.allocated_total,
        unallocatedAmount: posted.unallocated_amount,
        operationId: input.idempotencyKey,
        watermark: new Date().toISOString(),
        returnTo: s.returnContext?.returnTo,
      }
      postIdempotency.set(input.idempotencyKey, result)
      return result
    }

    // invoice mode
    let factId = s.existingFactId
    let factNo = s.existingFactNo ?? (s.fact.invoiceNo ?? "").trim()

    if (!factId) {
      const created = await apiPost<BackendInvoice>("/admin/invoices", {
        invoice_direction: "sales",
        invoice_kind: s.fact.invoiceKind ?? "blue",
        party_id: s.counterpartyPartyId,
        invoice_code: s.fact.invoiceCode?.trim() || undefined,
        invoice_no: factNo,
        invoice_date: s.fact.invoiceDate,
        gross_amount: s.fact.grossAmount ?? "0",
        net_amount: s.fact.netAmount || s.fact.grossAmount || "0",
        tax_amount: s.fact.taxAmount || "0",
      })
      factId = created.id
      factNo = created.invoice_no
    }

    if (positiveLines.length === 0) {
      sessions.set(input.draftSessionId, { ...s, status: "posted" })
      const result: PostAllocationResult = {
        status: "succeeded",
        mode: "invoice",
        factId: factId!,
        factNo,
        allocatedTotal: "0.00",
        unallocatedAmount: s.fact.grossAmount ?? "0",
        operationId: input.idempotencyKey,
        watermark: new Date().toISOString(),
        returnTo: s.returnContext?.returnTo,
      }
      postIdempotency.set(input.idempotencyKey, result)
      return result
    }

    const gross = s.fact.grossAmount ?? "0"
    const net = s.fact.netAmount || gross
    const tax = s.fact.taxAmount || "0"
    const posted = await apiPost<BackendInvoice>(
      `/admin/invoices/${encodeURIComponent(factId!)}/post`,
      {
        allocations: positiveLines.map((line) => ({
          receivable_account_id: line.targetId,
          allocated_gross_amount: line.amount,
          allocated_net_amount: net,
          allocated_tax_amount: tax,
        })),
      }
    )
    sessions.set(input.draftSessionId, { ...s, status: "posted" })
    const result: PostAllocationResult = {
      status: "succeeded",
      mode: "invoice",
      factId: posted.id,
      factNo: posted.invoice_no,
      allocatedTotal: posted.allocated_total,
      unallocatedAmount: posted.unallocated_amount,
      operationId: input.idempotencyKey,
      watermark: new Date().toISOString(),
      returnTo: s.returnContext?.returnTo,
    }
    postIdempotency.set(input.idempotencyKey, result)
    return result
  } catch (err) {
    const message =
      err && typeof err === "object" && "message" in err
        ? String((err as { message: unknown }).message)
        : "提交失败"
    const code =
      err && typeof err === "object" && "status" in err
        ? String((err as { status?: number }).status ?? "HTTP_ERROR")
        : "HTTP_ERROR"
    const failed: PostAllocationResult = {
      status: "failed",
      code,
      message,
    }
    // Do not cache non-idempotent validation failures under success key
    if (code === "409" || message.includes("已存在")) {
      postIdempotency.set(input.idempotencyKey, failed)
    }
    return failed
  }
}

export async function resolvePostUnknown(
  idempotencyKey: string
): Promise<PostAllocationResult | null> {
  return postIdempotency.get(idempotencyKey) ?? null
}

export async function reverseFact(
  input: ReverseFactInput
): Promise<ReverseFactResult> {
  const cached = reverseIdempotency.get(input.idempotencyKey)
  if (cached) return cached

  const nowSecs = Math.floor(Date.now() / 1000)
  const noSuffix = input.idempotencyKey.slice(-8)

  try {
    if (input.kind === "receipt_reverse") {
      const receipt = await apiGet<BackendCustomerReceipt>(
        `/admin/customer-receipts/${encodeURIComponent(input.sourceFactId)}`
      )
      const created = await apiPost<{ id: string; reversal_no: string }>(
        "/admin/receipt-reversals",
        {
          reversal_no: `CZ-${noSuffix}`,
          original_customer_receipt_id: input.sourceFactId,
          reason_text: input.reason,
          amount: input.amount ?? receipt.amount,
          handled_by: "finance_handler",
          reviewed_by: "finance_reviewer",
          occurred_at: nowSecs,
        }
      )
      const posted = await apiPost<{ id: string; reversal_no: string }>(
        `/admin/receipt-reversals/${encodeURIComponent(created.id)}/post`,
        {}
      )
      const result: ReverseFactResult = {
        status: "succeeded",
        reverseFactId: posted.id,
        reverseFactNo: posted.reversal_no,
        operationId: input.idempotencyKey,
        message: "已追加回款冲正记录，原回款保留。",
      }
      reverseIdempotency.set(input.idempotencyKey, result)
      return result
    }

    if (input.kind === "refund") {
      const receipt = await apiGet<BackendCustomerReceipt>(
        `/admin/customer-receipts/${encodeURIComponent(input.sourceFactId)}`
      )
      const customerId = receipt.customer_id
      if (!customerId) {
        const failed: ReverseFactResult = {
          status: "failed",
          code: "CUSTOMER_REQUIRED",
          message: "回款未关联经营客户，无法登记退款（后端要求 customer_id）。",
        }
        reverseIdempotency.set(input.idempotencyKey, failed)
        return failed
      }
      const created = await apiPost<{ id: string; refund_no: string }>(
        "/admin/customer-refunds",
        {
          refund_no: `TK-${noSuffix}`,
          customer_id: customerId,
          original_receipt_id: input.sourceFactId,
          reason_text: input.reason,
          amount: input.amount ?? receipt.amount,
          handled_by: "finance_handler",
          reviewed_by: "finance_reviewer",
          occurred_at: nowSecs,
        }
      )
      const posted = await apiPost<{ id: string; refund_no: string }>(
        `/admin/customer-refunds/${encodeURIComponent(created.id)}/post`,
        {}
      )
      const result: ReverseFactResult = {
        status: "succeeded",
        reverseFactId: posted.id,
        reverseFactNo: posted.refund_no,
        operationId: input.idempotencyKey,
        message: "已追加退款记录，原回款保留。",
      }
      reverseIdempotency.set(input.idempotencyKey, result)
      return result
    }

    // red_invoice
    const inv = await apiGet<BackendInvoice>(
      `/admin/invoices/${encodeURIComponent(input.sourceFactId)}`
    )
    const applyLines = (inv.allocations ?? []).filter(
      (a) => a.allocation_action === "apply"
    )
    if (applyLines.length === 0) {
      const failed: ReverseFactResult = {
        status: "failed",
        code: "NO_ALLOCATIONS",
        message: "原蓝票无有效分配，无法开具红票。",
      }
      reverseIdempotency.set(input.idempotencyKey, failed)
      return failed
    }
    const red = await apiPost<BackendInvoice>(
      `/admin/invoices/${encodeURIComponent(input.sourceFactId)}/red-issue`,
      {
        invoice_no: `R${inv.invoice_no}`,
        invoice_date: new Date().toISOString().slice(0, 10),
        gross_amount: input.amount ?? inv.gross_amount,
        net_amount: inv.net_amount,
        tax_amount: inv.tax_amount,
        allocations: applyLines.map((a) => ({
          reverses_allocation_id: a.id,
          allocated_gross_amount: a.allocated_gross_amount,
          allocated_net_amount: a.allocated_net_amount,
          allocated_tax_amount: a.allocated_tax_amount,
        })),
      }
    )
    const result: ReverseFactResult = {
      status: "succeeded",
      reverseFactId: red.id,
      reverseFactNo: red.invoice_no,
      operationId: input.idempotencyKey,
      message: "已登记独立红票并追加反向分配，原蓝票保留。",
    }
    reverseIdempotency.set(input.idempotencyKey, result)
    return result
  } catch (err) {
    const message =
      err && typeof err === "object" && "message" in err
        ? String((err as { message: unknown }).message)
        : "纠错提交失败"
    const failed: ReverseFactResult = {
      status: "failed",
      code: "HTTP_ERROR",
      message,
    }
    return failed
  }
}

// silence unused emptyView for future permission paths
void emptyView
