/** W13 卡券票款复核 · 后端响应 DTO（/admin/… 接口原始结构）。 */

import type { WorkItemDto } from "@/features/work-items"

export type BackendReceivableEntry = {
    id: string
    entry_type: string
    direction: string
    amount: string
    due_date: string
    source_document_id: string
    posted_at: number
}

export type BackendFundsReview = {
    id: string
    review_no: number
    review_type: string
    review_result: string
    reviewed_by: string
    reviewed_at: number
    evidence_reference?: string | null
    subject_hash_at_review?: string | null
}

export type BackendReceiptFact = {
    receipt_id: string
    receipt_no: string
    received_at: string
    gross_amount: string
    allocated_to_account: string
    other_allocation_summary?: string | null
    reversed: boolean
}

export type BackendInvoiceFact = {
    invoice_id: string
    invoice_no: string
    direction: "BLUE" | "RED"
    issued_at: string
    gross_amount: string
    net_amount: string
    tax_amount: string
    allocated_to_account: string
    reversed: boolean
}

export type BackendReceivableAccount = {
    id: string
    sales_order_id: string
    source_sales_order_revision_id: string
    current_sales_order_revision_id: string
    sales_order_no: string
    sales_order_revision_no: number
    sales_order_snapshot_at: number
    account_seq: number
    customer_id: string
    customer_name: string
    counterparty_party_id: string
    counterparty_party_name?: string | null
    review_status: string
    gross_total: string
    settled_total: string
    open_total: string
    invoiceable_total: string
    invoiced_total: string
    open_invoiceable_total: string
    status: string
    version: number
    account_domain_version: string
    created_at: number
    entries: BackendReceivableEntry[]
    reviews: BackendFundsReview[]
    review_chain_tail_id?: string | null
    review_chain_version?: string | null
    next_review_no?: number | null
    funds_fact_version?: string | null
    receipt_facts?: BackendReceiptFact[] | null
    invoice_facts?: BackendInvoiceFact[] | null
    work_item?: WorkItemDto | null
    active_review_type?: "OPENING" | "SYNC_DELTA" | null
    allowed_actions?: Array<
        | "CONFIRM_ZERO"
        | "APPROVE"
        | "REJECT"
        | "REGISTER_RECEIPT"
        | "REGISTER_INVOICE"
    >
    action_blockers?: Array<{
        action: string
        code: string
        message: string
    }>
}

export type BackendCustomerReceipt = {
    id: string
    receipt_no: string
    status: string
    received_at: number
    amount: string
    allocated_total: string
    unallocated_amount: string
    allocations: Array<{
        id: string
        receivable_entry_id: string
        allocated_amount: string
        allocation_action: string
    }>
}

export type BackendInvoice = {
    id: string
    invoice_no: string
    invoice_kind: "blue" | "red"
    invoice_date: string
    gross_amount: string
    net_amount: string
    tax_amount: string
    allocated_total: string
    status: string
    allocations: Array<{
        id: string
        receivable_account_id?: string
        allocated_gross_amount: string
        allocation_action: string
    }>
}
