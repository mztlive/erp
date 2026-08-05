/**
 * W11 客户往来 API：queryFn / mutationFn 纯函数。
 * 开放余额与净分配一律来自服务端投影，禁止前端拟合计冒充结果。
 */

import { mockDelay } from "@/lib/mock-delay"
import { filterRowsBySearch } from "@/lib/filter-utils"
import {
  W11_COUNTERPARTIES,
  W11_DEMO_HAS_DATA_SCOPE,
  W11_METRICS_SEED,
} from "@/mock/customer-receivables"
import {
  createW11AllocationSession,
  getW11AllocationSession,
  getW11Invoice,
  getW11Receipt,
  getW11Receivable,
  isW11PermissionRevoked,
  listW11LiveInvoices,
  listW11LiveReceipts,
  listW11LiveReceivables,
  postW11Allocation,
  resolveW11PostUnknown,
  reverseW11Fact,
  saveW11AllocationDraft,
} from "@/features/customer-receivables/session"
import type {
  AllocationLine,
  AllowedAction,
  CreateSessionInput,
  CustomerAccountsDetailView,
  CustomerAccountsListView,
  CustomerAccountsQuery,
  PostAllocationInput,
  PostAllocationResult,
  ReceiptRow,
  ReceivableAccountRow,
  ReverseFactInput,
  ReverseFactResult,
  SalesInvoiceRow,
  SaveAllocationDraftInput,
} from "@/features/customer-receivables/types"
import { DUE_LABEL } from "@/features/customer-receivables/types"

const PERMISSION_VERSION = "pv-w11-demo-1"

function parseMoney(v: string): number {
  const n = Number(v)
  return Number.isFinite(n) ? n : 0
}

function reviewLabel(
  s: ReceivableAccountRow["reviewStatus"]
): string {
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

function dueLabel(s: ReceivableAccountRow["dueState"]): string {
  switch (s) {
    case "overdue":
      return "已逾期"
    case "due_today":
      return "今日到期"
    default:
      return "未到期"
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

function projectAllocations(
  rows: readonly {
    allocationId: string
    action: "APPLY" | "REVERSE"
    amountGross: string
    targetLabel: string
    targetId: string
    occurredAt: string
    reverseOfAllocationId?: string
  }[]
): AllocationLine[] {
  return rows.map((a) => ({
    ...a,
    isPosted: true as const,
  }))
}

function projectReceivable(
  seed: ReturnType<typeof listW11LiveReceivables>[number]
): ReceivableAccountRow {
  const can = !isW11PermissionRevoked()
  const meta = statusMeta(seed.status)
  const allowed: AllowedAction[] = can
    ? ["VIEW_DETAIL", "REGISTER_RECEIPT", "REGISTER_INVOICE"]
    : ["VIEW_DETAIL"]
  return {
    accountId: seed.accountId,
    accountSeq: seed.accountSeq,
    counterpartyPartyId: seed.counterpartyPartyId,
    counterpartyPartyName: seed.counterpartyPartyName,
    customerId: seed.customerId,
    customerName: seed.customerName,
    salesOrderId: seed.salesOrderId,
    salesOrderNo: seed.salesOrderNo,
    businessType: seed.businessType,
    businessTypeLabel:
      seed.businessType === "card" ? "卡券" : "实物服务",
    grossTotal: seed.grossTotal,
    settledTotal: seed.settledTotal,
    openTotal: seed.openTotal,
    invoicedTotal: seed.invoicedTotal,
    openInvoiceableTotal: seed.openInvoiceableTotal,
    dueDate: seed.dueDate,
    dueState: seed.dueState,
    dueStateLabel: dueLabel(seed.dueState),
    status: seed.status,
    ...meta,
    reviewStatus: seed.reviewStatus,
    reviewStatusLabel: reviewLabel(seed.reviewStatus),
    baselineVersion: seed.baselineVersion,
    entries: seed.entries,
    allowedActions: allowed,
    actionBlockers: can
      ? []
      : [
          {
            action: "REGISTER_RECEIPT",
            code: "PERMISSION_REVOKED",
            message: "当前账号无登记/核销权限。",
          },
        ],
  }
}

function projectReceipt(
  seed: ReturnType<typeof listW11LiveReceipts>[number]
): ReceiptRow {
  const can = !isW11PermissionRevoked()
  const isPosted = seed.status === "posted" || seed.status === "reversed"
  const hasUnalloc = parseMoney(seed.unallocatedAmount) > 0
  const allowed: AllowedAction[] = ["VIEW_DETAIL"]
  if (can && seed.status === "posted" && hasUnalloc) {
    allowed.push("CONTINUE_ALLOCATE")
  }
  if (can && seed.status === "posted") {
    allowed.push("REVERSE_RECEIPT", "REFUND")
  }
  const statusLabel =
    seed.status === "draft"
      ? "草稿"
      : seed.status === "reversed"
        ? "已冲正"
        : "已确认"
  const statusTone =
    seed.status === "reversed"
      ? "destructive"
      : seed.status === "posted"
        ? "success"
        : "neutral"
  return {
    receiptId: seed.receiptId,
    receiptNo: seed.receiptNo,
    counterpartyPartyId: seed.counterpartyPartyId,
    counterpartyPartyName: seed.counterpartyPartyName,
    customerId: seed.customerId,
    customerName: seed.customerName,
    receivedAt: seed.receivedAt,
    amount: seed.amount,
    bankReferenceMasked: seed.bankReferenceMasked,
    allocatedTotal: seed.allocatedTotal,
    unallocatedAmount: seed.unallocatedAmount,
    status: seed.status,
    statusLabel,
    statusTone,
    baselineVersion: seed.baselineVersion,
    allocations: projectAllocations(seed.allocations),
    allowedActions: allowed,
    actionBlockers: [],
    isPosted,
    canEdit: false,
    canDelete: false,
  }
}

function projectInvoice(
  seed: ReturnType<typeof listW11LiveInvoices>[number]
): SalesInvoiceRow {
  const can = !isW11PermissionRevoked()
  const isPosted = seed.status === "registered" || seed.status === "reversed"
  const hasUnalloc =
    seed.invoiceKind === "blue" && parseMoney(seed.unallocatedAmount) > 0
  const allowed: AllowedAction[] = ["VIEW_DETAIL"]
  if (can && hasUnalloc) allowed.push("CONTINUE_ALLOCATE")
  if (can && seed.invoiceKind === "blue" && seed.status === "registered") {
    allowed.push("ISSUE_RED_INVOICE")
  }
  return {
    invoiceId: seed.invoiceId,
    invoiceCode: seed.invoiceCode,
    invoiceNo: seed.invoiceNo,
    invoiceKind: seed.invoiceKind,
    invoiceKindLabel: seed.invoiceKind === "red" ? "红票" : "蓝票",
    counterpartyPartyId: seed.counterpartyPartyId,
    counterpartyPartyName: seed.counterpartyPartyName,
    customerId: seed.customerId,
    customerName: seed.customerName,
    invoiceDate: seed.invoiceDate,
    grossAmount: seed.grossAmount,
    netAmount: seed.netAmount,
    taxAmount: seed.taxAmount,
    roundingAdjustmentAmount: seed.roundingAdjustmentAmount,
    roundingAdjustmentReason: seed.roundingAdjustmentReason,
    allocatedTotal: seed.allocatedTotal,
    unallocatedAmount: seed.unallocatedAmount,
    status: seed.status,
    statusLabel:
      seed.status === "draft"
        ? "草稿"
        : seed.status === "reversed"
          ? "已作废"
          : "已登记",
    statusTone:
      seed.invoiceKind === "red"
        ? "warning"
        : seed.status === "registered"
          ? "success"
          : "neutral",
    originalInvoiceId: seed.originalInvoiceId,
    baselineVersion: seed.baselineVersion,
    allocations: projectAllocations(seed.allocations),
    allowedActions: allowed,
    actionBlockers: [],
    isPosted,
    canEdit: false,
    canDelete: false,
  }
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
  if (query.counterpartyPartyId) {
    const cp = W11_COUNTERPARTIES.find(
      (c) => c.counterpartyPartyId === query.counterpartyPartyId
    )
    parts.push(cp?.counterpartyPartyName ?? query.counterpartyPartyId)
  }
  if (query.customerId) parts.push("已按经营客户过滤")
  if (query.q?.trim()) parts.push(`「${query.q.trim()}」`)
  if (query.due && query.due !== "all") {
    parts.push(DUE_LABEL[query.due])
  }
  return parts.join(" · ")
}

/**
 * 指标由服务端对授权范围投影返回。
 * 此处基于 live 集合重投影，模拟服务端水位，不是前端列表求和冒充。
 */
function projectMetrics(
  receivables: readonly ReceivableAccountRow[],
  receipts: readonly ReceiptRow[],
  invoices: readonly SalesInvoiceRow[]
): CustomerAccountsListView["metrics"] {
  // 演示：优先使用 seed 水位，会话变更后按 live 重算（仍是 mock 服务端职责）
  const openReceivableTotal = receivables
    .reduce((s, r) => s + parseMoney(r.openTotal), 0)
    .toFixed(2)
  const overdueReceivableTotal = receivables
    .filter((r) => r.dueState === "overdue")
    .reduce((s, r) => s + parseMoney(r.openTotal), 0)
    .toFixed(2)
  const unallocatedReceiptTotal = receipts
    .filter((r) => r.status === "posted")
    .reduce((s, r) => s + parseMoney(r.unallocatedAmount), 0)
    .toFixed(2)
  const unallocatedInvoiceTotal = invoices
    .filter((i) => i.invoiceKind === "blue" && i.status === "registered")
    .reduce((s, i) => s + parseMoney(i.unallocatedAmount), 0)
    .toFixed(2)
  const cardPendingReviewCount = receivables.filter(
    (r) =>
      r.reviewStatus === "pending_opening" ||
      r.reviewStatus === "pending_sync_diff"
  ).length

  // 无 live 变动时与 seed 一致；有会话过账后反映服务端最新水位
  if (
    listW11LiveReceivables().length === 6 &&
    listW11LiveReceipts().length === 3 &&
    listW11LiveInvoices().length === 2
  ) {
    return { ...W11_METRICS_SEED }
  }
  return {
    openReceivableTotal,
    overdueReceivableTotal,
    unallocatedReceiptTotal,
    unallocatedInvoiceTotal,
    cardPendingReviewCount,
  }
}

export async function fetchCustomerAccountsList(
  query: CustomerAccountsQuery
): Promise<CustomerAccountsListView> {
  await mockDelay(90)

  const moduleAllowed = !isW11PermissionRevoked()
  const hasDataScope = W11_DEMO_HAS_DATA_SCOPE

  if (!moduleAllowed) {
    return emptyView(query, "PERMISSION_REVOKED", false, false)
  }
  if (!hasDataScope) {
    return emptyView(query, "NO_DATA_SCOPE", true, false)
  }

  let receivables = listW11LiveReceivables().map(projectReceivable)
  let receipts = listW11LiveReceipts().map(projectReceipt)
  let invoices = listW11LiveInvoices().map(projectInvoice)

  if (query.counterpartyPartyId) {
    receivables = receivables.filter(
      (r) => r.counterpartyPartyId === query.counterpartyPartyId
    )
    receipts = receipts.filter(
      (r) => r.counterpartyPartyId === query.counterpartyPartyId
    )
    invoices = invoices.filter(
      (r) => r.counterpartyPartyId === query.counterpartyPartyId
    )
  }
  if (query.customerId) {
    receivables = receivables.filter((r) => r.customerId === query.customerId)
    receipts = receipts.filter((r) => r.customerId === query.customerId)
    invoices = invoices.filter((r) => r.customerId === query.customerId)
  }
  if (query.salesOrderId) {
    receivables = receivables.filter(
      (r) => r.salesOrderId === query.salesOrderId
    )
  }
  if (query.receivableAccountId) {
    receivables = receivables.filter(
      (r) => r.accountId === query.receivableAccountId
    )
  }
  if (query.q?.trim()) {
    receivables = filterRowsBySearch(receivables, query.q, (r) => [
      r.counterpartyPartyName,
      r.customerName,
      r.salesOrderNo,
      r.accountId,
    ])
    receipts = filterRowsBySearch(receipts, query.q, (r) => [
      r.receiptNo,
      r.counterpartyPartyName,
      r.customerName,
      r.bankReferenceMasked,
    ])
    invoices = filterRowsBySearch(invoices, query.q, (r) => [
      r.invoiceNo,
      r.invoiceCode,
      r.counterpartyPartyName,
      r.customerName,
    ])
  }
  if (query.due && query.due !== "all") {
    receivables = receivables.filter((r) => r.dueState === query.due)
  }
  if (query.status) {
    receivables = receivables.filter((r) => r.status === query.status)
    if (query.view === "receipt") {
      receipts = receipts.filter((r) => r.status === query.status)
    }
  }
  if (query.reviewStatus) {
    receivables = receivables.filter(
      (r) => r.reviewStatus === query.reviewStatus
    )
  }

  const unallocatedReceipts = receipts.filter(
    (r) => r.status === "posted" && parseMoney(r.unallocatedAmount) > 0
  )
  const unallocatedInvoices = invoices.filter(
    (i) =>
      i.invoiceKind === "blue" &&
      i.status === "registered" &&
      parseMoney(i.unallocatedAmount) > 0
  )

  let total = 0
  if (query.view === "receivable") total = receivables.length
  else if (query.view === "receipt") total = receipts.length
  else if (query.view === "sales_invoice") total = invoices.length
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
  const scopedEmpty =
    listW11LiveReceivables().length === 0 &&
    listW11LiveReceipts().length === 0 &&
    listW11LiveInvoices().length === 0

  const metrics = projectMetrics(
    listW11LiveReceivables().map(projectReceivable),
    listW11LiveReceipts().map(projectReceipt),
    listW11LiveInvoices().map(projectInvoice)
  )

  return {
    view: query.view,
    metrics,
    receivables,
    receipts,
    invoices,
    unallocated: {
      receipts: unallocatedReceipts,
      invoices: unallocatedInvoices,
      note: "待核销视图中回款与销项发票分区展示；两类未分配余额不得相加为单一指标。",
    },
    counterparties: [...W11_COUNTERPARTIES],
    total,
    filterSummary: filterSummary(query),
    permissionVersion: PERMISSION_VERSION,
    dataWatermark: `wm-w11-${Date.now()}`,
    queriedAt: new Date().toISOString(),
    hasDataScope: true,
    moduleAllowed: true,
    canRegister: true,
    canExport: true,
    emptyReason: total === 0 ? (hasFilters || !scopedEmpty ? "FILTER_NO_RESULT" : "NO_DATA") : undefined,
    submitPolicy: {
      allowUnallocatedRemainder: true,
      label: "允许保留未分配余额（系统统一判定）",
    },
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
    permissionVersion: PERMISSION_VERSION,
    dataWatermark: "",
    queriedAt: new Date().toISOString(),
    hasDataScope,
    moduleAllowed,
    canRegister: false,
    canExport: false,
    emptyReason: reason,
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
  await mockDelay(70)
  if (kind === "receivable") {
    const seed = getW11Receivable(id)
    if (!seed) return null
    return {
      kind,
      receivable: projectReceivable(seed),
      queriedAt: new Date().toISOString(),
    }
  }
  if (kind === "receipt") {
    const seed = getW11Receipt(id)
    if (!seed) return null
    return {
      kind,
      receipt: projectReceipt(seed),
      queriedAt: new Date().toISOString(),
    }
  }
  const seed = getW11Invoice(id)
  if (!seed) return null
  return {
    kind,
    invoice: projectInvoice(seed),
    queriedAt: new Date().toISOString(),
  }
}

export async function createAllocationSession(input: CreateSessionInput) {
  await mockDelay(80)
  return createW11AllocationSession(input)
}

export async function fetchAllocationSession(draftSessionId: string) {
  await mockDelay(50)
  return getW11AllocationSession(draftSessionId)
}

export async function saveAllocationDraft(input: SaveAllocationDraftInput) {
  await mockDelay(60)
  return saveW11AllocationDraft(input)
}

export async function postAllocation(
  input: PostAllocationInput
): Promise<PostAllocationResult> {
  await mockDelay(140)
  return postW11Allocation(input)
}

export async function resolvePostUnknown(
  idempotencyKey: string
): Promise<PostAllocationResult | null> {
  await mockDelay(80)
  return resolveW11PostUnknown(idempotencyKey)
}

export async function reverseFact(
  input: ReverseFactInput
): Promise<ReverseFactResult> {
  await mockDelay(120)
  return reverseW11Fact(input)
}
