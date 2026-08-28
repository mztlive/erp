/** W12 供应商往来 · 客户端契约类型（对齐 w12-supplier-payables §5/§8） */

import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type { StatusTone } from "@/components/ui/status-badge"

export type SupplierAccountsView =
    | "payable"
    | "payment"
    | "purchase_invoice"
    | "unallocated"

export type PayableSourceType = "PURCHASE_ORDER" | "SUPPLIER_SETTLEMENT"

export type PayableStatus = "OPEN" | "PARTIAL" | "SETTLED"

export type PaymentStatus = "DRAFT" | "POSTED" | "REVERSED"

type InvoiceKind = "BLUE" | "RED"

type InvoiceStatus = "POSTED" | "REVERSED"

export type AllocationTrack = "payment" | "purchase_invoice"

type PolicyState = "AVAILABLE" | "MISSING" | "STALE"

export type SupplierAccountsQuery = {
    view: SupplierAccountsView
    supplierId?: string
    sourceType?: PayableSourceType
    q?: string
    status?: string
    due?: "not_due" | "due_today" | "overdue" | "all"
    paymentGate?: "satisfied" | "unsatisfied" | "all"
    purchaseOrderId?: string
    pageSize?: number
}

type PayablePriorityPolicyView = {
    payablePriorityPolicyId?: string
    payablePriorityPolicyVersion?: number
    state: PolicyState
    mixedAutoAllocationAllowed: boolean
    blockerMessage?: string
}

type ActionBlocker = {
    action: string
    code: string
    message: string
}

/** 付款任务与付款事实使用的收款账户摘要；账号明文须另行受控揭示。 */
export type PaymentRecipient = Readonly<{
    bankAccountId: string
    version: number
    accountName: string
    bankName: string
    bankBranchName?: string
    accountNumberMasked: string
}>

export type PayableRow = Readonly<{
    payableAccountId: string
    supplierId: string
    supplierName: string
    sourceType: PayableSourceType
    sourceTypeLabel: string
    sourceDocumentId: string
    sourceDocumentNo: string
    sourceHref?: string
    /** 应付分录 ID（付款分配目标） */
    primaryEntryId: string
    entryLockVersion: number
    accountLockVersion: number
    grossTotal: string
    settledTotal: string
    openTotal: string
    invoicedTotal: string
    openInvoiceableTotal: string
    dueDate: string
    dueState: "not_due" | "due_today" | "overdue"
    dueStateLabel: string
    status: PayableStatus
    statusLabel: string
    statusTone: StatusTone
    paymentRecipient?: PaymentRecipient
    paymentGateSummary?: {
        state: "SATISFIED" | "BLOCKED" | "NOT_APPLICABLE"
        message: string
        required: string
        allocated: string
        gap: string
    }
    allowedActions: readonly string[]
    actionBlockers: readonly ActionBlocker[]
}>

export type PaymentAllocationLine = Readonly<{
    allocationId: string
    action: "APPLY" | "REVERSE"
    payableAccountId: string
    payableEntryId: string
    sourceType: PayableSourceType
    sourceDocumentId?: string
    sourceDocumentNo: string
    sourceHref?: string
    payableHref?: string
    amount: string
    occurredAt: string
    reverseOfAllocationId?: string
}>

/** 原付款下的冲正跟踪摘要；待审批记录不改变付款金额与付款状态。 */
export type PaymentReversalSummary = Readonly<{
    reversalId: string
    reversalNo: string
    reasonText: string
    amount: string
    occurredAt: string
    status: "draft" | "in_approval" | "posted" | "reversed"
    statusLabel: string
    statusTone: StatusTone
}>

export type PaymentRow = Readonly<{
    paymentId: string
    paymentNo: string
    supplierId: string
    supplierName: string
    paidAt: string
    amount: string
    bankReferenceMasked: string
    bankReceipt?: Readonly<{
        assetId: string
        fileName: string
        contentType: string
        byteSize: number
    }>
    allocatedTotal: string
    unallocatedAmount: string
    status: PaymentStatus
    statusLabel: string
    statusTone: StatusTone
    baselineVersion: number
    allocations: readonly PaymentAllocationLine[]
    allowedActions: readonly string[]
    actionBlockers: readonly ActionBlocker[]
    reversedByPaymentId?: string
    reverseOfPaymentId?: string
    paymentRecipient?: PaymentRecipient
    relatedReversals: readonly PaymentReversalSummary[]
}>

/** SupplierRefund 为 PROCESS_REQUIRED：行投影携带只读审批区。 */
export type SupplierRefundRow = Readonly<{
    refundId: string
    refundNo: string
    supplierId: string
    originalPaymentId?: string
    originalPayableEntryId?: string
    reasonText: string
    amount: string
    occurredAt: string
    status: "draft" | "in_approval" | "posted" | "reversed"
    statusLabel: string
    statusTone: StatusTone
    baselineVersion: number
    allowedActions: readonly string[]
    actionBlockers: readonly ActionBlocker[]
    approval?: DocumentApprovalView
}>

/** 从已过账付款发起供应商退款所需的原事实。 */
export type SupplierRefundRequest = Readonly<{
    sourcePaymentId: string
    sourcePaymentNo: string
    supplierId: string
    amount?: string
}>

/** PaymentReversal 为 PROCESS_REQUIRED：行投影携带只读审批区。 */
export type PaymentReversalRow = Readonly<{
    reversalId: string
    reversalNo: string
    originalPaymentId: string
    reasonText: string
    amount: string
    occurredAt: string
    status: "draft" | "in_approval" | "posted" | "reversed"
    statusLabel: string
    statusTone: StatusTone
    baselineVersion: number
    allowedActions: readonly string[]
    actionBlockers: readonly ActionBlocker[]
    approval?: DocumentApprovalView
}>

/** 从已过账付款发起付款冲正所需的原事实。 */
export type PaymentReversalRequest = Readonly<{
    sourcePaymentId: string
    sourcePaymentNo: string
    amount?: string
}>

type InvoiceAllocationLine = Readonly<{
    allocationId: string
    action: "APPLY" | "REVERSE"
    payableAccountId: string
    sourceType: PayableSourceType
    sourceDocumentNo: string
    sourceHref?: string
    payableHref?: string
    amountGross: string
    occurredAt: string
    reverseOfAllocationId?: string
}>

export type PurchaseInvoiceRow = Readonly<{
    invoiceId: string
    invoiceCode: string
    invoiceNo: string
    invoiceKind: InvoiceKind
    invoiceKindLabel: string
    supplierId: string
    supplierName: string
    invoiceDate: string
    grossAmount: string
    netAmount: string
    taxAmount: string
    allocatedTotal: string
    unallocatedAmount: string
    status: InvoiceStatus
    statusLabel: string
    statusTone: StatusTone
    originalInvoiceId?: string
    allocations: readonly InvoiceAllocationLine[]
    allowedActions: readonly string[]
    actionBlockers: readonly ActionBlocker[]
}>

export type UnallocatedRow = Readonly<{
    id: string
    track: AllocationTrack
    trackLabel: string
    documentNo: string
    supplierId: string
    supplierName: string
    amount: string
    unallocatedAmount: string
    occurredAt: string
    statusLabel: string
    statusTone: StatusTone
}>

type SupplierOption = Readonly<{
    supplierId: string
    supplierName: string
    openPayableTotal: string
    unallocatedPaymentTotal: string
    unallocatedInvoiceTotal: string
}>

type SupplierAccountsMetrics = Readonly<{
    openPayableTotal: string
    overduePayableTotal: string
    unallocatedPaymentTotal: string
    unallocatedInvoiceTotal: string
    prepayGateBlockedCount: number
}>

export type SupplierAccountsListView = Readonly<{
    view: SupplierAccountsView
    metrics: SupplierAccountsMetrics
    payables: readonly PayableRow[]
    payments: readonly PaymentRow[]
    invoices: readonly PurchaseInvoiceRow[]
    unallocated: readonly UnallocatedRow[]
    suppliers: readonly SupplierOption[]
    total: number
    filterSummary: string
    permissionVersion: string
    dataWatermark: string
    queriedAt: string
    moduleAllowed: boolean
    hasDataScope: boolean
    canRegisterPayment: boolean
    canRegisterInvoice: boolean
    canExport: boolean
    emptyReason?:
        | "NO_DATA"
        | "FILTER_NO_RESULT"
        | "NO_DATA_SCOPE"
        | "PERMISSION_REVOKED"
    payablePriorityPolicy: PayablePriorityPolicyView
    allowFullBankReveal: boolean
}>

export type PayableDetailView = Readonly<{
    payable: PayableRow
    entries: readonly {
        entryId: string
        entryTypeLabel: string
        direction: "increase" | "decrease"
        amount: string
        sourceLabel: string
        dueDate?: string
        occurredAt: string
    }[]
    paymentAllocations: readonly PaymentAllocationLine[]
    invoiceAllocations: readonly InvoiceAllocationLine[]
    dataWatermark: string
    queriedAt: string
}>

type AllocationPoolItem = Readonly<{
    payableAccountId: string
    primaryEntryId: string
    entryLockVersion: number
    accountLockVersion: number
    sourceType: PayableSourceType
    sourceTypeLabel: string
    sourceDocumentNo: string
    sourceDocumentId: string
    openTotal: string
    openInvoiceableTotal: string
    dueDate: string
    dueStateLabel: string
    statusLabel: string
}>

export type AllocationSessionView = Readonly<{
    draftSessionId: string
    track: AllocationTrack
    supplierId: string
    supplierName: string
    pool: readonly AllocationPoolItem[]
    payablePriorityPolicy: PayablePriorityPolicyView
    preselectedPayableAccountIds: readonly string[]
    purchaseOrderId?: string
    returnTo?: string
    fromWorkspace?: string
    dataWatermark: string
    queriedAt: string
    draftSavedAt?: string
    /** 继续核销已有发票时带入 */
    existingInvoiceId?: string
    existingAmount?: string
    existingUnallocated?: string
    existingDocumentNo?: string
}>

type AllocationTargetInput = {
    payableAccountId: string
    payableEntryId?: string
    amount: string
    entryLockVersion: number
    accountLockVersion: number
}

export type PostPaymentInput = {
    workItemId: string
    expectedTaskVersion: string
    expectedPayeeBankAccountId: string
    expectedPayeeBankAccountVersion: number
    draftSessionId: string
    supplierId: string
    paidAt: string
    amount: string
    bankReference: string
    bankReceiptAssetId: string
    bankReceiptFile: File | null
    note?: string
    targets: readonly AllocationTargetInput[]
    payablePriorityPolicyId?: string
    payablePriorityPolicyVersion?: number
    /** 用户显式逐项选择（策略缺失时必须） */
    explicitSelection: boolean
    idempotencyKey: string
}

export type PostInvoiceInput = {
    draftSessionId: string
    supplierId: string
    invoiceCode: string
    invoiceNo: string
    invoiceDate: string
    grossAmount: string
    netAmount: string
    taxAmount: string
    invoiceKind: InvoiceKind
    originalInvoiceId?: string
    targets: readonly AllocationTargetInput[]
    payablePriorityPolicyId?: string
    payablePriorityPolicyVersion?: number
    explicitSelection: boolean
    existingInvoiceId?: string
    idempotencyKey: string
}

export type ReversePaymentInput = {
    paymentId: string
    reason: string
    idempotencyKey: string
}

export type ReverseInvoiceInput = {
    invoiceId: string
    reason: string
    redInvoiceNo: string
    idempotencyKey: string
}

type FormalSubmitStatus = "succeeded" | "failed" | "unknown" | "blocked"

export type FormalSubmitResult = {
    status: FormalSubmitStatus
    title: string
    description: string
    reference?: string
    operationId?: string
    documentNo?: string
    unallocatedAmount?: string
    allocatedTotal?: string
    paymentGateRefreshHint?: string
    returnTo?: string
    existingDocumentId?: string
    errorCode?: string
    facts?: Array<{ label: string; value: string }>
    approval?: DocumentApprovalView
    subjectStatus?: string
}

export type SaveAllocationDraftInput = {
    draftSessionId: string
    track: AllocationTrack
    supplierId: string
    formSnapshot: Record<string, unknown>
}

/** 供应商往来四个工作视图，顺序即 Tab 展示顺序。 */
export const SUPPLIER_ACCOUNT_VIEWS = [
    "payable",
    "payment",
    "purchase_invoice",
    "unallocated",
] as const satisfies readonly SupplierAccountsView[]

export const VIEW_LABEL: Record<SupplierAccountsView, string> = {
    payable: "应付台账",
    payment: "付款",
    purchase_invoice: "进项发票",
    unallocated: "待核销",
}

export const SOURCE_TYPE_LABEL: Record<PayableSourceType, string> = {
    PURCHASE_ORDER: "采购单",
    SUPPLIER_SETTLEMENT: "供应商结算单",
}

export const ENTRY_TYPE_LABEL: Record<string, string> = {
    original: "原始应付",
    change_delta: "变更差额",
    supplier_refund: "供应商退款",
    reversal: "冲正",
    settlement_delta: "结算差额",
}

export const ALLOCATION_ACTION_LABEL: Record<"APPLY" | "REVERSE", string> = {
    APPLY: "核销",
    REVERSE: "冲减",
}

export const PAYABLE_STATUS_LABEL: Record<PayableStatus, string> = {
    OPEN: "未结",
    PARTIAL: "部分结清",
    SETTLED: "已结清",
}

export const PAYABLE_STATUS_TONE: Record<PayableStatus, StatusTone> = {
    OPEN: "warning",
    PARTIAL: "info",
    SETTLED: "success",
}

export const DUE_LABEL: Record<"not_due" | "due_today" | "overdue", string> = {
    not_due: "未到期",
    due_today: "今日到期",
    overdue: "已到期",
}

export const PAYMENT_GATE_LABEL: Record<"satisfied" | "unsatisfied", string> = {
    satisfied: "已满足",
    unsatisfied: "未满足",
}

export const TRACK_LABEL: Record<AllocationTrack, string> = {
    payment: "付款",
    purchase_invoice: "进项票",
}

/** 页面核销会话状态（URL 深链与本地会话共用） */
export type SessionState = {
    track: AllocationTrack
    supplierId: string
    draftSessionId?: string
    purchaseOrderId?: string
    returnTo?: string
    fromWorkspace?: string
    existingInvoiceId?: string
    preselectPayableAccountId?: string
}

/** 冲正/红票弹窗目标 */
export type ReverseTarget =
    | { kind: "payment"; id: string; no: string }
    | { kind: "invoice"; id: string; no: string }
