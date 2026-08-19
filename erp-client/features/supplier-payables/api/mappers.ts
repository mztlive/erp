/** W12 供应商往来 · 后端 DTO 与视图映射（纯函数，无 HTTP 无 React）。 */

import type { DocumentApprovalViewDto } from "@/features/approval-workflow/types"
import type {
    PayableRow,
    PaymentRow,
    PurchaseInvoiceRow,
    SupplierAccountsQuery,
} from "@/features/supplier-payables/types"
import {
    PAYABLE_STATUS_LABEL,
    PAYABLE_STATUS_TONE,
    SOURCE_TYPE_LABEL,
    VIEW_LABEL,
} from "@/features/supplier-payables/types"
import {
    mapSupplierPaymentApproval,
    supplierPaymentStatusLabel,
    supplierPaymentStatusTone,
} from "@/features/supplier-payables/lib/supplier-payment-approval"

// ─── Backend DTOs ──────────────────────────────────────────────────────────

type BackendPayableEntry = {
    id: string
    entry_type: string
    direction: "increase" | "decrease"
    amount: string
    due_date: string
    source_document_id: string
    source_sequence: number
    posted_at: number
}

export type BackendPayableAccount = {
    id: string
    source_document_id: string
    supplier_id: string
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
}

type BackendPaymentAllocation = {
    id: string
    allocation_seq: number
    allocation_action: "apply" | "reverse"
    payable_entry_id: string
    allocated_amount: string
    allocated_at: number
    reverses_allocation_id?: string | null
}

export type BackendSupplierPayment = {
    id: string
    payment_no: string
    status: string
    supplier_id: string
    paid_at: number
    amount: string
    bank_reference?: string | null
    version: number
    created_at: number
    allocated_total: string
    unallocated_amount: string
    allocations: BackendPaymentAllocation[]
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
    if (!v) return "****"
    if (v.includes("*")) return v
    if (v.length <= 4) return `****${v}`
    return `****${v.slice(-4)}`
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
 * 把后端付款状态映射为页面稳定码。审批中不得伪装成草稿或已过账。
 *
 * @param s 服务端状态字面量。
 */
export function mapPaymentStatus(s: string): PaymentRow["status"] {
    if (s === "reversed" || s === "REVERSED") return "REVERSED"
    if (s === "posted" || s === "POSTED") return "POSTED"
    if (
        s === "IN_APPROVAL" ||
        s === "in_approval" ||
        s === "pending_review" ||
        s === "PENDING_REVIEW"
    ) {
        return "IN_APPROVAL"
    }
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
        supplierName: a.supplier_id,
        sourceType,
        sourceTypeLabel: SOURCE_TYPE_LABEL[sourceType],
        sourceDocumentId: a.source_document_id,
        sourceDocumentNo: a.source_document_id,
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
        allowedActions: ["VIEW_DETAIL", "REGISTER_PAYMENT", "REGISTER_INVOICE"],
        actionBlockers: [],
    }
}

/**
 * 把付款 HTTP 视图投影为列表/详情行。审批绑定只透传，不推导责任。
 *
 * @param p 付款 HTTP 载荷。
 */
export function projectPayment(p: BackendSupplierPayment): PaymentRow {
    const status = mapPaymentStatus(p.status)
    const allowed: string[] = ["VIEW_DETAIL"]
    if (status === "DRAFT") {
        allowed.push("CONTINUE_ALLOCATE")
    }
    if (status === "POSTED") {
        allowed.push("REVERSE_PAYMENT", "REVERSE")
    }
    return {
        paymentId: p.id,
        paymentNo: p.payment_no,
        supplierId: p.supplier_id,
        supplierName: p.supplier_id,
        paidAt: instantToIso(p.paid_at),
        amount: p.amount,
        bankReferenceMasked: maskBank(p.bank_reference),
        allocatedTotal: p.allocated_total,
        unallocatedAmount: p.unallocated_amount,
        status,
        statusLabel: supplierPaymentStatusLabel(p.status),
        statusTone: supplierPaymentStatusTone(p.status),
        baselineVersion: p.version,
        allocations: (p.allocations ?? []).map((a) => ({
            allocationId: a.id,
            action: a.allocation_action === "reverse" ? "REVERSE" : "APPLY",
            payableAccountId: "",
            payableEntryId: a.payable_entry_id,
            sourceType: "PURCHASE_ORDER" as const,
            sourceDocumentNo: a.payable_entry_id,
            amount: a.allocated_amount,
            occurredAt: instantToIso(a.allocated_at),
            reverseOfAllocationId: a.reverses_allocation_id ?? undefined,
        })),
        allowedActions: allowed,
        actionBlockers: [],
        approval: mapSupplierPaymentApproval(p.approval),
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
        supplierName: inv.party_id,
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
        allocations: (inv.allocations ?? []).map((a) => ({
            allocationId: a.id,
            action: a.allocation_action === "reverse" ? "REVERSE" : "APPLY",
            payableAccountId:
                a.payable_account_id ?? a.receivable_account_id ?? "",
            sourceType: "PURCHASE_ORDER" as const,
            sourceDocumentNo: a.payable_account_id ?? "",
            amountGross: a.allocated_gross_amount,
            occurredAt: "",
            reverseOfAllocationId: a.reverses_allocation_id ?? undefined,
        })),
        allowedActions:
            status === "POSTED" && inv.invoice_kind === "blue"
                ? ["VIEW_DETAIL", "CONTINUE_ALLOCATE", "ISSUE_RED"]
                : ["VIEW_DETAIL"],
        actionBlockers: [],
    }
}

export function filterSummary(query: SupplierAccountsQuery): string {
    const parts = [VIEW_LABEL[query.view]]
    if (query.supplierId) parts.push(query.supplierId)
    if (query.sourceType) parts.push(SOURCE_TYPE_LABEL[query.sourceType])
    if (query.q?.trim()) parts.push(`「${query.q.trim()}」`)
    return parts.join(" · ")
}
