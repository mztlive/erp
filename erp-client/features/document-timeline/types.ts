export type TimelineCategory =
    | "sales"
    | "approval"
    | "procurement"
    | "fulfillment"
    | "receivable"
    | "payable"
    | "returns"
export type TimelineEvent = {
    id: string
    occurred_at: number
    category: TimelineCategory
    title: string
    actor_name: string
    document_type: string
    document_id: string
    document_number: string
    document_label: string
    href?: string | null
    details: string[]
    amount?: string | null
    amount_label?: string | null
    business_date?: string | null
    after_submission: boolean
}
export type TimelinePage = {
    items: TimelineEvent[]
    next_cursor?: string | null
    notice: string
}
export type TimelineSource = { id: string; number: string }
export type TaskTimeline = {
    timeline: TimelinePage
    sales_orders: TimelineSource[]
    submitted_at?: number | null
}
export type TimelineTarget = { kind: "sales" | "task" | "approval"; id: string }
export type TimelineFilter = {
    category?: TimelineCategory
    includeDetails?: boolean
    pageSize?: number
}
export type TimelineResponse = TimelinePage & {
    salesOrders?: TimelineSource[]
    submittedAt?: number | null
}

/** 工作台只为销售链路中的单据展示全流程入口。 */
export const timelineDocumentTypes = new Set([
    "sales_order",
    "voucher_sales_order",
    "sales_change_order",
    "purchase_order",
    "purchase_change_order",
    "purchase_receipt",
    "delivery",
    "customer_acceptance",
    "electronic_delivery",
    "service_fulfillment",
    "customer_receipt",
    "supplier_payment",
    "invoice",
    "sales_invoice_request",
    "purchase_payment_request",
    "customer_refund",
    "supplier_refund",
    "receipt_reversal",
    "payment_reversal",
    "sales_return_case",
    "purchase_return_order",
])
