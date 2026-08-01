/** W11 客户往来 · 客户端契约类型 */

export type CustomerAccountsView =
  | "receivable"
  | "receipt"
  | "sales_invoice"
  | "unallocated"

export type DueFilter = "not_due" | "due_today" | "overdue" | "all"

export type AllocationMode = "receipt" | "invoice"

export type CustomerAccountsQuery = {
  view: CustomerAccountsView
  counterpartyPartyId?: string
  customerId?: string
  q?: string
  status?: string
  due?: DueFilter
  reviewStatus?: string
  /** 列表预览焦点 */
  focusId?: string
  /** 来源销售单（W05 链入） */
  salesOrderId?: string
  receivableAccountId?: string
  /** 返回上下文 */
  returnTo?: string
  from?: string
}

export type AllowedAction =
  | "REGISTER_RECEIPT"
  | "REGISTER_INVOICE"
  | "CONTINUE_ALLOCATE"
  | "REVERSE_RECEIPT"
  | "REFUND"
  | "ISSUE_RED_INVOICE"
  | "EXPORT"
  | "VIEW_DETAIL"

export type ActionBlocker = Readonly<{
  action: AllowedAction | string
  code: string
  message: string
}>

export type StatusTone =
  | "success"
  | "destructive"
  | "warning"
  | "info"
  | "neutral"

export type CounterpartyOption = Readonly<{
  counterpartyPartyId: string
  counterpartyPartyName: string
  customerId: string
  customerName: string
}>

export type ReceivableEntry = Readonly<{
  entryId: string
  entryType: string
  direction: "increase" | "decrease"
  amountGross: string
  dueDate: string
  sourceLabel: string
  postedAt: string
  offsetOfEntryId?: string
}>

export type AllocationLine = Readonly<{
  allocationId: string
  action: "APPLY" | "REVERSE"
  amountGross: string
  targetLabel: string
  targetId: string
  occurredAt: string
  reverseOfAllocationId?: string
  isPosted: true
}>

export type ReceivableAccountRow = Readonly<{
  accountId: string
  accountSeq: number
  counterpartyPartyId: string
  counterpartyPartyName: string
  customerId: string
  customerName: string
  salesOrderId: string
  salesOrderNo: string
  businessType: "card" | "physical_service"
  businessTypeLabel: string
  grossTotal: string
  settledTotal: string
  openTotal: string
  invoicedTotal: string
  openInvoiceableTotal: string
  dueDate: string
  dueState: "not_due" | "due_today" | "overdue"
  dueStateLabel: string
  status: "open" | "partial" | "settled"
  statusLabel: string
  statusTone: StatusTone
  reviewStatus: "na" | "pending_opening" | "reviewed" | "pending_sync_diff"
  reviewStatusLabel: string
  baselineVersion: number
  entries: readonly ReceivableEntry[]
  allowedActions: readonly AllowedAction[]
  actionBlockers: readonly ActionBlocker[]
}>

export type ReceiptRow = Readonly<{
  receiptId: string
  receiptNo: string
  counterpartyPartyId: string
  counterpartyPartyName: string
  customerId: string
  customerName: string
  receivedAt: string
  amount: string
  bankReferenceMasked: string
  allocatedTotal: string
  unallocatedAmount: string
  status: "draft" | "posted" | "reversed"
  statusLabel: string
  statusTone: StatusTone
  baselineVersion: number
  allocations: readonly AllocationLine[]
  allowedActions: readonly AllowedAction[]
  actionBlockers: readonly ActionBlocker[]
  isPosted: boolean
  canEdit: false
  canDelete: false
}>

export type SalesInvoiceRow = Readonly<{
  invoiceId: string
  invoiceCode?: string
  invoiceNo: string
  invoiceKind: "blue" | "red"
  invoiceKindLabel: string
  counterpartyPartyId: string
  counterpartyPartyName: string
  customerId: string
  customerName: string
  invoiceDate: string
  grossAmount: string
  netAmount: string
  taxAmount: string
  roundingAdjustmentAmount?: string
  roundingAdjustmentReason?: string
  allocatedTotal: string
  unallocatedAmount: string
  status: "draft" | "registered" | "reversed"
  statusLabel: string
  statusTone: StatusTone
  originalInvoiceId?: string
  baselineVersion: number
  allocations: readonly AllocationLine[]
  allowedActions: readonly AllowedAction[]
  actionBlockers: readonly ActionBlocker[]
  isPosted: boolean
  canEdit: false
  canDelete: false
}>

export type CustomerAccountsMetrics = Readonly<{
  openReceivableTotal: string
  overdueReceivableTotal: string
  unallocatedReceiptTotal: string
  unallocatedInvoiceTotal: string
  cardPendingReviewCount: number
}>

export type UnallocatedSections = Readonly<{
  receipts: readonly ReceiptRow[]
  invoices: readonly SalesInvoiceRow[]
  /** 两类余额不可相加 — 服务端仅分别返回 */
  note: string
}>

export type CustomerAccountsListView = Readonly<{
  view: CustomerAccountsView
  metrics: CustomerAccountsMetrics
  receivables: readonly ReceivableAccountRow[]
  receipts: readonly ReceiptRow[]
  invoices: readonly SalesInvoiceRow[]
  unallocated: UnallocatedSections
  counterparties: readonly CounterpartyOption[]
  total: number
  filterSummary: string
  permissionVersion: string
  dataWatermark: string
  queriedAt: string
  hasDataScope: boolean
  moduleAllowed: boolean
  canRegister: boolean
  canExport: boolean
  emptyReason?: "NO_DATA" | "FILTER_NO_RESULT" | "NO_DATA_SCOPE" | "PERMISSION_REVOKED"
  /** 服务端提交策略：是否允许保留未分配余额 */
  submitPolicy: {
    allowUnallocatedRemainder: boolean
    label: string
  }
}>

export type DetailKind = "receivable" | "receipt" | "invoice"

export type CustomerAccountsDetailView = Readonly<{
  kind: DetailKind
  receivable?: ReceivableAccountRow
  receipt?: ReceiptRow
  invoice?: SalesInvoiceRow
  queriedAt: string
}>

export type AllocationTarget = Readonly<{
  targetId: string
  targetKind: "receivable_entry" | "receivable_account"
  label: string
  salesOrderNo: string
  openAmount: string
  dueDate?: string
  counterpartyPartyId: string
  baselineVersion: number
}>

export type AllocationDraftLine = {
  lineKey: string
  targetId: string
  targetKind: "receivable_entry" | "receivable_account"
  label: string
  salesOrderNo: string
  openAmount: string
  /** 用户输入，拟分配 */
  amount: string
  baselineVersion: number
}

export type AllocationSessionView = Readonly<{
  draftSessionId: string
  mode: AllocationMode
  counterpartyPartyId: string
  counterpartyPartyName: string
  customerId: string
  customerName: string
  status: "draft" | "posted"
  /** 已有回款/发票（继续核销） */
  existingFactId?: string
  existingFactNo?: string
  /** 记录表单草稿 */
  fact: {
    receivedAt?: string
    amount?: string
    bankReference?: string
    invoiceCode?: string
    invoiceNo?: string
    invoiceDate?: string
    grossAmount?: string
    netAmount?: string
    taxAmount?: string
    invoiceKind?: "blue" | "red"
    originalInvoiceId?: string
  }
  pool: readonly AllocationTarget[]
  allocations: readonly AllocationDraftLine[]
  /** 服务端返回的未分配（基于已保存草稿拟值，非正式） */
  proposedAllocatedTotal: string
  proposedUnallocated: string
  factAmount: string
  submitPolicy: {
    allowUnallocatedRemainder: boolean
    label: string
  }
  returnContext?: {
    returnTo?: string
    from?: string
    salesOrderId?: string
  }
  leaseValid: boolean
  editVersion: number
  savedAt?: string
  note: string
}>

export type SaveAllocationDraftInput = {
  draftSessionId: string
  fact: AllocationSessionView["fact"]
  allocations: readonly AllocationDraftLine[]
  editVersion: number
}

export type PostAllocationInput = {
  draftSessionId: string
  editVersion: number
  idempotencyKey: string
  /** 演示：强制结果不确定 */
  forceUnknown?: boolean
  /** 演示：强制跨主体拒绝 */
  forceCrossParty?: boolean
}

export type PostAllocationResult =
  | {
      status: "succeeded"
      mode: AllocationMode
      factId: string
      factNo: string
      allocatedTotal: string
      unallocatedAmount: string
      operationId: string
      watermark: string
      returnTo?: string
    }
  | {
      status: "failed"
      code: string
      message: string
      /** 并发时刷新后的开放余额提示 */
      refreshedTargets?: readonly {
        targetId: string
        openAmount: string
      }[]
      existingInvoiceId?: string
      existingInvoiceNo?: string
    }
  | {
      status: "unknown"
      message: string
      idempotencyKey: string
      operationId: string
    }

export type ReverseFactInput = {
  kind: "receipt_reverse" | "refund" | "red_invoice"
  sourceFactId: string
  amount?: string
  reason: string
  idempotencyKey: string
}

export type ReverseFactResult =
  | {
      status: "succeeded"
      reverseFactId: string
      reverseFactNo: string
      operationId: string
      message: string
    }
  | {
      status: "failed"
      code: string
      message: string
    }
  | {
      status: "unknown"
      message: string
      idempotencyKey: string
    }

export type CreateSessionInput = {
  mode: AllocationMode
  counterpartyPartyId: string
  existingFactId?: string
  salesOrderId?: string
  receivableAccountId?: string
  returnTo?: string
  from?: string
}

export const VIEW_LABEL: Record<CustomerAccountsView, string> = {
  receivable: "应收台账",
  receipt: "回款",
  sales_invoice: "销项发票",
  unallocated: "待核销",
}

export const DUE_LABEL: Record<DueFilter, string> = {
  all: "全部到期",
  not_due: "未到期",
  due_today: "今日到期",
  overdue: "已逾期",
}
