/** W12 供应商往来 · 后端 DTO 与视图映射（纯函数，无 HTTP 无 React）。 */

import type { DocumentApprovalViewDto } from "@/features/approval-workflow/types"
import type {
    PayableRow,
    PaymentAllocationLine,
    PaymentReversalRow,
    PaymentRow,
    PurchaseInvoiceRow,
    SupplierAccountsQuery,
    SupplierRefundRow,
} from "@/features/supplier-payables/types"
import {
    ENTRY_TYPE_LABEL,
    PAYABLE_STATUS_LABEL,
    PAYABLE_STATUS_TONE,
    SOURCE_TYPE_LABEL,
    VIEW_LABEL,
} from "@/features/supplier-payables/types"
import {
    businessLabelOrPlaceholder,
    MISSING_SUPPLIER_NAME,
} from "@/features/supplier-payables/lib/display-labels"
import {
    missingSourceDocumentNo,
    payablePreviewHref,
    sourceDocumentHref,
} from "@/features/supplier-payables/lib/related-documents"
import {
    supplierPaymentStatusLabel,
    supplierPaymentStatusTone,
} from "@/features/supplier-payables/lib/supplier-payment"
import {
    mapPaymentReversalApproval,
    paymentReversalStatusLabel,
    paymentReversalStatusTone,
} from "@/features/supplier-payables/lib/payment-reversal-approval"
import {
    mapSupplierRefundApproval,
    supplierRefundStatusLabel,
    supplierRefundStatusTone,
} from "@/features/supplier-payables/lib/supplier-refund-approval"

// ─── Backend DTOs ──────────────────────────────────────────────────────────

type BackendPayableEntry = {
    id: string
    entry_type: string
    direction: "increase" | "decrease"
    amount: string
    due_date: string
    source_document_id: string
    source_document_no?: string | null
    source_sequence: number
    posted_at: number
}

type BackendPaymentRecipient = {
    bank_account_id: string
    version: number
    account_name: string
    bank_name: string
    bank_branch_name?: string | null
    account_number_masked: string
}

export type BackendPayableAccount = {
    id: string
    source_document_id: string
    source_document_no?: string | null
    supplier_id: string
    supplier_no?: string | null
    supplier_name?: string | null
    source_type: string
    gross_total: string
    settled_total: string
    open_total: string
    invoiceable_total: string
    invoiced_total: string
    open_invoiceable_total: string
    status: string
    version: number
    created_at: number
    entries: BackendPayableEntry[]
    payment_recipient?: BackendPaymentRecipient | null
}

type BackendPaymentAllocation = {
    id: string
    allocation_seq: number
    allocation_action: "apply" | "reverse"
    payable_entry_id: string
    payable_account_id?: string | null
    source_type?: string | null
    source_document_id?: string | null
    source_document_no?: string | null
    allocated_amount: string
    allocated_at: number
    reverses_allocation_id?: string | null
}

export type BackendSupplierPayment = {
    id: string
    payment_no: string
    status: string
    supplier_id: string
    supplier_no?: string | null
    supplier_name?: string | null
    payment_recipient?: BackendPaymentRecipient | null
    paid_at: number
    amount: string
    bank_reference?: string | null
    bank_receipt?: {
        asset_id: string
        file_name: string
        content_type: string
        byte_size: number
    } | null
    version: number
    created_at: number
    allocated_total: string
    unallocated_amount: string
    allocations: BackendPaymentAllocation[]
}

/** SupplierRefund 为 PROCESS_REQUIRED：创建/详情 DTO 必须携带只读审批绑定。 */
export type BackendSupplierRefund = {
    id: string
    refund_no: string
    status: string
    purchase_return_order_id?: string | null
    supplier_id: string
    original_payment_id?: string | null
    original_payable_entry_id?: string | null
    reason_code?: string | null
    reason_text: string
    amount: string
    handled_by: string
    reviewed_by: string
    occurred_at: number
    version: number
    created_at: number
    approval?: DocumentApprovalViewDto | null
}

/** PaymentReversal 为 PROCESS_REQUIRED：创建/详情 DTO 必须携带只读审批绑定。 */
export type BackendPaymentReversal = {
    id: string
    reversal_no: string
    status: string
    original_supplier_payment_id: string
    reason_code?: string | null
    reason_text: string
    amount: string
    handled_by: string
    reviewed_by: string
    occurred_at: number
    version: number
    created_at: number
    approval?: DocumentApprovalViewDto | null
}

export type BackendInvoice = {
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
    original_invoice_id?: string | null
    status: string
    version: number
    allocated_total: string
    unallocated_amount: string
    allocations: Array<{
        id: string
        allocation_seq: number
        allocation_action: "apply" | "reverse"
        payable_account_id?: string
        receivable_account_id?: string
        allocated_gross_amount: string
        reverses_allocation_id?: string | null
    }>
}

export type BackendPurchaseInvoiceAllocation = {
    id: string
    invoice_id: string
    allocation_seq: number
    allocation_action: "apply" | "reverse"
    payable_account_id: string
    allocated_gross_amount: string
    allocated_net_amount: string
    allocated_tax_amount: string
    reverses_allocation_id?: string | null
}

// ─── Mapping helpers ───────────────────────────────────────────────────────

export function instantToIso(secs: number | undefined | null): string {
    if (secs == null || !Number.isFinite(secs)) return ""
    return new Date(secs * 1000).toISOString()
}

function maskBank(raw?: string | null): string {
    const v = (raw ?? "").trim()
    if (!v) return "未填写"
    if (v.includes("*")) return v
    if (v.length <= 4) return `****${v}`
    return `****${v.slice(-4)}`
}

function mapPaymentRecipient(
    recipient?: BackendPaymentRecipient | null,
): PayableRow["paymentRecipient"] {
    if (!recipient) return undefined
    return {
        bankAccountId: recipient.bank_account_id,
        version: recipient.version,
        accountName: recipient.account_name,
        bankName: recipient.bank_name,
        bankBranchName: recipient.bank_branch_name ?? undefined,
        accountNumberMasked: recipient.account_number_masked,
    }
}

export function mapPayableStatus(s: string): PayableRow["status"] {
    if (s === "settled") return "SETTLED"
    if (s === "partially_settled" || s === "partial") return "PARTIAL"
    return "OPEN"
}

export function mapSourceType(s: string): PayableRow["sourceType"] {
    if (s === "supplier_settlement" || s === "SUPPLIER_SETTLEMENT") {
        return "SUPPLIER_SETTLEMENT"
    }
    return "PURCHASE_ORDER"
}

/**
 * 把后端付款状态映射为页面稳定码。
 *
 * @param s 服务端状态字面量。
 */
export function mapPaymentStatus(s: string): PaymentRow["status"] {
    if (s === "reversed" || s === "REVERSED") return "REVERSED"
    if (s === "posted" || s === "POSTED") return "POSTED"
    return "DRAFT"
}

function mapInvoiceStatus(s: string): PurchaseInvoiceRow["status"] {
    if (s === "reversed" || s === "red_invoiced") return "REVERSED"
    return "POSTED"
}

export function mapBackendPayableStatus(status?: string): string | undefined {
    if (!status) return undefined
    const u = status.toUpperCase()
    if (u === "OPEN") return "open"
    if (u === "PARTIAL" || u === "PARTIALLY_SETTLED") return "partially_settled"
    if (u === "SETTLED") return "settled"
    return status.toLowerCase()
}

export function mapBackendSourceType(t?: string): string | undefined {
    if (!t) return undefined
    if (t === "PURCHASE_ORDER" || t === "purchase_order")
        return "purchase_order"
    if (t === "SUPPLIER_SETTLEMENT" || t === "supplier_settlement") {
        return "supplier_settlement"
    }
    return t
}

export function projectPayable(a: BackendPayableAccount): PayableRow {
    const status = mapPayableStatus(a.status)
    const sourceType = mapSourceType(a.source_type)
    const primary = (a.entries ?? []).find((e) => e.direction === "increase")
    return {
        payableAccountId: a.id,
        supplierId: a.supplier_id,
        supplierName: businessLabelOrPlaceholder(
            a.supplier_name,
            a.supplier_id,
            MISSING_SUPPLIER_NAME,
        ),
        sourceType,
        sourceTypeLabel: SOURCE_TYPE_LABEL[sourceType],
        sourceDocumentId: a.source_document_id,
        sourceDocumentNo: businessLabelOrPlaceholder(
            a.source_document_no,
            a.source_document_id,
            missingSourceDocumentNo(sourceType),
        ),
        sourceHref: sourceDocumentHref(sourceType, a.source_document_id),
        primaryEntryId: primary?.id ?? a.entries?.[0]?.id ?? a.id,
        entryLockVersion: a.version,
        accountLockVersion: a.version,
        grossTotal: a.gross_total,
        settledTotal: a.settled_total,
        openTotal: a.open_total,
        invoicedTotal: a.invoiced_total,
        openInvoiceableTotal: a.open_invoiceable_total,
        dueDate: primary?.due_date ?? a.entries?.[0]?.due_date ?? "",
        dueState: "not_due",
        dueStateLabel: "未到期",
        status,
        statusLabel: PAYABLE_STATUS_LABEL[status],
        statusTone: PAYABLE_STATUS_TONE[status],
        paymentRecipient: mapPaymentRecipient(a.payment_recipient),
        allowedActions: ["VIEW_DETAIL", "REGISTER_PAYMENT", "REGISTER_INVOICE"],
        actionBlockers: [],
    }
}

/**
 * 把付款核销分配投影为可读来源行。分录 ID 不得当作来源单号。
 *
 * @param a 付款核销分配 HTTP 载荷。
 */
function projectPaymentAllocation(
    a: BackendPaymentAllocation,
): PaymentAllocationLine {
    const sourceType = mapSourceType(a.source_type ?? "purchase_order")
    const payableAccountId = a.payable_account_id?.trim() ?? ""
    return {
        allocationId: a.id,
        action: a.allocation_action === "reverse" ? "REVERSE" : "APPLY",
        payableAccountId,
        payableEntryId: a.payable_entry_id,
        sourceType,
        sourceDocumentId: a.source_document_id ?? undefined,
        sourceDocumentNo: businessLabelOrPlaceholder(
            a.source_document_no,
            a.source_document_id ?? a.payable_entry_id,
            missingSourceDocumentNo(sourceType),
        ),
        sourceHref: sourceDocumentHref(sourceType, a.source_document_id),
        payableHref: payableAccountId
            ? payablePreviewHref(payableAccountId)
            : undefined,
        amount: a.allocated_amount,
        occurredAt: instantToIso(a.allocated_at),
        reverseOfAllocationId: a.reverses_allocation_id ?? undefined,
    }
}

/**
 * 应付分录类型的可读标签。未知类型不上屏枚举原值。
 *
 * @param entryType 后端分录类型。
 */
export function payableEntryTypeLabel(entryType: string | undefined): string {
    if (!entryType) return "应付分录"
    return ENTRY_TYPE_LABEL[entryType] ?? "应付分录"
}

/**
 * 把付款 HTTP 视图投影为列表/详情行。
 *
 * @param p 付款 HTTP 载荷。
 */
export function projectPayment(p: BackendSupplierPayment): PaymentRow {
    const status = mapPaymentStatus(p.status)
    const allowed: string[] = ["VIEW_DETAIL"]
    if (status === "POSTED") {
        allowed.push("REVERSE_PAYMENT", "REVERSE", "REFUND")
    }
    return {
        paymentId: p.id,
        paymentNo: p.payment_no,
        supplierId: p.supplier_id,
        supplierName: businessLabelOrPlaceholder(
            p.supplier_name,
            p.supplier_id,
            MISSING_SUPPLIER_NAME,
        ),
        paidAt: instantToIso(p.paid_at),
        amount: p.amount,
        bankReferenceMasked: maskBank(p.bank_reference),
        bankReceipt: p.bank_receipt
            ? {
                  assetId: p.bank_receipt.asset_id,
                  fileName: p.bank_receipt.file_name,
                  contentType: p.bank_receipt.content_type,
                  byteSize: p.bank_receipt.byte_size,
              }
            : undefined,
        allocatedTotal: p.allocated_total,
        unallocatedAmount: p.unallocated_amount,
        status,
        statusLabel: supplierPaymentStatusLabel(p.status),
        statusTone: supplierPaymentStatusTone(p.status),
        baselineVersion: p.version,
        allocations: (p.allocations ?? []).map(projectPaymentAllocation),
        allowedActions: allowed,
        actionBlockers: [],
        paymentRecipient: mapPaymentRecipient(p.payment_recipient),
    }
}

function mapRefundStatus(s: string): SupplierRefundRow["status"] {
    if (s === "reversed" || s === "REVERSED") return "reversed"
    if (s === "posted" || s === "POSTED") return "posted"
    if (
        s === "IN_APPROVAL" ||
        s === "in_approval" ||
        s === "pending_review" ||
        s === "PENDING_REVIEW"
    ) {
        return "in_approval"
    }
    return "draft"
}

/**
 * 把供应商退款详情投影为预览行。SupplierRefund 为 PROCESS_REQUIRED，只读映射审批绑定。
 *
 * @param refund 退款 HTTP 载荷。
 */
export function projectSupplierRefund(
    refund: BackendSupplierRefund,
): SupplierRefundRow {
    const status = mapRefundStatus(refund.status)
    return {
        refundId: refund.id,
        refundNo: refund.refund_no,
        supplierId: refund.supplier_id,
        originalPaymentId: refund.original_payment_id ?? undefined,
        originalPayableEntryId: refund.original_payable_entry_id ?? undefined,
        reasonText: refund.reason_text,
        amount: refund.amount,
        occurredAt: instantToIso(refund.occurred_at),
        status,
        statusLabel: supplierRefundStatusLabel(refund.status),
        statusTone: supplierRefundStatusTone(refund.status),
        baselineVersion: refund.version,
        allowedActions: ["VIEW_DETAIL"],
        actionBlockers: [],
        approval: mapSupplierRefundApproval(refund.approval),
    }
}

function mapReversalStatus(s: string): PaymentReversalRow["status"] {
    if (s === "reversed" || s === "REVERSED") return "reversed"
    if (s === "posted" || s === "POSTED") return "posted"
    if (
        s === "IN_APPROVAL" ||
        s === "in_approval" ||
        s === "pending_review" ||
        s === "PENDING_REVIEW"
    ) {
        return "in_approval"
    }
    return "draft"
}

/**
 * 把付款冲正详情投影为预览行。PaymentReversal 为 PROCESS_REQUIRED，只读映射审批绑定。
 *
 * @param reversal 冲正 HTTP 载荷。
 */
export function projectPaymentReversal(
    reversal: BackendPaymentReversal,
): PaymentReversalRow {
    const status = mapReversalStatus(reversal.status)
    return {
        reversalId: reversal.id,
        reversalNo: reversal.reversal_no,
        originalPaymentId: reversal.original_supplier_payment_id,
        reasonText: reversal.reason_text,
        amount: reversal.amount,
        occurredAt: instantToIso(reversal.occurred_at),
        status,
        statusLabel: paymentReversalStatusLabel(reversal.status),
        statusTone: paymentReversalStatusTone(reversal.status),
        baselineVersion: reversal.version,
        allowedActions: ["VIEW_DETAIL"],
        actionBlockers: [],
        approval: mapPaymentReversalApproval(reversal.approval),
    }
}

export function projectInvoice(inv: BackendInvoice): PurchaseInvoiceRow {
    const status = mapInvoiceStatus(inv.status)
    return {
        invoiceId: inv.id,
        invoiceCode: inv.invoice_code ?? "",
        invoiceNo: inv.invoice_no,
        invoiceKind: inv.invoice_kind === "red" ? "RED" : "BLUE",
        invoiceKindLabel: inv.invoice_kind === "red" ? "红票" : "蓝票",
        supplierId: inv.party_id,
        supplierName: businessLabelOrPlaceholder(
            undefined,
            inv.party_id,
            MISSING_SUPPLIER_NAME,
        ),
        invoiceDate: inv.invoice_date,
        grossAmount: inv.gross_amount,
        netAmount: inv.net_amount,
        taxAmount: inv.tax_amount,
        allocatedTotal: inv.allocated_total,
        unallocatedAmount: inv.unallocated_amount,
        status,
        statusLabel: status === "REVERSED" ? "已红冲" : "已登记",
        statusTone: inv.invoice_kind === "red" ? "warning" : "success",
        originalInvoiceId: inv.original_invoice_id ?? undefined,
        allocations: (inv.allocations ?? []).map((a) => {
            const payableAccountId =
                a.payable_account_id ?? a.receivable_account_id ?? ""
            return {
                allocationId: a.id,
                action:
                    a.allocation_action === "reverse" ? "REVERSE" : "APPLY",
                payableAccountId,
                sourceType: "PURCHASE_ORDER" as const,
                sourceDocumentNo: missingSourceDocumentNo("PURCHASE_ORDER"),
                payableHref: payableAccountId
                    ? payablePreviewHref(payableAccountId)
                    : undefined,
                amountGross: a.allocated_gross_amount,
                occurredAt: "",
                reverseOfAllocationId: a.reverses_allocation_id ?? undefined,
            }
        }),
        allowedActions:
            status === "POSTED" && inv.invoice_kind === "blue"
                ? ["VIEW_DETAIL", "CONTINUE_ALLOCATE", "ISSUE_RED"]
                : ["VIEW_DETAIL"],
        actionBlockers: [],
    }
}

export function filterSummary(query: SupplierAccountsQuery): string {
    const parts = [VIEW_LABEL[query.view]]
    if (query.sourceType) parts.push(SOURCE_TYPE_LABEL[query.sourceType])
    if (query.q?.trim()) parts.push(`「${query.q.trim()}」`)
    return parts.join(" · ")
}
