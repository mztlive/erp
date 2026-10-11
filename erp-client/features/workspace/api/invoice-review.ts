import { apiGet } from "@/lib/api"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"

export type InvoiceApprovalReview = {
    read_at: number
    account_label: string
    gross_total: string
    invoiceable_total: string
    invoiced_total: string
    settled_total: string
    open_total: string
    amounts: {
        current_reserved: string
        other_pending: string
        other_approved: string
        available: string
        shortage: string
    }
    source: NonNullable<ApprovalMaterials["display"]["source_sales"]>[number]
    sales_status: string
    fulfillment_status: string
    sales_owner: string | null
    warnings: string[]
    requests: {
        request_no: string
        is_current: boolean
        amount: string
        invoiced_amount: string
        status: string
        created_at: number
    }[]
    records: {
        receipts: InvoiceReviewRecord[]
        invoices: InvoiceReviewRecord[]
        receipt_count: number
        invoice_count: number
    }
    request_count: number
}

export const fetchInvoiceApprovalReview = (instanceId: string) =>
    apiGet<InvoiceApprovalReview>(
        `/admin/approval-instances/${encodeURIComponent(instanceId)}/invoice-review`,
        { cache: "no-store" },
    )

export type InvoiceReviewRecord = {
    number: string
    date: string
    amount: string
    status: string
}
