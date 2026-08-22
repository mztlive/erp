/**
 * Backend → client projection helpers (display only; no money recompute).
 */

import type {
    AllocationLine,
    AllowedAction,
    CustomerAccountsQuery,
    CustomerRefundRow,
    ReceiptReversalRow,
    ReceiptRow,
    ReceivableAccountRow,
    ReceivableEntry,
    SalesInvoiceRow,
} from "@/features/customer-receivables/types"
import { DUE_LABEL } from "@/features/customer-receivables/types"
import {
    customerReceiptStatusLabel,
    customerReceiptStatusTone,
    mapCustomerReceiptApproval,
} from "@/features/customer-receivables/lib/customer-receipt-approval"
import {
    customerRefundStatusLabel,
    customerRefundStatusTone,
    mapCustomerRefundApproval,
} from "@/features/customer-receivables/lib/customer-refund-approval"
import { stripInvoiceApprovalField } from "@/features/customer-receivables/lib/invoice-no-approval"
import {
    mapReceiptReversalApproval,
    receiptReversalStatusLabel,
    receiptReversalStatusTone,
} from "@/features/customer-receivables/lib/receipt-reversal-approval"

import type {
    BackendCustomerReceipt,
    BackendCustomerRefund,
    BackendInvoice,
    BackendInvoiceAllocation,
    BackendReceiptAllocation,
    BackendReceiptReversal,
    BackendReceivableAccount,
    BackendReceivableEntry,
} from "./dto"

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

function mapReceivableStatus(s: string): ReceivableAccountRow["status"] {
    if (s === "settled") return "settled"
    if (s === "partially_settled" || s === "partial") return "partial"
    return "open"
}

function mapReviewStatus(s: string): ReceivableAccountRow["reviewStatus"] {
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
    if (status === "settled")
        return { statusLabel: "已结清", statusTone: "success" }
    if (status === "partial")
        return { statusLabel: "部分结清", statusTone: "warning" }
    return { statusLabel: "未结", statusTone: "info" }
}

function mapReceiptStatus(s: string): ReceiptRow["status"] {
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

function mapRefundStatus(s: string): CustomerRefundRow["status"] {
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

function mapInvoiceStatus(s: string): SalesInvoiceRow["status"] {
    if (s === "reversed" || s === "red_invoiced") return "reversed"
    if (s === "registered") return "registered"
    return "draft"
}

function mapAllocationAction(a: string): "APPLY" | "REVERSE" {
    return a === "reverse" ? "REVERSE" : "APPLY"
}

export function mapBackendStatusFilter(
    status: string | undefined,
): string | undefined {
    if (!status) return undefined
    if (status === "partial") return "partially_settled"
    if (status === "pending_opening") return "opening_pending"
    if (status === "pending_sync_diff") return "sync_delta_pending"
    if (status === "na") return "not_applicable"
    return status
}

export function filterSummary(query: CustomerAccountsQuery): string {
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

export function projectReceivable(
    a: BackendReceivableAccount,
): ReceivableAccountRow {
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
        counterpartyPartyName: a.counterparty_party_name || a.counterparty_party_id,
        customerId: a.customer_id,
        customerName: a.customer_name || a.customer_id,
        salesOrderId: a.sales_order_id,
        salesOrderNo: a.sales_order_no || a.sales_order_id,
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

function projectReceiptAllocation(
    a: BackendReceiptAllocation,
): AllocationLine {
    return {
        allocationId: a.id,
        action: mapAllocationAction(a.allocation_action),
        amountGross: a.allocated_amount,
        targetLabel: a.receivable_entry_id,
        targetId: a.receivable_entry_id,
        occurredAt: instantToIso(a.allocated_at),
        reverseOfAllocationId: a.reverses_allocation_id ?? undefined,
        isPosted: true as const,
    }
}

export function projectReceipt(r: BackendCustomerReceipt): ReceiptRow {
    const status = mapReceiptStatus(r.status)
    const isPosted = status === "posted" || status === "reversed"
    const allowed: AllowedAction[] = ["VIEW_DETAIL"]
    if (status === "draft") {
        allowed.push("CONTINUE_ALLOCATE")
    }
    if (status === "posted") {
        allowed.push("REVERSE_RECEIPT", "REFUND")
    }
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
        statusLabel: customerReceiptStatusLabel(r.status),
        statusTone: customerReceiptStatusTone(r.status),
        baselineVersion: r.version,
        allocations: (r.allocations ?? []).map(projectReceiptAllocation),
        allowedActions: allowed,
        actionBlockers: [],
        isPosted,
        canEdit: false,
        canDelete: false,
        approval: mapCustomerReceiptApproval(r.approval),
    }
}

/**
 * 把客户退款详情投影为预览行。CustomerRefund 为 PROCESS_REQUIRED，只读映射审批绑定。
 *
 * @param refund 退款 HTTP 载荷。
 */
export function projectCustomerRefund(
    refund: BackendCustomerRefund,
): CustomerRefundRow {
    const status = mapRefundStatus(refund.status)
    return {
        refundId: refund.id,
        refundNo: refund.refund_no,
        customerId: refund.customer_id,
        originalReceiptId: refund.original_receipt_id ?? undefined,
        originalReceivableEntryId:
            refund.original_receivable_entry_id ?? undefined,
        reasonText: refund.reason_text,
        amount: refund.amount,
        occurredAt: instantToIso(refund.occurred_at),
        status,
        statusLabel: customerRefundStatusLabel(refund.status),
        statusTone: customerRefundStatusTone(refund.status),
        baselineVersion: refund.version,
        allowedActions: ["VIEW_DETAIL"],
        actionBlockers: [],
        approval: mapCustomerRefundApproval(refund.approval),
    }
}

function mapReversalStatus(s: string): ReceiptReversalRow["status"] {
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
 * 把回款冲正详情投影为预览行。ReceiptReversal 为 PROCESS_REQUIRED，只读映射审批绑定。
 *
 * @param reversal 冲正 HTTP 载荷。
 */
export function projectReceiptReversal(
    reversal: BackendReceiptReversal,
): ReceiptReversalRow {
    const status = mapReversalStatus(reversal.status)
    return {
        reversalId: reversal.id,
        reversalNo: reversal.reversal_no,
        originalReceiptId: reversal.original_customer_receipt_id,
        reasonText: reversal.reason_text,
        amount: reversal.amount,
        occurredAt: instantToIso(reversal.occurred_at),
        status,
        statusLabel: receiptReversalStatusLabel(reversal.status),
        statusTone: receiptReversalStatusTone(reversal.status),
        baselineVersion: reversal.version,
        allowedActions: ["VIEW_DETAIL"],
        actionBlockers: [],
        approval: mapReceiptReversalApproval(reversal.approval),
    }
}

function projectInvoiceAllocation(
    a: BackendInvoiceAllocation,
): AllocationLine {
    return {
        allocationId: a.id,
        action: mapAllocationAction(a.allocation_action),
        amountGross: a.allocated_gross_amount,
        targetLabel: a.receivable_account_id,
        targetId: a.receivable_account_id,
        occurredAt: "",
        reverseOfAllocationId: a.reverses_allocation_id ?? undefined,
        isPosted: true as const,
    }
}

/**
 * 把发票详情投影为列表/预览行。Invoice 为 NO_APPROVAL，丢弃误带的审批字段。
 *
 * @param inv 发票 HTTP 载荷。
 */
export function projectInvoice(inv: BackendInvoice): SalesInvoiceRow {
    const invoice = stripInvoiceApprovalField(inv)
    const status = mapInvoiceStatus(invoice.status)
    const isPosted = status === "registered" || status === "reversed"
    const allowed: AllowedAction[] = ["VIEW_DETAIL"]
    if (invoice.invoice_kind === "blue" && status === "registered") {
        allowed.push("CONTINUE_ALLOCATE", "ISSUE_RED_INVOICE")
    }
    return {
        invoiceId: invoice.id,
        invoiceCode: invoice.invoice_code ?? undefined,
        invoiceNo: invoice.invoice_no,
        invoiceKind: invoice.invoice_kind,
        invoiceKindLabel: invoice.invoice_kind === "red" ? "红票" : "蓝票",
        counterpartyPartyId: invoice.party_id,
        counterpartyPartyName: invoice.party_id,
        customerId: "",
        customerName: "",
        invoiceDate: invoice.invoice_date,
        grossAmount: invoice.gross_amount,
        netAmount: invoice.net_amount,
        taxAmount: invoice.tax_amount,
        roundingAdjustmentAmount: invoice.rounding_adjustment_amount,
        roundingAdjustmentReason: invoice.rounding_reason ?? undefined,
        allocatedTotal: invoice.allocated_total,
        unallocatedAmount: invoice.unallocated_amount,
        status,
        statusLabel:
            status === "draft"
                ? "草稿"
                : status === "reversed"
                  ? "已作废"
                  : "已登记",
        statusTone:
            invoice.invoice_kind === "red"
                ? "warning"
                : status === "registered"
                  ? "success"
                  : "neutral",
        originalInvoiceId: invoice.original_invoice_id ?? undefined,
        baselineVersion: invoice.version,
        allocations: (invoice.allocations ?? []).map(projectInvoiceAllocation),
        allowedActions: allowed,
        actionBlockers: [],
        isPosted,
        canEdit: false,
        canDelete: false,
    }
}
