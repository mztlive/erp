/** Backend DTO shapes (snake_case; amounts as strings). */

import type { DocumentApprovalViewDto } from "@/features/approval-workflow/types"

export type BackendReceivableEntry = {
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

export type BackendFundsReview = {
    id: string
    review_no: number
    review_type: string
    review_result: string
    reviewed_by: string
    reviewed_at: number
    evidence_reference?: string | null
}

export type BackendReceivableAccount = {
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

export type BackendReceiptAllocation = {
    id: string
    allocation_seq: number
    allocation_action: "apply" | "reverse"
    receivable_entry_id: string
    allocated_amount: string
    allocated_at: number
    reverses_allocation_id?: string | null
}

export type BackendCustomerReceipt = {
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
    approval?: DocumentApprovalViewDto | null
}

export type BackendInvoiceAllocation = {
    id: string
    allocation_seq: number
    allocation_action: "apply" | "reverse"
    receivable_account_id: string
    allocated_gross_amount: string
    allocated_net_amount: string
    allocated_tax_amount: string
    reverses_allocation_id?: string | null
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
