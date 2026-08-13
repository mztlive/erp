/** W12 供应商往来 · 客户端契约类型（对齐 w12-supplier-payables §5/§8） */

import type { StatusTone } from "@/components/ui/status-badge"

export type SupplierAccountsView =
    | "payable"
    | "payment"
    | "purchase_invoice"
    | "unallocated"

export type PayableSourceType = "PURCHASE_ORDER" | "SUPPLIER_SETTLEMENT"

export type PayableStatus = "OPEN" | "PARTIAL" | "SETTLED"

type PaymentStatus = "DRAFT" | "POSTED" | "REVERSED"

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

type PaymentAllocationLine = Readonly<{
    allocationId: string
    action: "APPLY" | "REVERSE"
    payableAccountId: string
    payableEntryId: string
    sourceType: PayableSourceType
    sourceDocumentNo: string
    amount: string
    occurredAt: string
    reverseOfAllocationId?: string
}>

export type PaymentRow = Readonly<{
    paymentId: string
    paymentNo: string
    supplierId: string
    supplierName: string
    paidAt: string
    amount: string
    bankReferenceMasked: string
    allocatedTotal: string
    unallocatedAmount: string
    status: PaymentStatus
    statusLabel: string
    statusTone: StatusTone
    allocations: readonly PaymentAllocationLine[]
    allowedActions: readonly string[]
    actionBlockers: readonly ActionBlocker[]
    reversedByPaymentId?: string
    reverseOfPaymentId?: string
}>

type InvoiceAllocationLine = Readonly<{
    allocationId: string
    action: "APPLY" | "REVERSE"
    payableAccountId: string
    sourceType: PayableSourceType
    sourceDocumentNo: string
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
    /** 继续核销已有付款/发票时带入 */
    existingPaymentId?: string
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
    draftSessionId: string
    supplierId: string
    paidAt: string
    amount: string
    bankReference: string
    note?: string
    targets: readonly AllocationTargetInput[]
    payablePriorityPolicyId?: string
    payablePriorityPolicyVersion?: number
    /** 用户显式逐项选择（策略缺失时必须） */
    explicitSelection: boolean
    existingPaymentId?: string
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
}

export type SaveAllocationDraftInput = {
    draftSessionId: string
    track: AllocationTrack
    supplierId: string
    formSnapshot: Record<string, unknown>
}

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

/** 页面核销会话状态（URL 深链与本地会话共用） */
export type SessionState = {
    track: AllocationTrack
    supplierId: string
    draftSessionId?: string
    purchaseOrderId?: string
    returnTo?: string
    fromWorkspace?: string
    existingPaymentId?: string
    existingInvoiceId?: string
    preselectPayableAccountId?: string
}

/** 冲正/红票弹窗目标 */
export type ReverseTarget =
    | { kind: "payment"; id: string; no: string }
    | { kind: "invoice"; id: string; no: string }
